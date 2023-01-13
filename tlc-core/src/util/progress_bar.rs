use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Default, Clone)]
pub struct ProgressBar(Arc<AtomicI64>);

#[derive(Debug, Default, Serialize, Clone, Copy, PartialEq)]
pub enum Progress {
    #[default]
    Uninitialized,
    InProgress {
        total: u32,
        count: u32,
    },
    Finished {
        total: u32,
    },
}

impl ProgressBar {
    pub fn get(&self) -> Progress {
        to_progress(self.0.load(Ordering::Relaxed))
    }

    pub fn cancel(&self) {
        self.0.store(i64::MIN, Ordering::Relaxed);
    }

    pub fn start(&self, total: u32) -> Result<()> {
        let old = self.0.swap((total as i64) << 32, Ordering::Relaxed);
        if old < 0 {
            bail!("cancelled")
        }
        Ok(())
    }

    pub fn add(&self, n: u32) -> Result<()> {
        let old = self.0.fetch_add(n as i64, Ordering::Relaxed);
        if old < 0 {
            bail!("cancelled");
        }
        assert_ne!(old, 0);
        let total = (old >> 32) as u32;
        let count = old as u32;
        assert!(count < total);

        Ok(())
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
    use std::{
        assert_matches::assert_matches,
        thread::{sleep, spawn},
        time::Duration,
    };

    use super::*;

    #[test]
    fn test_finish() {
        let progress_bar = ProgressBar::default();
        const TOTAL: u32 = 100000;
        progress_bar.start(TOTAL).unwrap();
        (0..TOTAL).for_each(|_| progress_bar.add(1).unwrap());
        assert_matches!(progress_bar.get(), Progress::Finished { total } if total == TOTAL);
    }

    #[test]
    fn test_cancel_previous() {
        let progress_bar = ProgressBar::default();
        let progress_bar1 = progress_bar.clone();
        let join_handle = spawn(move || {
            const TOTAL: u32 = 100000;
            progress_bar1.start(TOTAL).unwrap();
            (0..TOTAL)
                .try_for_each(|_| {
                    sleep(Duration::from_millis(1));
                    progress_bar1.add(1)
                })
                .unwrap_err();
        });
        sleep(Duration::from_millis(100));

        println!("{:?}", progress_bar.get());
        assert_matches!(progress_bar.get(), Progress::InProgress { .. });

        progress_bar.cancel();
        join_handle.join().unwrap();
    }

    #[test]
    fn test_cancelled_before_start() {
        let progress_bar = ProgressBar::default();
        progress_bar.cancel();
        progress_bar.start(100).unwrap_err();
    }
}
