use std::{path::Path, sync::Arc};

use anyhow::Result;
use ndarray::{prelude::*, ArcArray2};
use parking_lot::{Mutex, RwLock};
use tracing::{debug, error};

use crate::{
    config::TlcConfig,
    daq::{self, DaqMetadata},
    video::{self, VideoCache, VideoMetadata},
};

#[derive(Default)]
pub struct TlcState {
    config: TlcConfig,
    video_cache: Arc<RwLock<VideoCache>>,
    green2: Arc<Mutex<Option<Array2<u8>>>>,
    temperature2: Option<ArcArray2<f64>>,
}

impl TlcState {
    pub async fn new() -> Self {
        let mut tlc_state = TlcState {
            config: TlcConfig::from_default_path().unwrap_or_default(),
            ..Default::default()
        };
        if let Some(video_metadata) = tlc_state.config.video_metadata() {
            match video::spawn_load_packets(tlc_state.video_cache.clone(), &video_metadata.path)
                .await
            {
                Ok(video_metadata) => tlc_state.config.set_video_metadata(Some(video_metadata)),
                Err(e) => {
                    error!("Failed to read video metadata: {}", e);
                    tlc_state.config.set_video_metadata(None);
                }
            }
        }
        if let Some(daq_metadata) = tlc_state.config.daq_metadata() {
            match daq::read_daq(&daq_metadata.path) {
                Ok(temperature2) => {
                    let path = daq_metadata.path.clone();
                    tlc_state.config.set_daq_metadata(Some(DaqMetadata {
                        path,
                        nrows: temperature2.nrows(),
                    }));
                    tlc_state.temperature2 = Some(temperature2.into_shared());
                }
                Err(e) => {
                    error!("Failed to read daq metadata: {}", e);
                    tlc_state.config.set_daq_metadata(None);
                }
            }
        }

        tlc_state
    }

    pub async fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P) -> Result<VideoMetadata> {
        if let Some(video_metadata) = self.config.video_metadata() {
            if video_metadata.path == video_path.as_ref() {
                return Ok(video_metadata.clone());
            }
        }

        let video_cache = self.video_cache.clone();
        let video_metadata = video::spawn_load_packets(video_cache, video_path).await?;
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
            let video_cache = self.video_cache.clone();
            let shared_green2 = self.green2.clone();
            std::thread::spawn(move || {
                if let Ok(green2) = video::build_green2(video_cache, green2_param) {
                    *shared_green2.lock() = Some(green2);
                }
            });
        }
    }

    pub async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        video::read_single_frame(self.video_cache.clone(), frame_index).await
    }
}

#[cfg(test)]
mod tests {
    use crate::util;

    use super::*;

    #[tokio::test]
    async fn test_trigger_try_spawn_build_green2() {
        util::log::init();
        let mut tlc_state = TlcState::new().await;
        println!("{:#?}", tlc_state.config);
        let video_metadata = tlc_state
            .set_video_path("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();
        println!("{:#?}", video_metadata);

        tlc_state.synchronize_video_and_daq(10, 20).unwrap();
        tlc_state.set_start_frame(10).unwrap();
        tlc_state.set_start_row(0).unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        println!("{:?}", tlc_state.green2);
    }
}
