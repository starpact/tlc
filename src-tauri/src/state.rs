use std::{
    fmt::Debug,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{anyhow, bail, Result};
use ndarray::{ArcArray2, Array2};
use serde::Deserialize;
use tauri::async_runtime::spawn_blocking;

use crate::{
    daq::{DaqManager, DaqMetadata, InterpolationMethod},
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

        self.asyncify(|mut s| s.create_setting(create_request))
            .await?;

        // We cannot use `try_join` because we need to wait until both tasks are finished
        // to make sure every db operation finished before rollback starts.
        match tokio::join!(
            self.video_manager.spawn_read_video(Some(video_path)),
            self.daq_manager.read_daq(Some(daq_path)),
        ) {
            (Ok(_), Ok(_)) => Ok(()),
            // TBD: only return one error.
            (Err(e), _) | (_, Err(e)) => {
                // Rollback.
                self.asyncify(|mut s| s.delete_setting()).await?;
                Err(e)
            }
        }
    }

    pub async fn switch_setting(&self, setting_id: i64) -> Result<()> {
        self.asyncify(move |mut s| s.switch_setting(setting_id))
            .await?;

        self.video_manager.spawn_read_video(None).await?;
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
        self.asyncify(move |s| s.set_save_root_dir(&save_root_dir))
            .await
    }

    pub async fn get_video_metadata(&self) -> Result<VideoMetadata> {
        self.asyncify(|s| s.video_metadata()).await
    }

    pub async fn set_video_path(&self, video_path: PathBuf) -> Result<()> {
        self.video_manager.spawn_read_video(Some(video_path)).await
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
            .daq_raw()
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
        self.video_manager.build_green2_progress()
    }

    pub async fn get_filter_method(&self) -> Result<FilterMethod> {
        self.asyncify(|s| Ok(s.filter_metadata()?.filter_method))
            .await
    }

    pub async fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        self.asyncify(move |s| s.set_filter_method(filter_method))
            .await?;
        self.video_manager.spawn_detect_peak().await
    }

    pub async fn filter_single_point(&self, position: (usize, usize)) -> Result<Vec<u8>> {
        self.video_manager.filter_single_point(position).await
    }

    pub async fn spawn_filter_green2(&self) -> Result<()> {
        self.video_manager.spawn_detect_peak().await
    }

    pub fn get_detect_peak_progress(&self) -> Progress {
        self.video_manager.detect_peak_progress_bar()
    }

    pub async fn get_interpolation_method(&self) -> Result<InterpolationMethod> {
        self.asyncify(|s| s.interpolation_method()).await
    }

    pub async fn set_interpolation_method(
        &self,
        interpolation_method: InterpolationMethod,
    ) -> Result<()> {
        self.asyncify(move |s| s.set_interpolation_method(interpolation_method))
            .await?;
        self.daq_manager.interpolate().await?;

        Ok(())
    }

    pub async fn interpolate_single_frame(&self, frame_index: usize) -> Result<Array2<f64>> {
        let daq_manager = self.daq_manager.clone();
        spawn_blocking(move || {
            daq_manager
                .interpolator()
                .ok_or_else(|| anyhow!("interpolator not interpolated yet"))?
                .interpolate_single_frame(frame_index)
        })
        .await?
    }

    pub async fn interpolate(&self) -> Result<()> {
        self.daq_manager.interpolate().await
    }

    pub async fn get_iteration_method(&self) -> Result<IterationMethod> {
        self.asyncify(|s| s.iteration_method()).await
    }

    pub async fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        self.asyncify(move |s| s.set_iteration_method(iteration_method))
            .await
    }

    pub async fn solve(&self) -> Result<()> {
        let gmax_frame_indexes = self
            .video_manager
            .gmax_frame_indexes()
            .ok_or_else(|| anyhow!("gmax_frame_indexes not built yet"))?;
        let interpolator = self
            .daq_manager
            .interpolator()
            .ok_or_else(|| anyhow!("interpolator not built yet"))?;

        let setting_storage = self.setting_storage.clone();
        spawn_blocking(move || -> Result<()> {
            let setting_storage = setting_storage.lock().unwrap();

            let physical_param = setting_storage.physical_param()?;
            let frame_rate = setting_storage.video_metadata()?.frame_rate;
            let iteration_method = setting_storage.iteration_method()?;

            solve::solve(
                gmax_frame_indexes,
                interpolator,
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
    use std::time::Duration;

    use mockall::predicate::eq;

    use super::*;
    use crate::{setting::MockSettingStorage, util};

    // For unit tests.
    const SAMPLE_VIDEO_PATH: &str = "./tests/almost_empty.avi";
    // Too large, just for integration tests.
    const VIDEO_PATH: &str =
        "/home/yhj/Downloads/2021_YanHongjie/EXP/imp/videos/imp_20000_1_up.avi";
    // For both unit and integration tests.
    const DAQ_PATH: &str = "./tests/imp_20000_1.lvm";

    #[tokio::test]
    async fn test_create_setting_video_not_found() {
        util::log::init();

        let mut mock = MockSettingStorage::new();
        mock.expect_create_setting().once().return_once(|_| Ok(()));
        mock.expect_delete_setting().once().return_once(|| Ok(()));

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
        util::log::init();

        let video_metadata = VideoMetadata {
            path: PathBuf::from(SAMPLE_VIDEO_PATH),
            frame_rate: 25,
            nframes: 3,
            shape: (1024, 1280),
        };

        let mut mock = MockSettingStorage::new();
        mock.expect_create_setting().once().return_once(|_| Ok(()));
        mock.expect_set_video_metadata()
            .with(eq(video_metadata.clone()))
            .return_once(|_| Ok(()));
        mock.expect_video_metadata()
            .returning(move || Ok(video_metadata.clone()));
        mock.expect_delete_setting().once().return_once(|| Ok(()));

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
    async fn test_create_setting_ok() {
        util::log::init();

        let video_path = PathBuf::from(SAMPLE_VIDEO_PATH);
        let daq_path = PathBuf::from(DAQ_PATH);

        let video_metadata = VideoMetadata {
            path: video_path.clone(),
            frame_rate: 25,
            nframes: 3,
            shape: (1024, 1280),
        };
        let daq_metadata = DaqMetadata {
            path: daq_path.clone(),
            nrows: 2589,
            ncols: 10,
        };

        let mut mock = MockSettingStorage::new();
        mock.expect_create_setting().once().return_once(|_| Ok(()));
        mock.expect_set_video_metadata()
            .with(eq(video_metadata.clone()))
            .return_once(|_| Ok(()));
        mock.expect_video_metadata()
            .returning(move || Ok(video_metadata.clone()));
        mock.expect_set_daq_metadata()
            .with(eq(daq_metadata))
            .return_once(|_| Ok(()));

        let global_state = GlobalState::new(mock);
        global_state
            .create_setting(CreateSettingRequest {
                video_path,
                daq_path,
                ..Default::default()
            })
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_full_real() {
        util::log::init();

        let video_path = PathBuf::from(VIDEO_PATH);
        let daq_path = PathBuf::from(DAQ_PATH);

        let video_metadata = VideoMetadata {
            path: video_path,
            frame_rate: 25,
            nframes: 2444,
            shape: (1024, 1280),
        };
        let daq_metadata = DaqMetadata {
            path: daq_path,
            nrows: 2589,
            ncols: 10,
        };

        let video_path = video_metadata.path.clone();
        let daq_path = daq_metadata.path.clone();

        let mut mock = MockSettingStorage::new();
        mock.expect_create_setting().once().return_once(|_| Ok(()));
        mock.expect_set_video_metadata()
            .with(eq(video_metadata.clone()))
            .return_once(|_| Ok(()));
        mock.expect_video_metadata()
            .returning(move || Ok(video_metadata.clone()));
        mock.expect_set_daq_metadata()
            .with(eq(daq_metadata))
            .return_once(|_| Ok(()));

        let global_state = GlobalState::new(mock);
        global_state
            .create_setting(CreateSettingRequest {
                video_path,
                daq_path,
                ..Default::default()
            })
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
