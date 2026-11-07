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
    events: EventSink,
    pending: Vec<(Handle, Request)>,
}
impl MockEngine {
    pub fn new(events: EventSink) -> Self {
        Self {
            views: Registry::default(),
            events,
            pending: Vec::new(),
        }
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
                let view = self.views.insert(View {
                    urls: vec![url.clone()],
                    cursor: 0,
                })?;
                self.changed(view)?;
                Ok(Data::Handle(view))
            }
            Command::Close(view) => self.close(*view),
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
