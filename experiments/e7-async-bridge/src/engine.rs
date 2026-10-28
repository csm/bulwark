use crate::bridge::{Command, Data, EventSink, Fault, Request};
use anyhow::Result;
use std::collections::BTreeSet;

pub trait Backend {
    fn handle(&mut self, request: Request);
    fn tick(&mut self);
    fn frame_count(&self) -> u64 {
        0
    }
}

pub fn create(real_servo: bool, events: EventSink) -> Result<Box<dyn Backend>> {
    if real_servo {
        #[cfg(feature = "servo")]
        return Ok(Box::new(real::ServoBackend::new(events)?));
        #[cfg(not(feature = "servo"))]
        anyhow::bail!("--servo requires --features servo");
    }
    Ok(Box::new(Mock {
        views: BTreeSet::new(),
        next: 1,
        events,
        pending: Vec::new(),
    }))
}

struct Mock {
    views: BTreeSet<u64>,
    next: u64,
    events: EventSink,
    pending: Vec<(Request, u64, String)>,
}
impl Backend for Mock {
    fn handle(&mut self, request: Request) {
        let result = match &request.command {
            Command::Create(url) if url.starts_with("data:text/html,") => {
                let id = self.next;
                self.next += 1;
                self.views.insert(id);
                self.events.emit("page/created", id, String::new());
                self.events
                    .emit("page/ready", id, "Async channel fixture".into());
                Ok(Data::Id(id))
            }
            Command::Create(url) => Err(Fault::Engine(format!(
                "Mock only accepts HTML data URLs: {url}"
            ))),
            Command::Evaluate(view, script) if self.views.contains(view) => {
                let (view, script) = (*view, script.trim_matches('"').to_owned());
                self.pending.push((request, view, script));
                return;
            }
            Command::Close(view) if self.views.remove(view) => {
                self.events.emit("page/closed", *view, String::new());
                Ok(Data::Nil)
            }
            _ => Err(Fault::StaleHandle),
        };
        let _ = request.reply.send(result);
    }
    fn tick(&mut self) {
        // Reverse each batch to exercise correlation independently of completion order.
        while let Some((request, view, script)) = self.pending.pop() {
            let result = if self.views.contains(&view) {
                Ok(Data::Text(script))
            } else {
                Err(Fault::StaleHandle)
            };
            let _ = request.reply.send(result);
        }
    }
}

#[cfg(feature = "servo")]
mod real {
    use super::*;
    use servo::{
        EventLoopWaker, JSValue, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext,
        WebView, WebViewBuilder,
    };
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::Rc,
        thread,
    };
    use tokio::sync::oneshot;

    type Pending = Rc<RefCell<BTreeMap<u64, (u64, oneshot::Sender<Result<Data, Fault>>)>>>;

    #[derive(Clone)]
    struct Waker(thread::Thread);
    impl EventLoopWaker for Waker {
        fn clone_box(&self) -> Box<dyn EventLoopWaker> {
            Box::new(self.clone())
        }
        fn wake(&self) {
            self.0.unpark();
        }
    }
    struct Delegate {
        id: u64,
        events: EventSink,
        frames: Rc<Cell<u64>>,
    }
    impl servo::WebViewDelegate for Delegate {
        fn notify_load_status_changed(&self, view: WebView, status: servo::LoadStatus) {
            if status == servo::LoadStatus::Complete {
                self.events
                    .emit("page/ready", self.id, view.page_title().unwrap_or_default());
            }
        }
        fn notify_new_frame_ready(&self, view: WebView) {
            self.frames.set(self.frames.get() + 1);
            view.paint();
        }
    }

    pub struct ServoBackend {
        // Views and pending evaluations drop before their Servo and context.
        views: BTreeMap<u64, WebView>,
        pending: Pending,
        servo: Servo,
        context: Rc<SoftwareRenderingContext>,
        events: EventSink,
        next: u64,
        frames: Rc<Cell<u64>>,
    }
    impl ServoBackend {
        pub fn new(events: EventSink) -> Result<Self> {
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
            let context = Rc::new(
                SoftwareRenderingContext::new((640, 480).into())
                    .map_err(|error| anyhow::anyhow!("Software context: {error:?}"))?,
            );
            context
                .make_current()
                .map_err(|error| anyhow::anyhow!("GL context: {error:?}"))?;
            let servo = ServoBuilder::default()
                .event_loop_waker(Box::new(Waker(thread::current())))
                .build();
            Ok(Self {
                views: BTreeMap::new(),
                pending: Rc::default(),
                servo,
                context,
                events,
                next: 1,
                frames: Rc::default(),
            })
        }
    }
    impl Backend for ServoBackend {
        fn handle(&mut self, request: Request) {
            let result = match request.command {
                Command::Create(url) => match url::Url::parse(&url) {
                    Ok(url) => {
                        let id = self.next;
                        self.next += 1;
                        let view = WebViewBuilder::new(&self.servo, self.context.clone())
                            .url(url)
                            .delegate(Rc::new(Delegate {
                                id,
                                events: self.events.clone(),
                                frames: self.frames.clone(),
                            }))
                            .build();
                        view.show();
                        self.views.insert(id, view);
                        self.events.emit("page/created", id, String::new());
                        Ok(Data::Id(id))
                    }
                    Err(error) => Err(Fault::Engine(error.to_string())),
                },
                Command::Evaluate(view, script) => {
                    if let Some(webview) = self.views.get(&view) {
                        let id = request.id;
                        self.pending.borrow_mut().insert(id, (view, request.reply));
                        let pending = self.pending.clone();
                        webview.evaluate_javascript(script, move |result| {
                            if let Some((_, reply)) = pending.borrow_mut().remove(&id) {
                                let result = match result {
                                    Ok(JSValue::String(text)) => Ok(Data::Text(text)),
                                    Ok(value) => Err(Fault::Engine(format!(
                                        "Probe requires a JS string, got {value:?}"
                                    ))),
                                    Err(error) => Err(Fault::Engine(format!("{error:?}"))),
                                };
                                let _ = reply.send(result);
                            }
                        });
                        self.servo.spin_event_loop();
                        return;
                    }
                    Err(Fault::StaleHandle)
                }
                Command::Close(view) if self.views.remove(&view).is_some() => {
                    let ids: Vec<_> = self
                        .pending
                        .borrow()
                        .iter()
                        .filter_map(|(id, (owner, _))| (*owner == view).then_some(*id))
                        .collect();
                    for id in ids {
                        if let Some((_, reply)) = self.pending.borrow_mut().remove(&id) {
                            let _ = reply.send(Err(Fault::StaleHandle));
                        }
                    }
                    self.events.emit("page/closed", view, String::new());
                    Ok(Data::Nil)
                }
                _ => Err(Fault::StaleHandle),
            };
            let _ = request.reply.send(result);
            self.servo.spin_event_loop();
        }
        fn tick(&mut self) {
            self.pending
                .borrow_mut()
                .retain(|_, (_, reply)| !reply.is_closed());
            self.servo.spin_event_loop();
        }
        fn frame_count(&self) -> u64 {
            self.frames.get()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{mpsc, oneshot};

    #[tokio::test(flavor = "current_thread")]
    async fn closing_a_view_invalidates_an_outstanding_evaluation() {
        let (events, _receiver) = mpsc::channel(4);
        let mut engine = Mock {
            views: BTreeSet::new(),
            next: 1,
            events: EventSink::new(events),
            pending: Vec::new(),
        };
        let (reply, created) = oneshot::channel();
        engine.handle(Request {
            id: 1,
            command: Command::Create("data:text/html,test".into()),
            reply,
        });
        assert!(matches!(created.await.unwrap(), Ok(Data::Id(1))));
        let (reply, evaluated) = oneshot::channel();
        engine.handle(Request {
            id: 2,
            command: Command::Evaluate(1, "\"test\"".into()),
            reply,
        });
        let (reply, closed) = oneshot::channel();
        engine.handle(Request {
            id: 3,
            command: Command::Close(1),
            reply,
        });
        engine.tick();
        assert!(matches!(closed.await.unwrap(), Ok(Data::Nil)));
        assert!(matches!(evaluated.await.unwrap(), Err(Fault::StaleHandle)));
    }
}
