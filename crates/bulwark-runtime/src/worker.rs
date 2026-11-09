//! The runtime, its GC state, LocalSet, and fibers live and drop on this thread.
use crate::RuntimeHost;
use anyhow::{Result, anyhow};
use bulwark_engine::bridge::{Control, RuntimeEndpoint};
use cljrs_runtime::{
    ExecutionMode,
    env::{
        gas::{InterruptFlag, InterruptGuard},
        gc_roots,
    },
};
use std::{sync::Arc, thread};
use tokio::sync::oneshot;

pub struct Worker {
    thread: Option<thread::JoinHandle<Result<String>>>,
    interrupt: Arc<InterruptFlag>,
    cancel: Option<oneshot::Sender<()>>,
}

struct OnExit(Control);
impl Drop for OnExit {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.fail(bulwark_engine::Fault::Engine(
                "Runtime thread panicked".into(),
            ));
        } else {
            self.0.stop();
        }
    }
}

fn run(
    endpoint: RuntimeEndpoint,
    source: String,
    mode: ExecutionMode,
    interrupt: Arc<InterruptFlag>,
    cancelled: oneshot::Receiver<()>,
) -> Result<String> {
    let _exit = OnExit(endpoint.control.clone());
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let local = tokio::task::LocalSet::new();
    executor.block_on(local.run_until(async move {
        let _interrupt = InterruptGuard::install(interrupt);
        let mut host = RuntimeHost::with_bridge(mode, endpoint)?;
        let events = host.eval("(require 'bulwark.bridge) (bulwark.bridge/run-events!)")?;
        let _events_root = gc_roots::root_value(&events);
        let program = host.eval(&source)?;
        let _program_root = gc_roots::root_value(&program);
        host.collect();
        tokio::select! {
            result = cljrs_async::await_value(program) => {
                result.map(|value| value.to_string()).map_err(|error| anyhow!("{error}"))
            },
            result = cljrs_async::await_value(events) => match result {
                Err(error) => Err(anyhow!("Event dispatcher failed: {error}")),
                Ok(_) => Err(anyhow!("Event stream ended before program completion")),
            },
            _ = cancelled => Err(anyhow!("Runtime cancelled")),
        }
    }))
}

impl Worker {
    /// Run a Clojure program concurrently with the ordered event dispatcher.
    /// The program can return a plain value or a future; only its printed result crosses threads.
    pub fn spawn(endpoint: RuntimeEndpoint, source: String) -> Result<Self> {
        Self::spawn_with_mode(endpoint, source, ExecutionMode::Tiered)
    }

    /// Selects an evaluator for interpreter checks without changing runtime ownership.
    pub fn spawn_with_mode(
        endpoint: RuntimeEndpoint,
        source: String,
        mode: ExecutionMode,
    ) -> Result<Self> {
        let (cancel, cancelled) = oneshot::channel();
        let interrupt = Arc::new(InterruptFlag::new(false));
        let worker_interrupt = interrupt.clone();
        let thread = thread::Builder::new()
            .name("bulwark-runtime".into())
            .spawn(move || run(endpoint, source, mode, worker_interrupt, cancelled))?;
        Ok(Self {
            thread: Some(thread),
            interrupt,
            cancel: Some(cancel),
        })
    }
    pub fn stop(&mut self) {
        self.interrupt.request();
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
    /// The host owns the shutdown deadline. This method never joins a running worker.
    pub fn join_finished(&mut self) -> Option<Result<String>> {
        if !self.finished() {
            return None;
        }
        self.thread.take().map(|t| {
            t.join()
                .unwrap_or_else(|_| Err(anyhow!("Runtime thread panicked")))
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
