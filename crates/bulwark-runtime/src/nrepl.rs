//! Embedded nREPL runs on the same LocalSet as the browser program.
use anyhow::{Context, Result, ensure};
use cljrs_runtime::env::env::GlobalEnv;
use std::{
    fs::{File, OpenOptions},
    io::Write,
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

/// Opt-in nREPL configuration. The listener always binds to IPv4 loopback.
#[derive(Clone, Debug)]
pub struct Config {
    pub profile: PathBuf,
}

// Reserve the editor discovery file exclusively. A second instance must not
// overwrite the first instance's port or remove its file on shutdown.
struct PortFile {
    path: PathBuf,
    file: File,
}
impl PortFile {
    fn reserve(config: Config) -> Result<Self> {
        std::fs::create_dir_all(&config.profile)
            .with_context(|| format!("Cannot create profile {}", config.profile.display()))?;
        let path = config.profile.join(".nrepl-port");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).with_context(|| {
            format!("Cannot reserve {}; use another profile or remove a stale port file after its process exits", path.display())
        })?;
        Ok(Self { path, file })
    }
}
impl Drop for PortFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) struct Driver {
    poller: cljrs_nrepl::Poller,
    // Drop the network owner before removing editor discovery.
    _port_file: PortFile,
    interval: Interval,
    calls: u64,
    max_admission: Duration,
    max_jobs: usize,
    max_pending: usize,
    exhausted: u64,
}
impl Driver {
    pub fn start(globals: Arc<GlobalEnv>, config: Config) -> Result<Self> {
        let mut port_file = PortFile::reserve(config)?;
        let server = cljrs_nrepl::start(
            cljrs_nrepl::Config {
                addr: ([127, 0, 0, 1], 0).into(),
                port_file: None,
                limits: LIMITS,
            },
            globals,
        )
        .map_err(|error| anyhow::anyhow!("{error}"))?;
        let port = server.port();
        let poller = server.into_poller();
        writeln!(port_file.file, "{port}")?;
        port_file.file.flush()?;
        println!("nREPL: 127.0.0.1:{port}");
        let mut interval = tokio::time::interval(Duration::from_millis(1));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        Ok(Self {
            poller,
            _port_file: port_file,
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
            "Bulwark nREPL: admission_calls={}, max_admission={:?}, max_jobs={}, max_pending={}, budget_exhausted={}",
            self.calls, self.max_admission, self.max_jobs, self.max_pending, self.exhausted
        );
        // Poller drops on this runtime thread, stops its network thread, and
        // closes connections. PortFile removes discovery, then the LocalSet drops fibers.
    }
}

pub(crate) async fn next_turn(driver: &mut Option<Driver>) {
    if let Some(driver) = driver {
        driver.interval.tick().await;
    } else {
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_reservation_preserves_existing_discovery_and_cleans_up() {
        let profile =
            std::env::temp_dir().join(format!("bulwark-port-test-{}", std::process::id()));
        let config = Config {
            profile: profile.clone(),
        };
        let mut first = PortFile::reserve(config.clone()).unwrap();
        writeln!(first.file, "12345").unwrap();
        let path = profile.join(".nrepl-port");
        assert!(PortFile::reserve(config.clone()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "12345\n");
        drop(first);
        assert!(!path.exists());
        drop(PortFile::reserve(config).unwrap());
        assert!(!path.exists());
        std::fs::remove_dir(profile).unwrap();
    }
}
