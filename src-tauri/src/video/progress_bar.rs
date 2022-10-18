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

pub struct ResetGuard<'a> {
    total: u32,
    progress_bar: &'a ProgressBar,
}

impl ProgressBar {
    pub fn start(&self, new_total: u32) -> ResetGuard {
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

        ResetGuard {
            total: new_total,
            progress_bar: self,
        }
    }

    pub fn reset(&self) {
        self.start(0); // 0 can be any u32.
    }

    pub fn progress(&self) -> Progress {
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
        let total = (old >> 32) as u32;
        let count = old as u32;
        if count >= total {
            unreachable!("progress exceeds limit: {count} >= {total}");
        }

        Ok(())
    }

    fn interrupt(&self) {
        self.0.store(i64::MIN, Ordering::Relaxed);
        for i in 0.. {
            std::thread::sleep(std::time::Duration::from_millis(1));
            let progress = to_progress(self.0.load(Ordering::Relaxed));
            if matches!(progress, Progress::Uninitialized) {
                debug!("interrupted after {i} checks");
                break;
            }
        }
    }
}

impl<'a> Drop for ResetGuard<'a> {
    fn drop(&mut self) {
        let _ = self
            .progress_bar
            .0
            .fetch_update(Ordering::SeqCst, Ordering::Acquire, |x| {
                match to_progress(x) {
                    Progress::Finished { total } if total == self.total => None,
                    Progress::Uninitialized => None,
                    _ => Some(0),
                }
            });
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
    use std::{assert_matches::assert_matches, thread, time::Duration};

    use super::*;

    #[test]
    fn test_interrupt() {
        let progress_bar = ProgressBar::default();
        assert_matches!(progress_bar.progress(), Progress::Uninitialized);

        thread::scope(|s| {
            s.spawn(|| {
                let _reset_guard = progress_bar.start(666666);
                loop {
                    match progress_bar.add(1) {
                        Ok(_) => thread::sleep(Duration::from_millis(1)),
                        Err(e) => {
                            println!("{e}");
                            break;
                        }
                    }
                }
                // reset.
            });
            thread::sleep(Duration::from_millis(100));

            assert_matches!(progress_bar.progress(), Progress::InProgress { .. });
            const TOTAL: u32 = 2333333;
            let _reset_guard = progress_bar.start(TOTAL);
            assert_matches!(
                progress_bar.progress(),
                Progress::InProgress {
                    total: TOTAL,
                    count: 0
                }
            );
            progress_bar.add(1).unwrap();
            assert_matches!(
                progress_bar.progress(),
                Progress::InProgress {
                    total: TOTAL,
                    count: 1
                }
            );
            drop(_reset_guard);
            assert_matches!(progress_bar.progress(), Progress::Uninitialized);
        });
    }

    #[test]
    fn test_reset() {
        let progress_bar = ProgressBar::default();
        assert_matches!(progress_bar.progress(), Progress::Uninitialized);

        thread::scope(|s| {
            s.spawn(|| {
                let _reset_guard = progress_bar.start(666666);
                loop {
                    match progress_bar.add(1) {
                        Ok(_) => thread::sleep(Duration::from_millis(1)),
                        Err(e) => {
                            println!("{e}");
                            break;
                        }
                    }
                }
                // reset.
            });
            thread::sleep(Duration::from_millis(100));

            assert_matches!(progress_bar.progress(), Progress::InProgress { .. });
            progress_bar.start(0);
            assert_matches!(progress_bar.progress(), Progress::Uninitialized);
        });
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
