use std::path::Path;

use anyhow::{anyhow, Result};
use ndarray::ArcArray2;
use tracing::{debug, error};

use crate::{
    config::Config,
    daq::{DaqDataManager, DaqMetadata},
    solve,
    video::{VideoDataManager, VideoMetadata},
};

pub struct GlobalState {
    config: Config,
    video_data_manager: VideoDataManager,
    daq_data_manager: DaqDataManager,
}

impl GlobalState {
    pub fn new() -> Self {
        GlobalState {
            config: Config::from_default_path().unwrap_or_default(),
            video_data_manager: VideoDataManager::new(),
            daq_data_manager: DaqDataManager::default(),
        }
    }

    pub async fn try_load_video(&mut self) -> Result<()> {
        let video_path = &self
            .config
            .video_metadata()
            .ok_or_else(|| anyhow!("videop path unset"))?
            .path;
        match self.video_data_manager.spawn_load_packets(video_path).await {
            Ok(video_metadata) => self.config.set_video_metadata(Some(video_metadata)),
            Err(e) => {
                error!("Failed to read video metadata: {}", e);
                self.config.set_video_metadata(None);
            }
        }

        Ok(())
    }

    pub async fn try_load_daq(&mut self) -> Result<()> {
        let daq_path = &self
            .config
            .daq_metadata()
            .ok_or_else(|| anyhow!("daq path unset"))?
            .path;
        match self.daq_data_manager.read_daq(daq_path).await {
            Ok(daq_data) => {
                self.config.set_daq_metadata(Some(DaqMetadata {
                    path: daq_path.clone(),
                    nrows: daq_data.nrows(),
                }));
            }
            Err(e) => {
                error!("Failed to read daq metadata: {}", e);
                self.config.set_daq_metadata(None);
            }
        }

        Ok(())
    }

    pub async fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P) -> Result<VideoMetadata> {
        if let Some(video_metadata) = self.config.video_metadata() {
            if video_metadata.path == video_path.as_ref() {
                return Ok(video_metadata.clone());
            }
        }

        let video_metadata = self
            .video_data_manager
            .spawn_load_packets(video_path)
            .await?;
        self.config.set_video_metadata(Some(video_metadata.clone()));

        Ok(video_metadata)
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&mut self, daq_path: P) -> Result<DaqMetadata> {
        if let Some(daq_metadata) = self.config.daq_metadata() {
            if daq_metadata.path == daq_path.as_ref() {
                return Ok(daq_metadata.clone());
            }
        }

        let daq_data = self.daq_data_manager.read_daq(&daq_path).await?;
        let daq_metadata = DaqMetadata {
            path: daq_path.as_ref().to_owned(),
            nrows: daq_data.nrows(),
        };
        self.config.set_daq_metadata(Some(daq_metadata.clone()));

        Ok(daq_metadata)
    }

    pub async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        self.video_data_manager.read_single_frame(frame_index).await
    }

    pub fn get_daq_data(&self) -> Result<ArcArray2<f64>> {
        self.daq_data_manager
            .get_daq_data()
            .ok_or_else(|| anyhow!("daq not loaded"))
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
        if let Err(e) = self.try_spawn_build_green2() {
            debug!("Not ready to build green2 yet: {}", e);
        }

        Ok(())
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        self.config.set_start_row(start_row)?;
        if let Err(e) = self.try_spawn_build_green2() {
            debug!("Not ready to build green2 yet: {}", e);
        }

        Ok(())
    }

    fn try_spawn_build_green2(&self) -> Result<()> {
        let green2_param = self.config.green2_param()?;
        debug!("Start building green2: {:?}", green2_param);
        self.video_data_manager.spawn_build_green2(green2_param);

        Ok(())
    }

    pub async fn solve(&mut self) -> Result<()> {
        let physical_param = self
            .config
            .physical_param()
            .ok_or_else(|| anyhow!("physical parameters unset"))?;
        let frame_rate = self
            .config
            .video_metadata()
            .ok_or_else(|| anyhow!("video not loaded"))?
            .frame_rate;

        solve::solve(physical_param, self.config.iteration_method(), frame_rate);

        todo!()
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
        global_state.try_load_video().await.unwrap();
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
            global_state.video_data_manager.data().green2().unwrap()
        );
    }
}
