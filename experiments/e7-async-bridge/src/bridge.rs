//! Only these owned messages cross the engine/runtime thread boundary.
use cljrs_gc::GcPtr;
use cljrs_runtime::env::env::GlobalEnv;
use cljrs_value::{Arity, Keyword, MapValue, NativeFn, Value, ValueError};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    thread::Thread,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};

pub const CAPACITY: usize = 2;
pub const IN_FLIGHT: usize = 8;

#[derive(Debug)]
pub enum Command {
    Create(String),
    Evaluate(u64, String),
    Close(u64),
    StallBegin,
    StallEnd,
}

#[derive(Debug)]
pub enum Data {
    Id(u64),
    Text(String),
    Count(u64),
    Nil,
}

#[derive(Debug)]
pub enum Fault {
    Busy,
    Disconnected,
    ReplyDropped,
    Timeout,
    StaleHandle,
    Engine(String),
}
impl Fault {
    fn kind(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Disconnected => "engine-disconnected",
            Self::ReplyDropped => "reply-dropped",
            Self::Timeout => "timeout",
            Self::StaleHandle => "stale-handle",
            Self::Engine(_) => "engine-error",
        }
    }
}

#[derive(Debug)]
pub struct Request {
    pub id: u64,
    pub command: Command,
    pub reply: oneshot::Sender<Result<Data, Fault>>,
}

#[derive(Debug)]
pub struct Event {
    pub sequence: u64,
    pub kind: &'static str,
    pub view: u64,
    pub title: String,
}

#[derive(Clone)]
pub struct EventSink {
    sender: mpsc::Sender<Event>,
    next: Rc<Cell<u64>>,
    error: Rc<RefCell<Option<String>>>,
}
impl EventSink {
    pub fn new(sender: mpsc::Sender<Event>) -> Self {
        Self {
            sender,
            next: Rc::new(Cell::new(1)),
            error: Rc::default(),
        }
    }
    pub fn emit(&self, kind: &'static str, view: u64, title: String) {
        let sequence = self.next.get();
        self.next.set(sequence + 1);
        if let Err(error) = self.sender.try_send(Event {
            sequence,
            kind,
            view,
            title,
        }) {
            // Servo delegates cannot await. This probe fails explicitly instead
            // of silently dropping lifecycle events or blocking the engine.
            *self.error.borrow_mut() = Some(format!("Event delivery failed: {error}"));
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.borrow_mut().take()
    }
}

fn keyword(name: &str) -> Value {
    let keyword = match name.split_once('/') {
        Some((ns, name)) => Keyword::qualified(ns, name),
        None => Keyword::simple(name),
    };
    Value::keyword(keyword)
}
fn map(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(MapValue::from_pairs(
        pairs.into_iter().map(|(k, v)| (keyword(k), v)).collect(),
    ))
}
fn response(id: u64, result: Result<Data, Fault>) -> Value {
    let mut fields = vec![("request/id", Value::Long(id as i64))];
    match result {
        Ok(data) => fields.extend([
            ("ok", Value::Bool(true)),
            (
                "value",
                match data {
                    Data::Id(id) | Data::Count(id) => Value::Long(id as i64),
                    Data::Text(text) => Value::string(text),
                    Data::Nil => Value::Nil,
                },
            ),
        ]),
        Err(error) => {
            fields.extend([("ok", Value::Bool(false)), ("error", keyword(error.kind()))]);
            if let Fault::Engine(message) = error {
                fields.push(("message", Value::string(message)));
            }
        }
    }
    map(fields)
}

struct Bridge {
    sender: mpsc::Sender<Request>,
    engine_thread: Thread,
    next: Cell<u64>,
    permits: Arc<Semaphore>,
    latencies: Rc<RefCell<Vec<Duration>>>,
    timeout: Duration,
}
impl Bridge {
    fn request(&self, command: Command) -> Value {
        let id = self.next.get();
        self.next.set(id + 1);
        let permit = self.permits.clone().try_acquire_owned();
        let sender = self.sender.clone();
        let wake = self.engine_thread.clone();
        let latencies = self.latencies.clone();
        let timeout = self.timeout;
        cljrs_async::spawn_future(async move {
            let Ok(_permit) = permit else {
                return Ok(response(id, Err(Fault::Busy)));
            };
            let started = Instant::now();
            let (reply, result) = oneshot::channel();
            let request = Request { id, command, reply };
            let operation = async move {
                sender
                    .send(request)
                    .await
                    .map_err(|_| Fault::Disconnected)?;
                wake.unpark();
                result.await.map_err(|_| Fault::ReplyDropped)?
            };
            let result = tokio::time::timeout(timeout, operation)
                .await
                .unwrap_or(Err(Fault::Timeout));
            latencies.borrow_mut().push(started.elapsed());
            Ok(response(id, result))
        })
    }
}

/// Install future-returning native mechanisms. Captures contain no GC values.
pub fn install(
    globals: &Arc<GlobalEnv>,
    sender: mpsc::Sender<Request>,
    events: mpsc::Receiver<Event>,
    engine_thread: Thread,
    latencies: Rc<RefCell<Vec<Duration>>>,
    real_servo: bool,
) {
    let bridge = Rc::new(Bridge {
        sender,
        engine_thread,
        next: Cell::new(1),
        permits: Arc::new(Semaphore::new(IN_FLIGHT)),
        latencies,
        timeout: Duration::from_secs(5),
    });
    let request = NativeFn::with_closure(
        "bulwark.experiment.native/request",
        Arity::Fixed(3),
        move |args| {
            let (Value::Str(op), Value::Long(view), Value::Str(payload)) =
                (&args[0], &args[1], &args[2])
            else {
                return Err(ValueError::Other(
                    "request requires operation string, non-negative handle, payload string".into(),
                ));
            };
            let view =
                u64::try_from(*view).map_err(|_| ValueError::Other("negative handle".into()))?;
            let command = match op.get().as_str() {
                "create" => Command::Create(payload.get().clone()),
                "eval-js" => Command::Evaluate(view, payload.get().clone()),
                "close" => Command::Close(view),
                "stall-begin" => Command::StallBegin,
                "stall-end" => Command::StallEnd,
                _ => {
                    return Err(ValueError::Other(format!(
                        "Unknown operation: {}",
                        op.get()
                    )));
                }
            };
            Ok(bridge.request(command))
        },
    );
    let events = Rc::new(Mutex::new(events));
    let next_event = NativeFn::with_closure(
        "bulwark.experiment.native/next-event",
        Arity::Fixed(0),
        move |_| {
            let events = events.clone();
            Ok(cljrs_async::spawn_future(async move {
                Ok(match events.lock().await.recv().await {
                    Some(event) => map(vec![
                        ("sequence", Value::Long(event.sequence as i64)),
                        ("event/type", keyword(event.kind)),
                        ("webview", Value::Long(event.view as i64)),
                        ("title", Value::string(event.title)),
                    ]),
                    None => Value::Nil,
                })
            }))
        },
    );
    for (name, native) in [("request", request), ("next-event", next_event)] {
        globals.intern(
            "bulwark.experiment.native",
            name.into(),
            Value::NativeFunction(GcPtr::new(native)),
        );
    }
    globals.intern(
        "bulwark.experiment.native",
        "servo?".into(),
        Value::Bool(real_servo),
    );
    globals.mark_loaded("bulwark.experiment.native");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_owned_send_data_crosses_the_boundary() {
        fn require_send<T: Send>() {}
        require_send::<Request>();
        require_send::<Event>();
        require_send::<Result<Data, Fault>>();
    }

    #[test]
    fn event_overload_is_an_explicit_error() {
        let (sender, _receiver) = mpsc::channel(1);
        let sink = EventSink::new(sender);
        sink.emit("page/created", 1, String::new());
        sink.emit("page/closed", 1, String::new());
        assert!(sink.take_error().is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_and_dropped_reply_settle_without_blocking() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (sender, receiver) = mpsc::channel(1);
                let bridge = Bridge {
                    sender,
                    engine_thread: std::thread::current(),
                    next: Cell::new(1),
                    permits: Arc::new(Semaphore::new(IN_FLIGHT)),
                    latencies: Rc::default(),
                    timeout: Duration::from_secs(5),
                };
                drop(receiver);
                let result = cljrs_async::await_value(bridge.request(Command::Close(1)))
                    .await
                    .unwrap();
                assert!(result.to_string().contains(":engine-disconnected"));

                let (sender, mut receiver) = mpsc::channel(1);
                let bridge = Bridge { sender, ..bridge };
                let future = bridge.request(Command::Close(1));
                let task = tokio::task::spawn_local(async move {
                    drop(receiver.recv().await);
                });
                let result = cljrs_async::await_value(future).await.unwrap();
                task.await.unwrap();
                assert!(result.to_string().contains(":reply-dropped"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_timeout_revokes_a_queued_request_before_execution() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (sender, mut receiver) = mpsc::channel(1);
                let bridge = Bridge {
                    sender,
                    engine_thread: std::thread::current(),
                    next: Cell::new(1),
                    permits: Arc::new(Semaphore::new(IN_FLIGHT)),
                    latencies: Rc::default(),
                    timeout: Duration::from_millis(1),
                };
                let result = cljrs_async::await_value(bridge.request(Command::Close(1)))
                    .await
                    .unwrap();
                assert!(result.to_string().contains(":timeout"));
                assert!(receiver.recv().await.unwrap().reply.is_closed());
            })
            .await;
    }
}
