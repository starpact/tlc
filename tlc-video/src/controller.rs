use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Default, Clone)]
pub struct VideoController {
    read_video: ProgressBar,
    build_green2: ProgressBar,
    detect_peak: ProgressBar,
}

impl VideoController {
    pub fn read_video_progress(&self) -> Progress {
        self.read_video.get()
    }

    pub fn build_green2_progress(&self) -> Progress {
        self.build_green2.get()
    }

    pub fn detect_peak_progress(&self) -> Progress {
        self.detect_peak.get()
    }

    pub fn prepare_read_video(&mut self) -> ProgressBar {
        std::mem::take(&mut self.read_video).cancel();
        std::mem::take(&mut self.build_green2).cancel();
        std::mem::take(&mut self.detect_peak).cancel();
        self.read_video.clone()
    }

    pub fn prepare_build_green2(&mut self) -> ProgressBar {
        std::mem::take(&mut self.build_green2).cancel();
        std::mem::take(&mut self.detect_peak).cancel();
        self.build_green2.clone()
    }

    pub fn prepare_detect_peak(&mut self) -> ProgressBar {
        std::mem::take(&mut self.detect_peak).cancel();
        self.detect_peak.clone()
    }
}

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
    pub(crate) fn get(&self) -> Progress {
        to_progress(self.0.load(Ordering::Relaxed))
    }

    fn cancel(&self) {
        self.0.store(i64::MIN, Ordering::Relaxed);
    }

    pub(crate) fn start(&self, total: u32) -> Result<()> {
        let old = self.0.swap((total as i64) << 32, Ordering::Relaxed);
        if old < 0 {
            bail!("cancelled")
        }
        Ok(())
    }

    pub(crate) fn add(&self, n: u32) -> Result<()> {
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
