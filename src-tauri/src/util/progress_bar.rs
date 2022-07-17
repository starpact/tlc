use anyhow::{bail, Result};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
use tracing::debug;

#[derive(Debug, Default)]
pub struct ProgressBar {
    inner: Arc<AtomicI64>,
}

#[derive(Debug, Serialize)]
pub enum Progress {
    Uninitialized,
    InProgress { total: u32, count: u32 },
    Finished { total: u32 },
}

impl ProgressBar {
    pub fn start(&self, new_total: u32) {
        if self
            .inner
            .fetch_update(Ordering::SeqCst, Ordering::Acquire, |x| {
                match to_progress(x) {
                    Progress::InProgress { .. } => None,
                    _ => Some((new_total as i64) << 32),
                }
            })
            .is_err()
        {
            self.interrupt();
        }
    }

    pub fn get(&self) -> Progress {
        to_progress(self.inner.load(Ordering::Relaxed))
    }

    pub fn add(&self, n: i64) -> Result<()> {
        if self.inner.fetch_add(n, Ordering::Relaxed) < 0 {
            bail!("interrupted");
        }
        Ok(())
    }

    pub fn reset(&self) {
        self.inner.store(0, Ordering::Relaxed);
    }

    fn interrupt(&self) {
        self.inner.store(i64::MIN, Ordering::Relaxed);
        for i in 0.. {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let progress = to_progress(self.inner.load(Ordering::Relaxed));
            if matches!(progress, Progress::Uninitialized) {
                debug!("Interrupted after {} checks", i);
                break;
            }
        }
    }
}

impl Clone for ProgressBar {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Higher 32 bits: total, lower 32 bits: count.
fn to_progress(x: i64) -> Progress {
    let total = (x >> 32) as u32;
    let count = x as u32;
    match (total, count) {
        (0, 0) => Progress::Uninitialized,
        (total, count) if total == count => Progress::Finished { total },
        _ => Progress::InProgress { total, count },
    }
}
