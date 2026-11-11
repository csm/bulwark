//! Owned data shared by the engine and runtime. No Servo or GC types belong here.
pub mod bridge;
pub mod keys;
pub mod mock;

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

/// Handles fit in a Clojure long. Zero is reserved; exhaustion never wraps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle(pub u64);

struct IdAllocator(AtomicU64);
impl IdAllocator {
    const fn new() -> Self {
        Self(AtomicU64::new(1))
    }
    fn next(&self) -> Result<u64, Fault> {
        let mut id = self.0.load(Ordering::Relaxed);
        loop {
            if id > i64::MAX as u64 {
                return Err(Fault::IdExhausted);
            }
            match self
                .0
                .compare_exchange_weak(id, id + 1, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return Ok(id),
                Err(current) => id = current,
            }
        }
    }
}
static HANDLES: IdAllocator = IdAllocator::new();

/// Resource ids come from one process-wide allocator, even across registries.
pub struct Registry<T>(BTreeMap<Handle, T>);
impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}
impl<T> Registry<T> {
    pub fn insert(&mut self, resource: T) -> Result<Handle, Fault> {
        self.insert_with(|_| resource)
    }
    /// Allocate the handle before constructing a resource that needs it in callbacks.
    pub fn insert_with(&mut self, build: impl FnOnce(Handle) -> T) -> Result<Handle, Fault> {
        let id = Handle(HANDLES.next()?);
        self.0.insert(id, build(id));
        Ok(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (Handle, &T)> {
        self.0.iter().map(|(id, resource)| (*id, resource))
    }
    pub fn get(&self, id: Handle) -> Result<&T, Fault> {
        self.0.get(&id).ok_or(Fault::StaleHandle)
    }
    pub fn get_mut(&mut self, id: Handle) -> Result<&mut T, Fault> {
        self.0.get_mut(&id).ok_or(Fault::StaleHandle)
    }
    pub fn remove(&mut self, id: Handle) -> Result<T, Fault> {
        self.0.remove(&id).ok_or(Fault::StaleHandle)
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Busy,
    Disconnected,
    ReplyDropped,
    Timeout,
    StaleHandle,
    IdExhausted,
    PayloadTooLarge,
    EventOverflow,
    ShuttingDown,
    Engine(String),
}
impl Fault {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Disconnected => "engine-disconnected",
            Self::ReplyDropped => "reply-dropped",
            Self::Timeout => "timeout",
            Self::StaleHandle => "stale-handle",
            Self::IdExhausted => "id-exhausted",
            Self::PayloadTooLarge => "payload-too-large",
            Self::EventOverflow => "event-overflow",
            Self::ShuttingDown => "shutting-down",
            Self::Engine(_) => "engine-error",
        }
    }
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(message) => write!(f, "engine-error: {message}"),
            _ => f.write_str(self.kind()),
        }
    }
}
impl std::error::Error for Fault {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Started,
    HeadParsed,
    Complete,
}
impl LoadStatus {
    pub fn name(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::HeadParsed => "head-parsed",
            Self::Complete => "complete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewInfo {
    pub url: String,
    pub title: String,
    pub load_status: LoadStatus,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug)]
pub enum Command {
    Create {
        url: String,
    },
    Close(Handle),
    /// Select one retained view, or hide all views for non-web content.
    Select(Option<Handle>),
    Navigate {
        view: Handle,
        url: String,
    },
    Reload(Handle),
    Back(Handle),
    Forward(Handle),
    Info(Handle),
    Evaluate {
        view: Handle,
        script: String,
    },
    Log(String),
    SetEcho(String),
    Quit,
}
impl Command {
    pub fn string_bytes(&self) -> usize {
        match self {
            Self::Create { url } | Self::Navigate { url, .. } => url.len(),
            Self::Evaluate { script, .. } => script.len(),
            Self::Log(text) | Self::SetEcho(text) => text.len(),
            _ => 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Data {
    Nil,
    Handle(Handle),
    Text(String),
    Info(ViewInfo),
}
impl Data {
    pub fn string_bytes(&self) -> usize {
        match self {
            Self::Text(text) => text.len(),
            Self::Info(info) => info.url.len().saturating_add(info.title.len()),
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub key: String,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub pressed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    UrlChanged(String),
    TitleChanged(String),
    LoadStatusChanged(LoadStatus),
    HistoryChanged {
        can_go_back: bool,
        can_go_forward: bool,
    },
    Closed,
    Crashed(String),
    KeyInput(KeyInput),
    InputReset,
}
impl EventKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::UrlChanged(_) => "page/url-changed",
            Self::TitleChanged(_) => "page/title-changed",
            Self::LoadStatusChanged(_) => "page/load-status-changed",
            Self::HistoryChanged { .. } => "page/history-changed",
            Self::Closed => "page/closed",
            Self::Crashed(_) => "page/crashed",
            Self::KeyInput(_) => "input/key",
            Self::InputReset => "input/reset",
        }
    }
    pub fn string_bytes(&self) -> usize {
        match self {
            Self::UrlChanged(s) | Self::TitleChanged(s) | Self::Crashed(s) => s.len(),
            Self::KeyInput(key) => key.key.len(),
            _ => 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub sequence: u64,
    pub view: Handle,
    pub kind: EventKind,
}

/// Main-thread mechanism interface. Implementations settle requests immediately
/// or retain them until a callback; tick must clean up cancelled operations.
/// The native shell uses the concrete adapter for thread-affine input and presentation.
pub trait Engine {
    fn handle(&mut self, request: bridge::Request);
    fn tick(&mut self);
    /// Invalidate all handles and settle outstanding replies before runtime cancellation.
    fn shutdown(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_never_refer_to_replacements_or_other_registries() {
        let mut first = Registry::default();
        let id = first.insert("old").unwrap();
        first.remove(id).unwrap();
        let replacement = first.insert("new").unwrap();
        let mut second = Registry::default();
        let other = second.insert("other").unwrap();
        assert!(replacement > id);
        assert_ne!(other, replacement);
        assert_eq!(first.get(id), Err(Fault::StaleHandle));
        assert_eq!(second.get(id), Err(Fault::StaleHandle));
    }
    #[test]
    fn identifier_exhaustion_does_not_wrap() {
        let ids = IdAllocator(AtomicU64::new(i64::MAX as u64));
        assert_eq!(ids.next(), Ok(i64::MAX as u64));
        assert_eq!(ids.next(), Err(Fault::IdExhausted));
        assert_eq!(ids.next(), Err(Fault::IdExhausted));
    }

    #[test]
    fn concurrent_allocations_remain_unique_at_exhaustion() {
        let first = i64::MAX as u64 - 63;
        let ids = IdAllocator(AtomicU64::new(first));
        let mut allocated = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        let mut allocated = Vec::new();
                        loop {
                            match ids.next() {
                                Ok(id) => allocated.push(id),
                                Err(Fault::IdExhausted) => break,
                                Err(error) => panic!("Unexpected allocation error: {error}"),
                            }
                        }
                        allocated
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        allocated.sort_unstable();
        assert_eq!(allocated, (first..=i64::MAX as u64).collect::<Vec<_>>());
        assert_eq!(ids.next(), Err(Fault::IdExhausted));
    }
}
