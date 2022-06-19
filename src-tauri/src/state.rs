use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Result};
use ndarray::ArcArray2;
use rayon::ThreadPoolBuilder;
use tokio::sync::oneshot;
use tracing::{debug, error};

use crate::{
    config::Config,
    daq::{self, DaqMetadata},
    video::{self, VideoDataManager, VideoMetadata},
};

pub struct GlobalState {
    config: Config,

    video_data_manager: Arc<VideoDataManager>,

    frame_grabber: flume::Sender<(usize, oneshot::Sender<Result<String>>)>,

    temperature2: Option<ArcArray2<f64>>,
}

impl GlobalState {
    pub fn new() -> Self {
        let video_data_manager = Arc::new(VideoDataManager::default());

        let (tx, rx) = flume::unbounded();
        spawn_read_frame_daemon(rx, video_data_manager.clone());

        GlobalState {
            config: Config::from_default_path().unwrap_or_default(),
            video_data_manager,
            frame_grabber: tx,
            temperature2: None,
        }
    }

    pub async fn try_load_data(&mut self) {
        if let Some(video_metadata) = self.config.video_metadata() {
            match video::spawn_load_packets(self.video_data_manager.clone(), &video_metadata.path)
                .await
            {
                Ok(video_metadata) => self.config.set_video_metadata(Some(video_metadata)),
                Err(e) => {
                    error!("Failed to read video metadata: {}", e);
                    self.config.set_video_metadata(None);
                }
            }
        }

        if let Some(daq_metadata) = self.config.daq_metadata() {
            match daq::read_daq(&daq_metadata.path).await {
                Ok(daq_data) => {
                    let path = daq_metadata.path.clone();
                    self.config.set_daq_metadata(Some(DaqMetadata {
                        path,
                        nrows: daq_data.nrows(),
                    }));
                    self.temperature2 = Some(daq_data.into_shared());
                }
                Err(e) => {
                    error!("Failed to read daq metadata: {}", e);
                    self.config.set_daq_metadata(None);
                }
            }
        }
    }

    pub async fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P) -> Result<VideoMetadata> {
        if let Some(video_metadata) = self.config.video_metadata() {
            if video_metadata.path == video_path.as_ref() {
                return Ok(video_metadata.clone());
            }
        }

        let video_data_manager = self.video_data_manager.clone();
        let video_metadata = video::spawn_load_packets(video_data_manager, video_path).await?;
        self.config.set_video_metadata(Some(video_metadata.clone()));

        Ok(video_metadata)
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&mut self, daq_path: P) -> Result<DaqMetadata> {
        if let Some(daq_metadata) = self.config.daq_metadata() {
            if daq_metadata.path == daq_path.as_ref() {
                return Ok(daq_metadata.clone());
            }
        }

        let daq_data = daq::read_daq(&daq_path).await?;
        let daq_metadata = DaqMetadata {
            path: daq_path.as_ref().to_owned(),
            nrows: daq_data.nrows(),
        };
        self.config.set_daq_metadata(Some(daq_metadata.clone()));

        Ok(daq_metadata)
    }

    pub fn synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
    ) -> Result<()> {
        self.config
            .synchronize_video_and_daq(start_frame, start_row)
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        self.config.set_start_frame(start_frame)?;
        self.try_spawn_build_green2();

        Ok(())
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        self.config.set_start_row(start_row)?;
        self.try_spawn_build_green2();

        Ok(())
    }

    fn try_spawn_build_green2(&self) {
        if let Ok(green2_param) = self.config.green2_param() {
            debug!("Start building green2: {:?}", green2_param);
            let video_data_manager = self.video_data_manager.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = video_data_manager.build_green2(green2_param) {
                    debug!("{}", e);
                }
            });
        }
    }

    pub async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        self.frame_grabber
            .send((frame_index, tx))
            .expect("Frame grab daemon exited unexptedly");
        rx.await.map_err(|_| anyhow!("no idle worker thread"))?
    }
}

fn spawn_read_frame_daemon(
    rx: flume::Receiver<(usize, oneshot::Sender<Result<String>>)>,
    video_data_manager: Arc<VideoDataManager>,
) {
    std::thread::spawn(move || {
        // As thread-local decoders are designed to be kept in just a few threads,
        // so a stand-alone `rayon` thread pool is used.
        // `spawn` form `rayon`'s global thread pool will block when something like
        // `par_iter` is working as `rayon` uses `depth-first` strategy for highest
        // efficiency. So another dedicated thread pool is used.
        const NUM_THREADS: usize = 4;
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(NUM_THREADS)
            .build()
            .expect("Failed to init rayon thread pool");

        // When user drags the progress bar quickly, the decoding can not keep up
        // and there will be significant lag. Actually, we do not have to decode
        // every frames, and the key is how to give up decoding some frames properly.
        // The naive solution to avoid too much backlog is maintaining the number of
        // pending tasks and directly abort current decoding if it already exceeds the
        // limit. But it's not perfect for this use case because it can not guarantee
        // decoding the frame where the progress bar **stops**.
        // To solve this, we introduce an unbounded channel to accept all frame indexes
        // but only **the latest few** will be actually decoded.
        while let Ok((frame_index, tx)) = rx.recv() {
            let video_data_manager = video_data_manager.clone();
            thread_pool.spawn(move || {
                let ret = video_data_manager.read_single_frame(frame_index);
                let _ = tx.send(ret);
            });

            loop {
                let len = rx.len();
                if len <= NUM_THREADS {
                    break;
                }
                for _ in 0..len - NUM_THREADS {
                    let _ = rx.recv();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::util;

    use super::*;

    #[tokio::test]
    async fn test_trigger_try_spawn_build_green2() {
        util::log::init();
        let mut global_state = GlobalState::new();
        global_state.try_load_data().await;
        println!("{:#?}", global_state.config);
        let video_metadata = global_state
            .set_video_path("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();
        println!("{:#?}", video_metadata);

        global_state.synchronize_video_and_daq(10, 20).unwrap();
        global_state.set_start_frame(10).unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        println!(
            "{:?}",
            global_state
                .video_data_manager
                .video_data
                .read()
                .unwrap()
                .green2()
                .unwrap()
        );
    }
}
