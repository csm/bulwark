use bulwark_engine::{
    Command, Data, Engine, EventKind, Fault, Handle, LoadStatus, Registry, ViewInfo,
    bridge::{EventSink, Request, Wake},
};
use servo::{
    EventLoopWaker, JSValue, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext,
    WebView, WebViewBuilder, WebViewDelegate,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

#[derive(Clone)]
struct Waker(Wake);
impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
    fn wake(&self) {
        (self.0)();
    }
}

#[derive(Default)]
struct ViewState {
    revoked: Cell<bool>,
    dirty: Cell<bool>,
}
struct View {
    webview: WebView,
    state: Rc<ViewState>,
}
struct Delegate {
    handle: Handle,
    state: Rc<ViewState>,
    events: EventSink,
    wake: Wake,
}
impl Delegate {
    fn emit(&self, kind: EventKind) {
        if !self.state.revoked.get() {
            // EventSink stores fatal overflow outside the queue and wakes the host.
            let _ = self.events.emit(self.handle, kind);
        }
    }
    fn revoke(&self, kind: EventKind) {
        self.emit(kind);
        self.state.revoked.set(true);
        (self.wake)();
    }
    fn frame_ready(&self) {
        // Servo can repeat this callback until paint. Wake only on a transition
        // so an occluded native window does not spin on duplicate proxy events.
        if !self.state.revoked.get() && !self.state.dirty.replace(true) {
            (self.wake)();
        }
    }
}
impl WebViewDelegate for Delegate {
    fn notify_url_changed(&self, _: WebView, url: url::Url) {
        self.emit(EventKind::UrlChanged(url.into()));
    }
    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        self.emit(EventKind::TitleChanged(title.unwrap_or_default()));
    }
    fn notify_load_status_changed(&self, _: WebView, status: servo::LoadStatus) {
        self.emit(EventKind::LoadStatusChanged(load_status(status)));
    }
    fn notify_history_changed(&self, view: WebView, _: Vec<url::Url>, _: usize) {
        self.emit(EventKind::HistoryChanged {
            can_go_back: view.can_go_back(),
            can_go_forward: view.can_go_forward(),
        });
    }
    fn notify_closed(&self, _: WebView) {
        self.revoke(EventKind::Closed);
    }
    fn notify_crashed(&self, _: WebView, reason: String, _: Option<String>) {
        self.revoke(EventKind::Crashed(reason));
    }
    fn notify_new_frame_ready(&self, _: WebView) {
        self.frame_ready();
    }
}

fn load_status(status: servo::LoadStatus) -> LoadStatus {
    match status {
        servo::LoadStatus::Started => LoadStatus::Started,
        servo::LoadStatus::HeadParsed => LoadStatus::HeadParsed,
        servo::LoadStatus::Complete => LoadStatus::Complete,
    }
}

type Pending = Rc<RefCell<BTreeMap<u64, (Handle, Request)>>>;

/// Owns Servo, its views, and deferred replies on the creating thread.
/// The initial adapter accepts one live view per context (the Phase 0 fallback).
pub struct ServoEngine {
    // Resources and callbacks drop before Servo and its rendering context.
    views: Registry<View>,
    pending: Pending,
    servo: Servo,
    context: Rc<dyn RenderingContext>,
    events: EventSink,
    wake: Wake,
    stopped: bool,
    frames: u64,
    scale: f32,
    focused: bool,
}
impl ServoEngine {
    /// Creates a software surface. The host must drive ticks and call paint.
    pub fn software(events: EventSink, wake: Wake) -> Result<Self, Fault> {
        let context = SoftwareRenderingContext::new((640, 480).into())
            .map_err(|error| Fault::Engine(format!("Software context: {error:?}")))?;
        Self::with_context(Rc::new(context), events, wake)
    }

    /// Binds an embedder-owned context. All calls must stay on this thread.
    pub fn with_context(
        context: Rc<dyn RenderingContext>,
        events: EventSink,
        wake: Wake,
    ) -> Result<Self, Fault> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        context
            .make_current()
            .map_err(|error| Fault::Engine(format!("GL context: {error:?}")))?;
        let servo = ServoBuilder::default()
            .event_loop_waker(Box::new(Waker(wake.clone())))
            .build();
        Ok(Self {
            views: Registry::default(),
            pending: Rc::default(),
            servo,
            context,
            events,
            wake,
            stopped: false,
            frames: 0,
            scale: 1.0,
            focused: true,
        })
    }

    fn view(&self, handle: Handle) -> Result<&View, Fault> {
        self.views.get(handle).and_then(|view| {
            if view.state.revoked.get() {
                Err(Fault::StaleHandle)
            } else {
                Ok(view)
            }
        })
    }

    fn create(&mut self, url: &str) -> Result<Data, Fault> {
        let url = url::Url::parse(url).map_err(|error| Fault::Engine(error.to_string()))?;
        if self.views.iter().next().is_some() {
            return Err(Fault::Engine(
                "One live webview per rendering context is supported".into(),
            ));
        }
        let handle = self.views.insert_with(|handle| {
            let state = Rc::new(ViewState {
                dirty: Cell::new(true),
                ..Default::default()
            });
            let webview = WebViewBuilder::new(&self.servo, self.context.clone())
                .url(url)
                .hidpi_scale_factor(euclid::Scale::new(self.scale))
                .delegate(Rc::new(Delegate {
                    handle,
                    state: state.clone(),
                    events: self.events.clone(),
                    wake: self.wake.clone(),
                }))
                .build();
            webview.show();
            webview.set_focused(self.focused);
            View { webview, state }
        })?;
        Ok(Data::Handle(handle))
    }

    fn invalidate(&mut self, handle: Handle) -> Result<(), Fault> {
        let view = self.views.remove(handle)?;
        // Suppress late delegate callbacks before dropping the final embedder reference.
        view.state.revoked.set(true);
        self.reject_pending(Some(handle), Fault::StaleHandle);
        Ok(())
    }

    fn reject_pending(&self, owner: Option<Handle>, fault: Fault) {
        let ids: Vec<_> = self
            .pending
            .borrow()
            .iter()
            .filter_map(|(id, (view, _))| owner.is_none_or(|owner| owner == *view).then_some(*id))
            .collect();
        for id in ids {
            if let Some((_, request)) = self.pending.borrow_mut().remove(&id) {
                request.complete(Err(fault.clone()));
            }
        }
    }

    /// Native host access on the engine thread; no Servo value crosses the runtime bridge.
    pub fn active_webview(&self) -> Option<(Handle, WebView)> {
        self.views.iter().find_map(|(handle, view)| {
            (!view.state.revoked.get()).then(|| (handle, view.webview.clone()))
        })
    }

    /// Whether a live view needs a new frame, including its initial paint.
    pub fn needs_repaint(&self) -> bool {
        self.views
            .iter()
            .any(|(_, view)| !view.state.revoked.get() && view.state.dirty.get())
    }

    /// Persist native scale and focus so replacement views inherit the window state.
    pub fn set_window_state(&mut self, scale: f32, focused: bool) {
        if self.scale != scale {
            self.scale = scale;
            if let Some((_, view)) = self.active_webview() {
                view.set_hidpi_scale_factor(euclid::Scale::new(scale));
            }
        }
        if self.focused != focused {
            self.focused = focused;
            if let Some((_, view)) = self.active_webview() {
                view.set_focused(focused);
            }
        }
    }

    /// Paint into the supplied context without presenting the native window.
    /// The host can composite chrome before its single buffer swap.
    /// Force repaint for native exposure events even if no new frame is pending.
    pub fn paint_frame(&mut self, force: bool) -> bool {
        for (_, view) in self.views.iter() {
            if !view.state.revoked.get() && (view.state.dirty.replace(false) || force) {
                view.webview.paint();
                self.frames += 1;
                return true;
            }
        }
        false
    }

    /// Paint and present a dirty frame for hosts without chrome.
    pub fn paint(&mut self) -> bool {
        let painted = self.paint_frame(false);
        if painted {
            self.context.present();
        }
        painted
    }
    pub fn frame_count(&self) -> u64 {
        self.frames
    }
}

impl Engine for ServoEngine {
    fn handle(&mut self, request: Request) {
        if self.stopped {
            request.complete(Err(Fault::ShuttingDown));
            return;
        }
        if request.cancelled() {
            request.complete(Err(Fault::Timeout));
            return;
        }
        let result = (|| match &request.command {
            Command::Create { url } => self.create(url),
            Command::Close(handle) => {
                self.view(*handle)?;
                self.invalidate(*handle)?;
                self.events.emit(*handle, EventKind::Closed)?;
                Ok(Data::Nil)
            }
            Command::Navigate { view, url } => {
                let view = self.view(*view)?;
                let url = url::Url::parse(url).map_err(|error| Fault::Engine(error.to_string()))?;
                view.webview.load(url);
                Ok(Data::Nil)
            }
            Command::Reload(handle) => {
                self.view(*handle)?.webview.reload();
                Ok(Data::Nil)
            }
            Command::Back(handle) => {
                self.view(*handle)?.webview.go_back(1);
                Ok(Data::Nil)
            }
            Command::Forward(handle) => {
                self.view(*handle)?.webview.go_forward(1);
                Ok(Data::Nil)
            }
            Command::Info(handle) => {
                let view = &self.view(*handle)?.webview;
                Ok(Data::Info(ViewInfo {
                    url: view.url().map(String::from).unwrap_or_default(),
                    title: view.page_title().unwrap_or_default(),
                    load_status: load_status(view.load_status()),
                    can_go_back: view.can_go_back(),
                    can_go_forward: view.can_go_forward(),
                }))
            }
            Command::Evaluate { view, .. } => {
                self.view(*view)?;
                Ok(Data::Nil)
            }
            _ => Err(Fault::Engine("Shell command sent to engine".into())),
        })();
        if let Command::Evaluate { view, script } = &request.command
            && result.is_ok()
        {
            let (view, script, id) = (*view, script.clone(), request.id);
            self.pending.borrow_mut().insert(id, (view, request));
            let pending = self.pending.clone();
            let state = self.view(view).expect("Validated view").state.clone();
            self.view(view)
                .expect("Validated view")
                .webview
                .evaluate_javascript(script, move |result| {
                    let entry = pending.borrow_mut().remove(&id);
                    if let Some((_, request)) = entry {
                        let result = if state.revoked.get() {
                            Err(Fault::StaleHandle)
                        } else if request.cancelled() {
                            Err(Fault::Timeout)
                        } else {
                            match result {
                                Ok(JSValue::String(text)) => Ok(Data::Text(text)),
                                Ok(_) => Err(Fault::Engine(
                                    "eval-js requires a JavaScript string result".into(),
                                )),
                                Err(error) => Err(Fault::Engine(format!("{error:?}"))),
                            }
                        };
                        request.complete(result);
                    }
                });
        } else {
            request.complete(result);
        }
        // Tick owns event-loop progress, after the bounded admission turn.
    }

    fn tick(&mut self) {
        if self.stopped {
            return;
        }
        let cancelled: Vec<_> = self
            .pending
            .borrow()
            .iter()
            .filter_map(|(id, (_, request))| request.cancelled().then_some(*id))
            .collect();
        for id in cancelled {
            if let Some((_, request)) = self.pending.borrow_mut().remove(&id) {
                request.complete(Err(Fault::Timeout));
            }
        }
        self.servo.spin_event_loop();
        let revoked: Vec<_> = self
            .views
            .iter()
            .filter_map(|(id, view)| view.state.revoked.get().then_some(id))
            .collect();
        for handle in revoked {
            let _ = self.invalidate(handle);
        }
    }

    fn shutdown(&mut self) {
        self.stopped = true;
        for (_, view) in self.views.iter() {
            view.state.revoked.set(true);
        }
        self.reject_pending(None, Fault::ShuttingDown);
        self.views.clear();
    }
}
impl Drop for ServoEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn repeated_frame_notifications_wake_once_until_paint_and_never_after_revocation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (_, endpoint) = bulwark_engine::bridge::channel(Arc::new(|| {}));
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let delegate = Delegate {
            handle: Handle(1),
            state: Rc::default(),
            events: endpoint.events,
            wake: Arc::new(move || {
                count.fetch_add(1, Ordering::Relaxed);
            }),
        };
        delegate.frame_ready();
        delegate.frame_ready();
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        delegate.state.dirty.set(false);
        delegate.frame_ready();
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        delegate.state.revoked.set(true);
        delegate.state.dirty.set(false);
        delegate.frame_ready();
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert!(!delegate.state.dirty.get());
    }

    #[test]
    fn terminal_event_revokes_callbacks_and_suppresses_late_events() {
        let (mut runtime, endpoint) = bulwark_engine::bridge::channel(Arc::new(|| {}));
        let delegate = Delegate {
            handle: Handle(1),
            state: Rc::default(),
            events: endpoint.events,
            wake: Arc::new(|| {}),
        };
        delegate.revoke(EventKind::Crashed("expected crash".into()));
        assert!(delegate.state.revoked.get());
        delegate.emit(EventKind::TitleChanged("late title".into()));
        delegate.revoke(EventKind::Closed);
        assert_eq!(
            runtime.events.try_recv().unwrap().kind,
            EventKind::Crashed("expected crash".into())
        );
        assert!(runtime.events.try_recv().is_err());
        assert!(runtime.control.error().is_none());
    }
}
