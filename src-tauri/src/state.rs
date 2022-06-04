use std::{path::Path, sync::Arc};

use anyhow::Result;
use ndarray::ArcArray2;
use tracing::{debug, error};

use crate::{
    config::Config,
    daq::{self, DaqMetadata},
    video::{self, VideoDataManager, VideoMetadata},
};

#[derive(Default)]
pub struct GlobalState {
    config: Config,

    video_data_manager: Arc<VideoDataManager>,

    temperature2: Option<ArcArray2<f64>>,
}

impl GlobalState {
    pub fn new() -> Self {
        GlobalState {
            config: Config::from_default_path().unwrap_or_default(),
            ..Default::default()
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
            match daq::read_daq(&daq_metadata.path) {
                Ok(temperature2) => {
                    let path = daq_metadata.path.clone();
                    self.config.set_daq_metadata(Some(DaqMetadata {
                        path,
                        nrows: temperature2.nrows(),
                    }));
                    self.temperature2 = Some(temperature2.into_shared());
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
            let video_cache = self.video_data_manager.clone();
            std::thread::spawn(move || {
                if let Err(e) = video_cache.build_green2(green2_param) {
                    debug!("{}", e);
                }
            });
        }
    }

    pub async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        video::read_single_frame(self.video_data_manager.clone(), frame_index).await
    }
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
