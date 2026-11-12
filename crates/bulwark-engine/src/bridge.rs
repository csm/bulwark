//! Bounded transport implementing ADR D1. Admission never waits for capacity.
use crate::{Command, Data, Event, EventKind, Fault, Handle, IdAllocator};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};

pub const REQUEST_CAPACITY: usize = 32;
pub const UNFINISHED_LIMIT: usize = 64;
pub const EVENT_CAPACITY: usize = 256;
pub const PAYLOAD_BYTES: usize = 1024 * 1024;
pub const EVENT_BYTES: usize = 64 * 1024;
pub const REQUESTS_PER_TURN: usize = 16;
pub const TURN_BUDGET: Duration = Duration::from_millis(2);
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
pub type Wake = Arc<dyn Fn() + Send + Sync>;
static REQUEST_IDS: IdAllocator = IdAllocator::new();

struct State {
    stopped: AtomicBool,
    error: Mutex<Option<Fault>>,
    wake: Wake,
    key_routes: Mutex<Arc<crate::keys::KeyRoutes>>,
    // The latest window snapshot, and whether its change notification is outstanding.
    viewport: Mutex<(Option<crate::ui::Viewport>, bool)>,
}
/// Terminal state lives outside the event queue, so a full queue cannot hide it.
/// Stopping requests host shutdown; the host cancels the runtime after engine cleanup.
#[derive(Clone)]
pub struct Control(Arc<State>);
impl Control {
    /// A single bounded snapshot replaces the previous one; no queued updates.
    pub fn set_key_routes(&self, routes: crate::keys::KeyRoutes) -> Result<(), Fault> {
        if self.is_stopped() {
            return Err(Fault::ShuttingDown);
        }
        let old = std::mem::replace(&mut *self.0.key_routes.lock().unwrap(), Arc::new(routes));
        drop(old);
        Ok(())
    }
    pub fn key_routes(&self) -> Arc<crate::keys::KeyRoutes> {
        self.0.key_routes.lock().unwrap().clone()
    }
    /// Host side. Records the window snapshot and returns true when the host must emit one
    /// viewport event. Further changes stay coalesced until the runtime reads the snapshot,
    /// so a resize during a runtime stall cannot fill the event queue.
    pub fn set_viewport(&self, viewport: crate::ui::Viewport) -> bool {
        let mut state = self.0.viewport.lock().unwrap();
        if state.0 == Some(viewport) {
            return false;
        }
        state.0 = Some(viewport);
        !std::mem::replace(&mut state.1, true)
    }
    /// Runtime side. Returns the latest snapshot and rearms the change notification.
    pub fn viewport(&self) -> Option<crate::ui::Viewport> {
        let mut state = self.0.viewport.lock().unwrap();
        state.1 = false;
        state.0
    }
    pub fn is_stopped(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }
    pub fn error(&self) -> Option<Fault> {
        self.0.error.lock().unwrap().clone()
    }
    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
        (self.0.wake)();
    }
    pub fn fail(&self, fault: Fault) {
        self.0.error.lock().unwrap().get_or_insert(fault);
        self.stop();
    }
}

struct Completion {
    result: Result<Data, Fault>,
    // Capacity remains occupied while a completed reply awaits consumption.
    _permit: OwnedSemaphorePermit,
}
pub struct Request {
    pub id: u64,
    pub command: Command,
    pub deadline: Instant,
    reply: oneshot::Sender<Completion>,
    permit: OwnedSemaphorePermit,
}
impl Request {
    pub fn cancelled(&self) -> bool {
        self.reply.is_closed() || Instant::now() >= self.deadline
    }
    pub fn complete(self, result: Result<Data, Fault>) {
        let bytes = match &result {
            Ok(data) => data.string_bytes(),
            Err(Fault::Engine(message)) => message.len(),
            _ => 0,
        };
        let result = if bytes > PAYLOAD_BYTES {
            Err(Fault::PayloadTooLarge)
        } else {
            result
        };
        let _ = self.reply.send(Completion {
            result,
            _permit: self.permit,
        });
    }
}

pub struct Pending {
    pub id: u64,
    deadline: Instant,
    receiver: oneshot::Receiver<Completion>,
}
impl Pending {
    pub async fn wait(self) -> Result<Data, Fault> {
        let completion = tokio::time::timeout_at(self.deadline.into(), self.receiver)
            .await
            .map_err(|_| Fault::Timeout)?
            .map_err(|_| Fault::ReplyDropped)?;
        completion.result
    }
}

#[derive(Clone)]
pub struct Client {
    sender: mpsc::Sender<Request>,
    permits: Arc<Semaphore>,
    control: Control,
}
impl Client {
    /// Queue and unfinished capacity are checked synchronously, before spawning
    /// any language future. A timeout retains its permit until engine cleanup.
    pub fn submit(&self, command: Command, timeout: Duration) -> Result<Pending, Fault> {
        if self.control.is_stopped() {
            return Err(Fault::ShuttingDown);
        }
        if command.string_bytes() > PAYLOAD_BYTES {
            return Err(Fault::PayloadTooLarge);
        }
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| Fault::Busy)?;
        let id = REQUEST_IDS.next()?;
        let deadline = Instant::now().checked_add(timeout).ok_or(Fault::Timeout)?;
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(Request {
                id,
                command,
                deadline,
                reply,
                permit,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Fault::Busy,
                mpsc::error::TrySendError::Closed(_) => Fault::Disconnected,
            })?;
        (self.control.0.wake)();
        Ok(Pending {
            id,
            deadline,
            receiver,
        })
    }
    pub fn unfinished(&self) -> usize {
        UNFINISHED_LIMIT - self.permits.available_permits()
    }
}

/// Main-thread event producer. Rc enforces one ordered producer thread.
#[derive(Clone)]
pub struct EventSink {
    sender: mpsc::Sender<Event>,
    sequence: Rc<Cell<u64>>,
    control: Control,
}
impl EventSink {
    pub fn emit(&self, view: Handle, kind: EventKind) -> Result<(), Fault> {
        if self.control.is_stopped() {
            return Err(Fault::ShuttingDown);
        }
        if kind.string_bytes() > EVENT_BYTES {
            self.control.fail(Fault::PayloadTooLarge);
            return Err(Fault::PayloadTooLarge);
        }
        let sequence = self.sequence.get();
        if sequence > i64::MAX as u64 {
            self.control.fail(Fault::IdExhausted);
            return Err(Fault::IdExhausted);
        }
        self.sequence.set(sequence + 1);
        self.sender
            .try_send(Event {
                sequence,
                view,
                kind,
            })
            .map_err(|error| {
                let fault = match error {
                    mpsc::error::TrySendError::Full(_) => Fault::EventOverflow,
                    mpsc::error::TrySendError::Closed(_) => Fault::Disconnected,
                };
                self.control.fail(fault.clone());
                fault
            })
    }
}

pub struct RuntimeEndpoint {
    pub client: Client,
    pub events: mpsc::Receiver<Event>,
    pub control: Control,
}
pub struct EngineEndpoint {
    requests: mpsc::Receiver<Request>,
    pub events: EventSink,
    pub control: Control,
}
impl EngineEndpoint {
    /// Count and elapsed time are checked between calls, never within a call.
    /// The host must return to native event processing before its next turn.
    pub fn admit(&mut self, mut handle: impl FnMut(Request)) -> usize {
        let started = Instant::now();
        let mut admitted = 0;
        while admitted < REQUESTS_PER_TURN
            && started.elapsed() < TURN_BUDGET
            && !self.control.is_stopped()
        {
            let Ok(request) = self.requests.try_recv() else {
                break;
            };
            admitted += 1;
            if request.cancelled() {
                request.complete(Err(Fault::Timeout));
            } else {
                handle(request);
            }
        }
        if !self.requests.is_empty() && !self.control.is_stopped() {
            (self.control.0.wake)();
        }
        admitted
    }
    /// Call before invalidating engine resources and cancelling the runtime.
    pub fn stop_admission(&mut self) {
        self.control.stop();
        self.requests.close();
        while let Ok(request) = self.requests.try_recv() {
            request.complete(Err(Fault::ShuttingDown));
        }
    }
}

pub fn channel(wake: Wake) -> (RuntimeEndpoint, EngineEndpoint) {
    let control = Control(Arc::new(State {
        stopped: AtomicBool::new(false),
        error: Mutex::new(None),
        key_routes: Mutex::new(Arc::default()),
        viewport: Mutex::new((None, false)),
        wake,
    }));
    let (sender, requests) = mpsc::channel(REQUEST_CAPACITY);
    let (events, receiver) = mpsc::channel(EVENT_CAPACITY);
    let client = Client {
        sender,
        permits: Arc::new(Semaphore::new(UNFINISHED_LIMIT)),
        control: control.clone(),
    };
    let sink = EventSink {
        sender: events,
        sequence: Rc::new(Cell::new(1)),
        control: control.clone(),
    };
    (
        RuntimeEndpoint {
            client,
            events: receiver,
            control: control.clone(),
        },
        EngineEndpoint {
            requests,
            events: sink,
            control,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (RuntimeEndpoint, EngineEndpoint) {
        channel(Arc::new(|| {}))
    }
    fn submit(client: &Client) -> Pending {
        client.submit(Command::Quit, DEFAULT_TIMEOUT).unwrap()
    }
    #[test]
    fn boundary_contains_only_owned_send_data() {
        fn send<T: Send>() {}
        send::<Request>();
        send::<RuntimeEndpoint>();
        send::<Data>();
        send::<Event>();
    }
    #[tokio::test]
    async fn queue_and_unfinished_limits_include_unconsumed_replies() {
        let (runtime, mut engine) = pair();
        let mut pending: Vec<_> = (0..REQUEST_CAPACITY)
            .map(|_| submit(&runtime.client))
            .collect();
        assert!(matches!(
            runtime.client.submit(Command::Quit, DEFAULT_TIMEOUT),
            Err(Fault::Busy)
        ));
        assert!(engine.admit(|r| r.complete(Ok(Data::Nil))) <= REQUESTS_PER_TURN);
        while !engine.requests.is_empty() {
            engine.admit(|r| r.complete(Ok(Data::Nil)));
        }
        pending.extend((0..REQUEST_CAPACITY).map(|_| submit(&runtime.client)));
        while !engine.requests.is_empty() {
            engine.admit(|r| r.complete(Ok(Data::Nil)));
        }
        assert_eq!(runtime.client.unfinished(), UNFINISHED_LIMIT);
        assert!(matches!(
            runtime.client.submit(Command::Quit, DEFAULT_TIMEOUT),
            Err(Fault::Busy)
        ));
        pending.pop().unwrap().wait().await.unwrap();
        assert_eq!(runtime.client.unfinished(), UNFINISHED_LIMIT - 1);
        submit(&runtime.client);
    }
    #[tokio::test]
    async fn replies_correlate_when_engine_completes_in_reverse_order() {
        let (runtime, mut engine) = pair();
        let a = submit(&runtime.client);
        let b = submit(&runtime.client);
        assert_ne!(a.id, b.id);
        let mut batch = Vec::new();
        engine.admit(|r| batch.push(r));
        let second = batch.pop().unwrap();
        let first = batch.pop().unwrap();
        second.complete(Ok(Data::Text("b".into())));
        first.complete(Ok(Data::Text("a".into())));
        assert_eq!(a.wait().await, Ok(Data::Text("a".into())));
        assert_eq!(b.wait().await, Ok(Data::Text("b".into())));
    }
    #[tokio::test]
    async fn timeout_revokes_queued_work_and_capacity_waits_for_cleanup() {
        let (runtime, mut engine) = pair();
        let request = runtime
            .client
            .submit(Command::Quit, Duration::ZERO)
            .unwrap();
        assert_eq!(request.wait().await, Err(Fault::Timeout));
        assert_eq!(runtime.client.unfinished(), 1);
        engine.admit(|_| panic!("Expired request executed"));
        assert_eq!(runtime.client.unfinished(), 0);
        let expired = runtime
            .client
            .submit(Command::Quit, Duration::ZERO)
            .unwrap();
        engine.admit(|_| panic!("Request past its deadline executed with an open receiver"));
        assert_eq!(expired.wait().await, Err(Fault::Timeout));
        let request = submit(&runtime.client);
        drop(request);
        engine.admit(|_| panic!("Cancelled request executed"));
        assert_eq!(runtime.client.unfinished(), 0);
    }
    #[test]
    fn slow_engine_call_returns_control_before_another_request() {
        let (runtime, mut engine) = pair();
        let _first = submit(&runtime.client);
        let _second = submit(&runtime.client);
        assert_eq!(
            engine.admit(|request| {
                std::thread::sleep(TURN_BUDGET + Duration::from_millis(1));
                request.complete(Ok(Data::Nil));
            }),
            1
        );
        assert_eq!(engine.requests.len(), 1);
    }
    #[tokio::test]
    async fn exact_payload_limits_are_accepted() {
        let (mut runtime, mut engine) = pair();
        let pending = runtime
            .client
            .submit(Command::Log("x".repeat(PAYLOAD_BYTES)), DEFAULT_TIMEOUT)
            .unwrap();
        engine.admit(|r| r.complete(Ok(Data::Text("x".repeat(PAYLOAD_BYTES)))));
        assert!(
            matches!(pending.wait().await, Ok(Data::Text(text)) if text.len() == PAYLOAD_BYTES)
        );
        engine
            .events
            .emit(Handle(1), EventKind::TitleChanged("x".repeat(EVENT_BYTES)))
            .unwrap();
        assert_eq!(
            runtime.events.recv().await.unwrap().kind.string_bytes(),
            EVENT_BYTES
        );
        assert!(!runtime.control.is_stopped());
    }
    #[tokio::test]
    async fn payload_bounds_apply_to_commands_replies_and_error_messages() {
        let (runtime, mut engine) = pair();
        assert!(matches!(
            runtime.client.submit(
                Command::Log("é".repeat(PAYLOAD_BYTES / 2 + 1)),
                DEFAULT_TIMEOUT
            ),
            Err(Fault::PayloadTooLarge)
        ));
        assert_eq!(runtime.client.unfinished(), 0);
        for result in [
            Ok(Data::Info(crate::ViewInfo {
                url: "u".repeat(PAYLOAD_BYTES),
                title: "x".into(),
                load_status: crate::LoadStatus::Complete,
                can_go_back: false,
                can_go_forward: false,
            })),
            Err(Fault::Engine("e".repeat(PAYLOAD_BYTES + 1))),
        ] {
            let pending = submit(&runtime.client);
            let mut result = Some(result);
            engine.admit(|r| r.complete(result.take().unwrap()));
            assert_eq!(pending.wait().await, Err(Fault::PayloadTooLarge));
        }
    }
    #[tokio::test]
    async fn event_order_and_fatal_overload_use_separate_signal() {
        let (mut runtime, engine) = pair();
        for _ in 0..EVENT_CAPACITY {
            engine.events.emit(Handle(1), EventKind::Closed).unwrap();
        }
        assert_eq!(
            engine.events.emit(Handle(1), EventKind::Closed),
            Err(Fault::EventOverflow)
        );
        assert!(runtime.control.is_stopped());
        assert_eq!(runtime.control.error(), Some(Fault::EventOverflow));
        engine.control.fail(Fault::Disconnected);
        assert_eq!(runtime.control.error(), Some(Fault::EventOverflow));
        for sequence in 1..=EVENT_CAPACITY {
            assert_eq!(
                runtime.events.recv().await.unwrap().sequence,
                sequence as u64
            );
        }
    }
    #[test]
    fn oversized_events_fail_before_admission() {
        let (mut runtime, engine) = pair();
        assert_eq!(
            engine.events.emit(
                Handle(1),
                EventKind::TitleChanged("t".repeat(EVENT_BYTES + 1))
            ),
            Err(Fault::PayloadTooLarge)
        );
        assert!(runtime.events.try_recv().is_err());
        assert_eq!(runtime.control.error(), Some(Fault::PayloadTooLarge));
    }
    #[test]
    fn viewport_changes_coalesce_until_the_runtime_reads_the_snapshot() {
        let (runtime, engine) = pair();
        let viewport = |width| crate::ui::Viewport {
            width,
            height: 760,
            scale: 2.0,
        };
        assert_eq!(runtime.control.viewport(), None);
        assert!(engine.control.set_viewport(viewport(1000)));
        assert!(!engine.control.set_viewport(viewport(1000)));
        assert!(!engine.control.set_viewport(viewport(900)));
        assert!(!engine.control.set_viewport(viewport(800)));
        assert_eq!(runtime.control.viewport(), Some(viewport(800)));
        assert!(!engine.control.set_viewport(viewport(800)));
        assert!(engine.control.set_viewport(viewport(700)));
        assert_eq!(runtime.control.viewport(), Some(viewport(700)));
    }
    #[tokio::test]
    async fn shutdown_rejects_queued_and_new_work() {
        let (runtime, mut engine) = pair();
        let pending = submit(&runtime.client);
        engine.stop_admission();
        assert_eq!(pending.wait().await, Err(Fault::ShuttingDown));
        assert!(matches!(
            runtime.client.submit(Command::Quit, DEFAULT_TIMEOUT),
            Err(Fault::ShuttingDown)
        ));
    }
    #[tokio::test]
    async fn disconnection_and_dropped_replies_settle() {
        let (runtime, mut engine) = pair();
        let pending = submit(&runtime.client);
        engine.admit(drop);
        assert_eq!(pending.wait().await, Err(Fault::ReplyDropped));
        drop(engine);
        assert!(matches!(
            runtime.client.submit(Command::Quit, DEFAULT_TIMEOUT),
            Err(Fault::Disconnected)
        ));
    }
}
