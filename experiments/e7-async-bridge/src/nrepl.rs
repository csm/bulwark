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

pub struct Driver {
    poller: cljrs_nrepl::Poller,
    interval: Interval,
    calls: u64,
    max_admission: Duration,
}
impl Driver {
    pub fn start(globals: Arc<GlobalEnv>, port_file: Option<PathBuf>) -> Result<Self> {
        let server = cljrs_nrepl::start(
            cljrs_nrepl::Config {
                port_file,
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
        })
    }
    pub fn admit(&mut self, local: &LocalSet) -> Result<()> {
        let started = Instant::now();
        // Upstream still drains admission without a budget. This probe measures
        // that call under finite load; the timer is not a work or queue bound.
        let status = self
            .poller
            .spawn_pending(local, cljrs_runtime::tiered::eval);
        self.calls += 1;
        self.max_admission = self.max_admission.max(started.elapsed());
        ensure!(
            status == cljrs_nrepl::PollStatus::Open,
            "nREPL network thread disconnected"
        );
        Ok(())
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        println!(
            "E7 nREPL: admission_calls={}, max_admission={:?}",
            self.calls, self.max_admission
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
