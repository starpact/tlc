use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Result};
use ndarray::ArcArray2;
use tauri::async_runtime::spawn_blocking;
use tracing::info;

use crate::{
    daq::{DaqManager, DaqMetadata},
    setting::{SettingStorage, StartIndex},
    solve::{self, IterationMethod},
    util::progress_bar::Progress,
    video::{FilterMethod, VideoManager, VideoMetadata},
};

pub struct GlobalState {
    setting_storage: Arc<Mutex<SettingStorage>>,
    video_manager: VideoManager,
    daq_manager: Arc<Mutex<DaqManager>>,
}

impl GlobalState {
    pub fn new() -> Self {
        GlobalState {
            setting_storage: Arc::new(Mutex::new(SettingStorage::new())),
            video_manager: Default::default(),
            daq_manager: Default::default(),
        }
    }

    pub async fn get_save_root_dir(&self) -> Result<String> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || setting_storage.lock().unwrap().save_root_dir()).await?
    }

    pub async fn set_save_root_dir<P: AsRef<Path>>(&self, save_root_dir: P) -> Result<()> {
        let save_root_dir = save_root_dir.as_ref();
        if !save_root_dir.is_dir() {
            bail!("{save_root_dir:?} is not a valid directory");
        }
        let save_root_dir = save_root_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid save_root_dir: {save_root_dir:?}"))?
            .to_owned();

        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .set_save_root_dir(save_root_dir)
        })
        .await?
    }

    pub async fn get_video_metadata(&self) -> Result<VideoMetadata> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .video_metadata()?
                .ok_or_else(|| anyhow!("video path unset"))
        })
        .await?
    }

    pub async fn set_video_path<P: AsRef<Path>>(&self, video_path: P) -> Result<()> {
        let video_metadata = self.video_manager.spawn_load_packets(video_path).await?;
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .set_video_metadata(video_metadata)
        })
        .await?
    }

    pub async fn get_daq_metadata(&self) -> Result<DaqMetadata> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .daq_metadata()?
                .ok_or_else(|| anyhow!("daq path unset"))
        })
        .await?
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&self, daq_path: P) -> Result<()> {
        let daq_manager = self.daq_manager.clone();
        let daq_path = daq_path.as_ref().to_owned();
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            // Lock of `daq_manager` should be released after db write.
            let mut daq_manager = daq_manager.lock().unwrap();
            let daq_metadata = daq_manager.read_daq(daq_path)?;
            setting_storage
                .lock()
                .unwrap()
                .set_daq_metadata(daq_metadata)
        })
        .await?
    }

    pub async fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        self.video_manager
            .read_single_frame_base64(frame_index)
            .await
    }

    pub async fn get_daq_data(&self) -> Result<ArcArray2<f64>> {
        self.daq_manager
            .lock()
            .unwrap()
            .daq_data()
            .ok_or_else(|| anyhow!("daq path unset"))
    }

    pub async fn synchronize_video_and_daq(
        &self,
        start_frame: usize,
        start_row: usize,
    ) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .synchronize_video_and_daq(start_frame, start_row)
        })
        .await?
    }

    pub async fn get_start_index(&self) -> Result<StartIndex> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .start_index()?
                .ok_or_else(|| anyhow!("video and daq not synchronized yet"))
        })
        .await?
    }

    pub async fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        let green2_metadata = spawn_blocking(move || {
            let setting_storage = setting_storage.lock().unwrap();
            setting_storage.set_start_frame(start_frame)?;
            setting_storage.green2_metadata()
        })
        .await?;

        match green2_metadata {
            Ok(green2_param) => self.video_manager.spawn_build_green2(green2_param).await?,
            Err(e) => info!("Not ready to build green2 yet: {e}"),
        }

        Ok(())
    }

    pub async fn set_start_row(&self, start_row: usize) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        let green2_metadata = spawn_blocking(move || {
            let setting_storage = setting_storage.lock().unwrap();
            setting_storage.set_start_row(start_row)?;
            setting_storage.green2_metadata()
        })
        .await?;

        match green2_metadata {
            Ok(green2_param) => self.video_manager.spawn_build_green2(green2_param).await?,
            Err(e) => info!("Not ready to build green2 yet: {e}"),
        }

        Ok(())
    }

    pub async fn get_area(&self) -> Result<(usize, usize, usize, usize)> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .area()?
                .ok_or_else(|| anyhow!("area not selected yet"))
        })
        .await?
    }

    pub async fn set_area(&self, area: (usize, usize, usize, usize)) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        let green2_metadata = spawn_blocking(move || {
            let setting_storage = setting_storage.lock().unwrap();
            setting_storage.set_area(area)?;
            setting_storage.green2_metadata()
        })
        .await?;

        match green2_metadata {
            Ok(green2_metadata) => {
                self.video_manager
                    .spawn_build_green2(green2_metadata)
                    .await?
            }
            Err(e) => info!("Not ready to build green2 yet: {e}"),
        }

        Ok(())
    }

    pub async fn spawn_build_green2(&self) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        let green2_metadata =
            spawn_blocking(move || setting_storage.lock().unwrap().green2_metadata()).await??;
        self.video_manager.spawn_build_green2(green2_metadata).await
    }

    pub fn get_build_green2_progress(&self) -> Progress {
        self.video_manager.build_progress()
    }

    pub async fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .set_filter_method(filter_method)
        })
        .await??;

        self.video_manager.spawn_filter_green2(filter_method).await
    }

    pub async fn filter_single_point(&self, position: (usize, usize)) -> Result<Vec<u8>> {
        let setting_storage = self.setting_storage.clone();
        let filter_method =
            spawn_blocking(move || setting_storage.lock().unwrap().filter_method()).await??;
        self.video_manager
            .filter_single_point(filter_method, position)
            .await
    }

    pub async fn filter(&self) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        let filter_method =
            spawn_blocking(move || setting_storage.lock().unwrap().filter_method()).await??;

        self.video_manager.spawn_filter_green2(filter_method).await
    }

    pub fn get_filter_green2_progress(&self) -> Progress {
        self.video_manager.filter_progress()
    }

    pub async fn get_iteration_method(&self) -> Result<IterationMethod> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || setting_storage.lock().unwrap().iteration_method()).await?
    }

    pub async fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            setting_storage
                .lock()
                .unwrap()
                .set_iteration_method(iteration_method)
        })
        .await?
    }

    pub async fn solve(&self) -> Result<()> {
        let filtered_green2 = self
            .video_manager
            .filtered_green2()
            .ok_or_else(|| anyhow!("green2 not built or filtered yet"))?;

        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || -> Result<()> {
            let setting_storage = setting_storage.lock().unwrap();

            let physical_param = setting_storage.physical_param()?;
            let frame_rate = setting_storage
                .video_metadata()?
                .ok_or_else(|| anyhow!("video path unset"))?
                .frame_rate;
            let iteration_method = setting_storage.iteration_method()?;

            solve::solve(
                filtered_green2,
                physical_param,
                iteration_method,
                frame_rate,
            );

            Ok(())
        })
        .await??;

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
        let global_state = GlobalState::new();
        println!("{:#?}", global_state.setting_storage);
        global_state
            .set_video_path("/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi")
            .await
            .unwrap();

        global_state
            .synchronize_video_and_daq(10, 20)
            .await
            .unwrap();
        global_state.set_start_frame(10).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    }
}
