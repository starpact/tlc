use anyhow::{bail, Result};
use serde::Serialize;
use std::sync::atomic::{AtomicI64, Ordering};
use tracing::debug;

#[derive(Debug, Default)]
pub struct ProgressBar(AtomicI64);

#[derive(Debug, Serialize)]
pub enum Progress {
    Uninitialized,
    InProgress { total: u32, count: u32 },
    Finished { total: u32 },
}

impl ProgressBar {
    pub fn start(&self, new_total: u32) {
        while self
            .0
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
        to_progress(self.0.load(Ordering::Relaxed))
    }

    pub fn add(&self, n: i64) -> Result<()> {
        let old = self.0.fetch_add(n, Ordering::Relaxed);
        if old < 0 {
            bail!("interrupted");
        }
        if old == 0 {
            unreachable!("add before start");
        }
        if old as u32 >= (old >> 32) as u32 {
            unreachable!("progress exceeds limit");
        }

        Ok(())
    }

    pub fn reset(&self) {
        self.0.store(0, Ordering::Relaxed);
    }

    fn interrupt(&self) {
        self.0.store(i64::MIN, Ordering::Relaxed);
        for i in 0.. {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let progress = to_progress(self.0.load(Ordering::Relaxed));
            if matches!(progress, Progress::Uninitialized) {
                debug!("Interrupted after {i} checks");
                break;
            }
        }
    }
}

/// Higher 32 bits: total, lower 32 bits: count.
fn to_progress(x: i64) -> Progress {
    let count = x as u32;
    let total = (x >> 32) as u32;
    match (count, total) {
        (0, 0) => Progress::Uninitialized,
        (count, total) if count == total => Progress::Finished { total },
        _ => Progress::InProgress { total, count },
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_interrupt() {
        let progress_bar = ProgressBar::default();
        assert_matches!(progress_bar.get(), Progress::Uninitialized);

        progress_bar.start(2333);
        progress_bar.add(1).unwrap();

        progress_bar.start(2333);
        progress_bar.add(1).unwrap();
        assert_matches!(
            progress_bar.get(),
            Progress::InProgress {
                total: 2333,
                count: 1
            }
        );
    }

    #[test]
    #[should_panic]
    fn test_add_before_start_panic() {
        ProgressBar::default().add(1).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_progress_exceeds_limit() {
        let progress_bar = ProgressBar::default();
        progress_bar.start(1);
        progress_bar.add(1).unwrap();
        progress_bar.add(1).unwrap();
    }
}
