//! Bounded diagnostic for CLJ-ASYNC-004, without a timing threshold.
use std::{
    future::{Future, pending},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};

#[derive(Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.iter()
            .all(|arg| matches!(arg.as_str(), "--baseline" | "--require-idle")),
        "Usage: gc_idle [--baseline] [--require-idle]"
    );
    let initialized = !args.iter().any(|arg| arg == "--baseline");
    let host = cljrs_runtime::Runtime::builder().build()?;
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let local = tokio::task::LocalSet::new();
    let _runtime = executor.enter();
    let _local = local.enter();
    if initialized {
        cljrs_async::init(host.globals());
    }
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(wakes.clone());
    let mut context = Context::from_waker(&waker);
    let mut drive = Box::pin(local.run_until(pending::<()>()));

    // Discard setup wakes before sampling the otherwise idle LocalSet.
    for _ in 0..4 {
        assert!(drive.as_mut().poll(&mut context).is_pending());
    }
    wakes.0.store(0, Ordering::Relaxed);
    let mut self_waking_polls = 0;
    for _ in 0..16 {
        let before = wakes.0.load(Ordering::Relaxed);
        assert!(drive.as_mut().poll(&mut context).is_pending());
        self_waking_polls += usize::from(wakes.0.load(Ordering::Relaxed) > before);
    }
    println!(
        "initialized={initialized}, pending_polls=16, self_waking_polls={self_waking_polls}, wake_calls={}",
        wakes.0.load(Ordering::Relaxed)
    );
    if !initialized || args.iter().any(|arg| arg == "--require-idle") {
        anyhow::ensure!(
            self_waking_polls == 0,
            "Idle executor continues to wake itself"
        );
    }
    Ok(())
}
