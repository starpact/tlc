use std::{
    fmt::Debug,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{anyhow, bail, Result};
use ndarray::ArcArray2;
use serde::Deserialize;
use tauri::async_runtime::spawn_blocking;

use crate::{
    daq::{DaqManager, DaqMetadata},
    setting::{self, SettingStorage, StartIndex},
    solve::{self, IterationMethod, PhysicalParam},
    video::{FilterMethod, Progress, VideoManager, VideoMetadata},
};

pub struct GlobalState<S: SettingStorage> {
    setting_storage: Arc<Mutex<S>>,
    video_manager: VideoManager<S>,
    daq_manager: DaqManager<S>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(Default))]
pub struct CreateSettingRequest {
    pub name: String,
    pub save_root_dir: String,
    pub video_path: PathBuf,
    pub daq_path: PathBuf,
    pub filter_method: FilterMethod,
    pub iteration_method: IterationMethod,
    pub physical_param: PhysicalParam,
}

impl<S: SettingStorage> GlobalState<S> {
    pub fn new(setting_storage: S) -> Self {
        let setting_storage = Arc::new(Mutex::new(setting_storage));
        let video_manager = VideoManager::new(setting_storage.clone());
        let daq_manager = DaqManager::new(setting_storage.clone());

        GlobalState {
            setting_storage,
            video_manager,
            daq_manager,
        }
    }

    pub async fn create_setting(&self, request: CreateSettingRequest) -> Result<()> {
        let video_path = request.video_path;
        let daq_path = request.daq_path;
        let create_request = setting::CreateRequest {
            name: request.name,
            save_root_dir: request.save_root_dir,
            filter_method: request.filter_method,
            iteration_method: request.iteration_method,
            physical_param: request.physical_param,
        };

        let setting_id = self
            .asyncify(|mut s| s.create_setting(create_request))
            .await?;

        let load_video_and_daq = async move {
            self.video_manager
                .spawn_load_packets(Some(video_path))
                .await?;
            self.daq_manager.read_daq(Some(daq_path)).await?;
            Ok(())
        };

        if let e @ Err(_) = load_video_and_daq.await {
            // Rollback.
            self.asyncify(move |mut s| s.delete_setting(setting_id))
                .await?;
            return e;
        }

        Ok(())
    }

    pub async fn switch_setting(&self, setting_id: i64) -> Result<()> {
        self.asyncify(move |mut s| s.switch_setting(setting_id))
            .await?;

        self.video_manager.spawn_load_packets(None).await?;
        self.daq_manager.read_daq(None).await?;

        Ok(())
    }

    pub async fn get_save_root_dir(&self) -> Result<String> {
        self.asyncify(move |s| s.save_root_dir()).await
    }

    pub async fn set_save_root_dir(&self, save_root_dir: PathBuf) -> Result<()> {
        if !save_root_dir.is_dir() {
            bail!("save_root_dir is not a valid directory: {save_root_dir:?}");
        }
        self.asyncify(|s| s.set_save_root_dir(save_root_dir)).await
    }

    pub async fn get_video_metadata(&self) -> Result<VideoMetadata> {
        self.asyncify(|s| s.video_metadata()).await
    }

    pub async fn set_video_path(&self, video_path: PathBuf) -> Result<()> {
        self.video_manager
            .spawn_load_packets(Some(video_path))
            .await
    }

    pub async fn get_daq_metadata(&self) -> Result<DaqMetadata> {
        self.asyncify(|s| s.daq_metadata()).await
    }

    pub async fn set_daq_path(&self, daq_path: PathBuf) -> Result<()> {
        self.daq_manager.read_daq(Some(daq_path)).await
    }

    pub async fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
        self.video_manager
            .read_single_frame_base64(frame_index)
            .await
    }

    pub async fn get_daq_data(&self) -> Result<ArcArray2<f64>> {
        self.daq_manager
            .daq_data()
            .ok_or_else(|| anyhow!("daq path unset"))
    }

    pub async fn synchronize_video_and_daq(
        &self,
        start_frame: usize,
        start_row: usize,
    ) -> Result<()> {
        self.asyncify(move |s| s.synchronize_video_and_daq(start_frame, start_row))
            .await
    }

    pub async fn get_start_index(&self) -> Result<StartIndex> {
        self.asyncify(|s| s.start_index()).await
    }

    pub async fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        self.asyncify(move |s| s.set_start_frame(start_frame))
            .await?;
        self.video_manager.spawn_build_green2().await
    }

    pub async fn set_start_row(&self, start_row: usize) -> Result<()> {
        self.asyncify(move |s| s.set_start_row(start_row)).await?;
        self.video_manager.spawn_build_green2().await
    }

    pub async fn get_area(&self) -> Result<(usize, usize, usize, usize)> {
        self.asyncify(|s| s.area()).await
    }

    pub async fn set_area(&self, area: (usize, usize, usize, usize)) -> Result<()> {
        self.asyncify(move |s| s.set_area(area)).await?;
        self.video_manager.spawn_build_green2().await
    }

    pub async fn spawn_build_green2(&self) -> Result<()> {
        self.video_manager.spawn_build_green2().await
    }

    pub fn get_build_green2_progress(&self) -> Progress {
        self.video_manager.build_progress()
    }

    pub async fn filter_method(&self) -> Result<FilterMethod> {
        self.asyncify(|s| Ok(s.filter_metadata()?.filter_method))
            .await
    }

    pub async fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        self.asyncify(move |s| s.set_filter_method(filter_method))
            .await?;
        self.video_manager.spawn_filter_green2().await
    }

    pub async fn filter_single_point(&self, position: (usize, usize)) -> Result<Vec<u8>> {
        self.video_manager.filter_single_point(position).await
    }

    pub async fn spawn_filter_green2(&self) -> Result<()> {
        self.video_manager.spawn_filter_green2().await
    }

    pub fn get_filter_green2_progress(&self) -> Progress {
        self.video_manager.filter_progress()
    }

    pub async fn get_iteration_method(&self) -> Result<IterationMethod> {
        self.asyncify(|s| s.iteration_method()).await
    }

    pub async fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        self.asyncify(move |s| s.set_iteration_method(iteration_method))
            .await
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
            let frame_rate = setting_storage.video_metadata()?.frame_rate;
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

    async fn asyncify<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(MutexGuard<S>) -> Result<T> + Send + 'static,
    {
        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || {
            let setting_storage = setting_storage.lock().unwrap();
            f(setting_storage)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use crate::{
        setting::{MockSettingStorage, SqliteSettingStorage},
        util::{self, log},
    };

    use super::*;

    const SAMPLE_VIDEO_PATH: &str = "./tests/almost_empty.avi";

    #[tokio::test]
    async fn test_create_setting_video_not_found() {
        log::init();

        let mut mock = MockSettingStorage::new();
        mock.expect_create_setting().once().return_once(|_| Ok(10));
        mock.expect_delete_setting()
            .once()
            .with(eq(10))
            .return_once(|_| Ok(()));

        let global_state = GlobalState::new(mock);
        global_state
            .create_setting(CreateSettingRequest {
                video_path: PathBuf::from("not_found.avi"),
                ..Default::default()
            })
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn test_create_setting_daq_not_found() {
        log::init();

        let video_metadata = VideoMetadata {
            path: PathBuf::from(SAMPLE_VIDEO_PATH),
            frame_rate: 25,
            nframes: 3,
            shape: (1024, 1280),
            fingerprint: "TODO".to_owned(),
        };

        let mut mock = MockSettingStorage::new();
        mock.expect_create_setting().once().return_once(|_| Ok(10));
        mock.expect_set_video_metadata()
            .with(eq(video_metadata.clone()))
            .return_once(|_| Ok(()));
        mock.expect_video_metadata()
            .returning(move || Ok(video_metadata.clone()));
        mock.expect_delete_setting()
            .once()
            .with(eq(10))
            .return_once(|_| Ok(()));

        let global_state = GlobalState::new(mock);
        global_state
            .create_setting(CreateSettingRequest {
                video_path: PathBuf::from(SAMPLE_VIDEO_PATH),
                daq_path: PathBuf::from("not_found.lvm"),
                ..Default::default()
            })
            .await
            .unwrap_err();
    }

    #[tokio::test]
    #[ignore]
    async fn test_trigger_try_spawn_build_green2() {
        util::log::init();
        let global_state = GlobalState::new(SqliteSettingStorage::new());
        println!("{:#?}", global_state.setting_storage);

        global_state
            .set_video_path(PathBuf::from(
                "/home/yhj/Documents/2021yhj/EXP/imp/videos/imp_50000_1_up.avi",
            ))
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
