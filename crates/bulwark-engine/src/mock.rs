//! Deterministic test engine. Evaluation accepts only a quoted string literal;
//! this backend does not implement JavaScript or render web content.
use crate::{
    Command, Data, Engine, EventKind, Fault, Handle, LoadStatus, Registry, ViewInfo,
    bridge::{EventSink, Request},
};

struct View {
    urls: Vec<String>,
    cursor: usize,
}
impl View {
    fn info(&self) -> ViewInfo {
        ViewInfo {
            url: self.urls[self.cursor].clone(),
            title: "Bulwark mock".into(),
            load_status: LoadStatus::Complete,
            can_go_back: self.cursor > 0,
            can_go_forward: self.cursor + 1 < self.urls.len(),
        }
    }
}
pub struct MockEngine {
    views: Registry<View>,
    selected: Option<Handle>,
    ui: Option<crate::ui::Ui>,
    events: EventSink,
    pending: Vec<(Handle, Request)>,
}
impl MockEngine {
    pub fn new(events: EventSink) -> Self {
        Self {
            views: Registry::default(),
            selected: None,
            ui: None,
            events,
            pending: Vec::new(),
        }
    }
    /// The presented UI description.
    pub fn ui(&self) -> Option<&crate::ui::Ui> {
        self.ui.as_ref()
    }
    fn changed(&self, view: Handle) -> Result<(), Fault> {
        let info = self.views.get(view)?.info();
        for event in [
            EventKind::UrlChanged(info.url),
            EventKind::TitleChanged(info.title),
            EventKind::LoadStatusChanged(info.load_status),
            EventKind::HistoryChanged {
                can_go_back: info.can_go_back,
                can_go_forward: info.can_go_forward,
            },
        ] {
            self.events.emit(view, event)?;
        }
        Ok(())
    }
    fn close(&mut self, view: Handle) -> Result<Data, Fault> {
        self.views.remove(view)?;
        if self.selected == Some(view) {
            self.selected = None;
        }
        let mut remaining = Vec::new();
        for (owner, request) in self.pending.drain(..) {
            if owner == view {
                request.complete(Err(Fault::StaleHandle));
            } else {
                remaining.push((owner, request));
            }
        }
        self.pending = remaining;
        self.events.emit(view, EventKind::Closed)?;
        Ok(Data::Nil)
    }
}
impl Engine for MockEngine {
    fn handle(&mut self, request: Request) {
        if request.cancelled() {
            request.complete(Err(Fault::Timeout));
            return;
        }
        let result = (|| match &request.command {
            Command::Create { url } => {
                let first = self.views.iter().next().is_none();
                let view = self.views.insert(View {
                    urls: vec![url.clone()],
                    cursor: 0,
                })?;
                // A UI description decides visibility once one exists.
                if first && self.ui.is_none() {
                    self.selected = Some(view);
                }
                self.changed(view)?;
                Ok(Data::Handle(view))
            }
            Command::Close(view) => self.close(*view),
            Command::Select(view) => {
                if let Some(view) = view {
                    self.views.get(*view)?;
                }
                self.selected = *view;
                Ok(Data::Nil)
            }
            Command::SetUi(ui) => {
                // Validate before any change, so a rejected description changes nothing.
                ui.check_revision(self.ui.as_ref())?;
                let web = ui.web().map(|(view, _)| view);
                if let Some(view) = web {
                    self.views.get(view)?;
                }
                self.selected = web;
                self.ui = Some(ui.clone());
                Ok(Data::Nil)
            }
            Command::Navigate { view, url } => {
                let state = self.views.get_mut(*view)?;
                state.urls.truncate(state.cursor + 1);
                state.urls.push(url.clone());
                state.cursor += 1;
                self.changed(*view)?;
                Ok(Data::Nil)
            }
            Command::Reload(view) => {
                self.changed(*view)?;
                Ok(Data::Nil)
            }
            Command::Back(view) | Command::Forward(view) => {
                let state = self.views.get_mut(*view)?;
                if matches!(request.command, Command::Back(_)) {
                    state.cursor = state.cursor.saturating_sub(1);
                } else {
                    state.cursor = (state.cursor + 1).min(state.urls.len() - 1);
                }
                self.changed(*view)?;
                Ok(Data::Nil)
            }
            Command::Info(view) => Ok(Data::Info(self.views.get(*view)?.info())),
            Command::Evaluate { view, .. } => {
                self.views.get(*view)?;
                Ok(Data::Nil)
            }
            _ => Err(Fault::Engine("Shell command sent to engine".into())),
        })();
        if let Command::Evaluate { view, .. } = &request.command
            && result.is_ok()
        {
            self.pending.push((*view, request));
        } else {
            request.complete(result);
        }
    }
    fn tick(&mut self) {
        // Reverse order exercises one-shot correlation independently of order.
        while let Some((view, request)) = self.pending.pop() {
            let result = if request.cancelled() {
                Err(Fault::Timeout)
            } else if self.views.get(view).is_err() {
                Err(Fault::StaleHandle)
            } else if let Command::Evaluate { script, .. } = &request.command {
                script
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .map(|s| Data::Text(s.to_owned()))
                    .ok_or_else(|| {
                        Fault::Engine("Mock eval-js requires a quoted string literal".into())
                    })
            } else {
                unreachable!()
            };
            request.complete(result);
        }
    }
    fn shutdown(&mut self) {
        self.selected = None;
        self.views.clear();
        for (_, request) in self.pending.drain(..) {
            request.complete(Err(Fault::ShuttingDown));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{DEFAULT_TIMEOUT, channel};
    use std::sync::Arc;

    #[tokio::test]
    async fn selection_preserves_other_views_and_stale_selection_changes_nothing() {
        let (runtime, mut endpoint) = channel(Arc::new(|| {}));
        let mut engine = MockEngine::new(endpoint.events.clone());
        let mut views = Vec::new();
        for url in ["about:first", "about:second"] {
            let pending = runtime
                .client
                .submit(Command::Create { url: url.into() }, DEFAULT_TIMEOUT)
                .unwrap();
            endpoint.admit(|r| engine.handle(r));
            let Data::Handle(view) = pending.wait().await.unwrap() else {
                panic!("Expected handle")
            };
            views.push(view);
        }
        assert_eq!(engine.selected, Some(views[0]));
        for (command, selected) in [
            (Command::Select(Some(views[1])), Some(views[1])),
            (Command::Close(views[0]), Some(views[1])),
            (Command::Select(None), None),
            (Command::Select(Some(views[1])), Some(views[1])),
        ] {
            let pending = runtime.client.submit(command, DEFAULT_TIMEOUT).unwrap();
            endpoint.admit(|r| engine.handle(r));
            assert_eq!(pending.wait().await, Ok(Data::Nil));
            assert_eq!(engine.selected, selected);
        }
        let stale = runtime
            .client
            .submit(Command::Select(Some(views[0])), DEFAULT_TIMEOUT)
            .unwrap();
        endpoint.admit(|r| engine.handle(r));
        assert_eq!(stale.wait().await, Err(Fault::StaleHandle));
        assert_eq!(engine.selected, Some(views[1]));
        assert_eq!(
            engine.views.get(views[1]).unwrap().info().url,
            "about:second"
        );
    }

    #[tokio::test]
    async fn ui_descriptions_select_the_web_element_and_reject_stale_input() {
        use crate::ui::{Element, Rect, Ui, Widget};
        let (runtime, mut endpoint) = channel(Arc::new(|| {}));
        let mut engine = MockEngine::new(endpoint.events.clone());
        let rect = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let describe = |revision, view: Option<Handle>| {
            let widget = view.map_or(Widget::Text("text".into()), Widget::Web);
            Command::SetUi(
                Ui::new(
                    revision,
                    Some(1),
                    vec![Element {
                        id: Some(1),
                        rect,
                        widget,
                    }],
                )
                .unwrap(),
            )
        };
        let mut views = Vec::new();
        for url in ["about:a", "about:b"] {
            let pending = runtime
                .client
                .submit(Command::Create { url: url.into() }, DEFAULT_TIMEOUT)
                .unwrap();
            endpoint.admit(|r| engine.handle(r));
            let Data::Handle(view) = pending.wait().await.unwrap() else {
                panic!("Expected handle")
            };
            views.push(view);
        }
        for (command, result, selected, revision) in [
            (
                describe(2, Some(views[1])),
                Ok(Data::Nil),
                Some(views[1]),
                2,
            ),
            (
                describe(1, Some(views[0])),
                Err(Fault::StaleRevision),
                Some(views[1]),
                2,
            ),
            (
                describe(2, Some(views[0])),
                Err(Fault::StaleRevision),
                Some(views[1]),
                2,
            ),
            (
                describe(3, Some(Handle(u64::MAX >> 1))),
                Err(Fault::StaleHandle),
                Some(views[1]),
                2,
            ),
            (describe(3, None), Ok(Data::Nil), None, 3),
            (
                Command::Create {
                    url: "about:c".into(),
                },
                Ok(Data::Nil),
                None,
                3,
            ),
        ] {
            let creating = matches!(command, Command::Create { .. });
            let pending = runtime.client.submit(command, DEFAULT_TIMEOUT).unwrap();
            endpoint.admit(|r| engine.handle(r));
            let reply = pending.wait().await;
            if creating {
                assert!(matches!(reply, Ok(Data::Handle(_))));
            } else {
                assert_eq!(reply, result);
            }
            assert_eq!(
                engine.selected, selected,
                "A described frame decides visibility"
            );
            assert_eq!(engine.ui().unwrap().revision(), revision);
        }
    }

    #[tokio::test]
    async fn close_invalidates_pending_evaluation_and_stale_handle() {
        let (runtime, mut endpoint) = channel(Arc::new(|| {}));
        let mut engine = MockEngine::new(endpoint.events.clone());
        let created = runtime
            .client
            .submit(
                Command::Create {
                    url: "about:blank".into(),
                },
                DEFAULT_TIMEOUT,
            )
            .unwrap();
        endpoint.admit(|r| engine.handle(r));
        let Data::Handle(view) = created.wait().await.unwrap() else {
            panic!("Expected a handle");
        };
        let script = runtime
            .client
            .submit(
                Command::Evaluate {
                    view,
                    script: "\"late\"".into(),
                },
                DEFAULT_TIMEOUT,
            )
            .unwrap();
        let close = runtime
            .client
            .submit(Command::Close(view), DEFAULT_TIMEOUT)
            .unwrap();
        endpoint.admit(|r| engine.handle(r));
        assert_eq!(close.wait().await, Ok(Data::Nil));
        assert_eq!(script.wait().await, Err(Fault::StaleHandle));
        let stale = runtime
            .client
            .submit(Command::Info(view), DEFAULT_TIMEOUT)
            .unwrap();
        endpoint.admit(|r| engine.handle(r));
        assert_eq!(stale.wait().await, Err(Fault::StaleHandle));
    }
}
