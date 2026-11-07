//! Embedded nREPL runs on the same LocalSet as the browser program.
use anyhow::{Result, ensure};
use cljrs_runtime::env::env::GlobalEnv;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    task::LocalSet,
    time::{Interval, MissedTickBehavior},
};

const ADMISSION_BUDGET: usize = 4;
const LIMITS: cljrs_nrepl::Limits = cljrs_nrepl::Limits {
    max_pending_jobs: 32,
    max_session_jobs: 24,
    max_request_bytes: 64 * 1024,
    max_output_bytes: 256 * 1024,
};

pub struct Driver {
    poller: cljrs_nrepl::Poller,
    interval: Interval,
    calls: u64,
    max_admission: Duration,
    max_jobs: usize,
    max_pending: usize,
    exhausted: u64,
}
impl Driver {
    pub fn start(globals: Arc<GlobalEnv>, port_file: Option<PathBuf>) -> Result<Self> {
        let server = cljrs_nrepl::start(
            cljrs_nrepl::Config {
                port_file,
                limits: LIMITS,
                ..Default::default()
            },
            globals,
        )
        .map_err(|error| anyhow::anyhow!("{error}"))?;
        println!("nREPL: 127.0.0.1:{}", server.port());
        let mut interval = tokio::time::interval(Duration::from_millis(1));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        Ok(Self {
            poller: server.into_poller(),
            interval,
            calls: 0,
            max_admission: Duration::ZERO,
            max_jobs: 0,
            max_pending: 0,
            exhausted: 0,
        })
    }
    pub fn admit(&mut self, local: &LocalSet) -> Result<()> {
        let started = Instant::now();
        // Yield back to the LocalSet between batches, including under overload.
        let batch =
            self.poller
                .spawn_pending_budget(local, cljrs_runtime::tiered::eval, ADMISSION_BUDGET);
        self.calls += 1;
        self.max_admission = self.max_admission.max(started.elapsed());
        self.max_jobs = self.max_jobs.max(batch.jobs);
        self.max_pending = self.max_pending.max(self.poller.pending_jobs());
        self.exhausted += u64::from(batch.budget_exhausted);
        ensure!(
            batch.jobs <= ADMISSION_BUDGET && self.max_pending <= LIMITS.max_pending_jobs,
            "nREPL exceeded its admission limits"
        );
        ensure!(
            batch.status == cljrs_nrepl::PollStatus::Open,
            "nREPL network thread disconnected"
        );
        Ok(())
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        println!(
            "E7 nREPL: admission_calls={}, max_admission={:?}, max_jobs={}, max_pending={}, budget_exhausted={}",
            self.calls, self.max_admission, self.max_jobs, self.max_pending, self.exhausted
        );
        // Poller drops on this runtime thread, stops its network thread, and
        // removes the port file. The LocalSet then drops its remaining fibers.
    }
}

pub async fn next_turn(driver: &mut Option<Driver>) {
    if let Some(driver) = driver {
        driver.interval.tick().await;
    } else {
        std::future::pending::<()>().await;
    }
}
