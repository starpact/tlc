use std::{
    fmt::Debug,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{anyhow, bail, Result};
use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use video::{FilterMethod, Progress};

use crate::{
    daq::InterpMethod,
    post_processing::draw_area,
    setting::{self, SettingStorage, StartIndex},
    solve::{IterationMethod, PhysicalParam},
};

pub struct GlobalState<S: SettingStorage> {
    setting_storage: Arc<Mutex<S>>,
    // video_manager: VideoManager<S>,
    nu_data: Arc<Mutex<Option<NuData>>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct NuData {
    nu2: ArcArray2<f64>,
    nu_nan_mean: f64,
    nu_plot_base64: String,
    edge_truncation: (f64, f64),
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
    pub fn new(setting_storage: S) -> GlobalState<S> {
        let setting_storage = Arc::new(Mutex::new(setting_storage));
        // let video_manager = VideoManager::new(setting_storage.clone());

        GlobalState {
            setting_storage,
            // video_manager,
            nu_data: Default::default(),
        }
    }

    pub async fn create_setting(&self, request: CreateSettingRequest) -> Result<()> {
        let video_path = request.video_path;
        let create_request = setting::CreateRequest {
            name: request.name,
            save_root_dir: request.save_root_dir,
            filter_method: request.filter_method,
            iteration_method: request.iteration_method,
            physical_param: request.physical_param,
        };

        self.asyncify(|mut s| s.create_setting(create_request))
            .await?;

        // if let Err(e) = self.video_manager.spawn_read_video(Some(video_path)).await {
        //     self.asyncify(|mut s| s.delete_setting()).await?;
        //     return Err(e);
        // }

        Ok(())
    }

    pub async fn switch_setting(&self, setting_id: i64) -> Result<()> {
        self.asyncify(move |mut s| s.switch_setting(setting_id))
            .await?;

        // self.video_manager.spawn_read_video(None).await?;

        Ok(())
    }

    pub async fn get_save_root_dir(&self) -> Result<PathBuf> {
        self.asyncify(move |s| s.save_root_dir()).await
    }

    pub async fn set_save_root_dir(&self, save_root_dir: PathBuf) -> Result<()> {
        if !save_root_dir.is_dir() {
            bail!("save_root_dir is not a valid directory: {save_root_dir:?}");
        }
        self.asyncify(move |s| s.set_save_root_dir(&save_root_dir))
            .await
    }

    // pub async fn read_single_frame_base64(&self, frame_index: usize) -> Result<String> {
    //     self.video_manager
    //         .read_single_frame_base64(frame_index)
    //         .await
    // }

    // pub async fn get_daq_raw(&self) -> Result<ArcArray2<f64>> {
    //     self.daq_manager
    //         .raw()
    //         .ok_or_else(|| anyhow!("daq path unset"))
    // }

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

    // pub async fn set_start_frame(&self, start_frame: usize) -> Result<()> {
    //     self.asyncify(move |s| s.set_start_frame(start_frame))
    //         .await?;
    //     self.video_manager.spawn_build_green2().await
    // }

    // pub async fn set_start_row(&self, start_row: usize) -> Result<()> {
    //     self.asyncify(move |s| s.set_start_row(start_row)).await?;
    //     self.video_manager.spawn_build_green2().await
    // }

    pub async fn get_area(&self) -> Result<(usize, usize, usize, usize)> {
        self.asyncify(|s| s.area()).await
    }

    // pub async fn set_area(&self, area: (usize, usize, usize, usize)) -> Result<()> {
    //     self.asyncify(move |s| s.set_area(area)).await?;
    //     self.video_manager.spawn_build_green2().await
    // }

    // pub async fn spawn_build_green2(&self) -> Result<()> {
    //     self.video_manager.spawn_build_green2().await
    // }

    // pub fn get_build_green2_progress(&self) -> Progress {
    //     self.video_manager.build_green2_progress()
    // }

    pub async fn get_filter_method(&self) -> Result<FilterMethod> {
        self.asyncify(|s| Ok(s.filter_meta()?.filter_method)).await
    }

    // pub async fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
    //     self.asyncify(move |s| s.set_filter_method(filter_method))
    //         .await?;
    //     self.video_manager.spawn_detect_peak().await
    // }

    // pub async fn filter_single_point(&self, position: (usize, usize)) -> Result<Vec<u8>> {
    //     self.video_manager.filter_single_point(position).await
    // }

    // pub async fn spawn_detect_peak(&self) -> Result<()> {
    //     self.video_manager.spawn_detect_peak().await
    // }

    // pub fn get_detect_peak_progress(&self) -> Progress {
    //     self.video_manager.detect_peak_progress_bar()
    // }

    pub async fn get_interpolation_method(&self) -> Result<InterpMethod> {
        self.asyncify(|s| s.interp_method()).await
    }

    // pub async fn set_interpolation_method(
    //     &self,
    //     interpolation_method: InterpolationMethod,
    // ) -> Result<()> {
    //     self.asyncify(move |s| s.set_interpolation_method(interpolation_method))
    //         .await?;
    //     self.daq_manager.interpolate().await?;
    //
    //     Ok(())
    // }

    // pub async fn interpolate_single_frame(&self, frame_index: usize) -> Result<Array2<f64>> {
    //     let daq_manager = self.daq_manager.clone();
    //     spawn_blocking(move || {
    //         daq_manager
    //             .interpolator()
    //             .ok_or_else(|| anyhow!("interpolator not interpolated yet"))?
    //             .interpolate_single_frame(frame_index)
    //     })
    //     .await?
    // }

    // pub async fn interpolate(&self) -> Result<()> {
    //     self.daq_manager.interpolate().await
    // }

    pub async fn get_iteration_method(&self) -> Result<IterationMethod> {
        self.asyncify(|s| s.iteration_method()).await
    }

    pub async fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        self.asyncify(move |s| s.set_iteration_method(iteration_method))
            .await
    }

    pub async fn set_gmax_temperature(&self, gmax_temperature: f64) -> Result<()> {
        self.asyncify(move |s| s.set_gmax_temperature(gmax_temperature))
            .await
    }

    pub async fn set_solid_thermal_conductivity(
        &self,
        solid_thermal_conductivity: f64,
    ) -> Result<()> {
        self.asyncify(move |s| s.set_solid_thermal_conductivity(solid_thermal_conductivity))
            .await
    }

    pub async fn set_solid_thermal_diffusivity(
        &self,
        solid_thermal_diffusivity: f64,
    ) -> Result<()> {
        self.asyncify(move |s| s.set_solid_thermal_diffusivity(solid_thermal_diffusivity))
            .await
    }

    pub async fn set_characteristic_length(&self, characteristic_length: f64) -> Result<()> {
        self.asyncify(move |s| s.set_characteristic_length(characteristic_length))
            .await
    }

    pub async fn set_air_thermal_conductivity(&self, air_thermal_conductivity: f64) -> Result<()> {
        self.asyncify(move |s| s.set_air_thermal_conductivity(air_thermal_conductivity))
            .await
    }

    // pub async fn solve(&self) -> Result<()> {
    //     let gmax_frame_indexes = self
    //         .video_manager
    //         .gmax_frame_indexes()
    //         .ok_or_else(|| anyhow!("gmax_frame_indexes not built yet"))?;
    //     let interpolator = self
    //         .daq_manager
    //         .interpolator()
    //         .ok_or_else(|| anyhow!("interpolator not built yet"))?;
    //
    //     let setting_storage = self.setting_storage.clone();
    //     let nu_data = self.nu_data.clone();
    //     spawn_blocking(move || -> Result<()> {
    //         let setting_storage = setting_storage.lock().unwrap();
    //         let physical_param = setting_storage.physical_param()?;
    //         let frame_rate = setting_storage.video_metadata()?.frame_rate;
    //         let iteration_method = setting_storage.iteration_method()?;
    //
    //         let nu2 = solve::solve(
    //             gmax_frame_indexes,
    //             interpolator,
    //             physical_param,
    //             iteration_method,
    //             frame_rate,
    //         );
    //
    //         let nu_nan_mean = nan_mean(nu2.view());
    //         debug!(nu_nan_mean);
    //
    //         let nu_path = setting_storage.nu_path()?;
    //         if nu_path.exists() {
    //             warn!("nu_path({nu_path:?}) already exists, overwrite")
    //         }
    //         save_matrix(nu_path, nu2.view())?;
    //
    //         let plot_path = setting_storage.plot_path()?;
    //         if plot_path.exists() {
    //             warn!("plot_path({plot_path:?}) already exists, overwrite")
    //         }
    //         let edge_truncation = default_edge_truncation_from_mean(nu_nan_mean);
    //         let nu_plot_base64 = draw_area(plot_path, nu2.view(), edge_truncation)?;
    //
    //         *nu_data.lock().unwrap() = Some(NuData {
    //             nu2: nu2.into_shared(),
    //             nu_nan_mean,
    //             nu_plot_base64,
    //             edge_truncation,
    //         });
    //
    //         Ok(())
    //     })
    //     .await??;
    //
    //     Ok(())
    // }

    pub async fn get_nu(&self, edge_truncation: Option<(f64, f64)>) -> Result<NuData> {
        let setting_storage = self.setting_storage.clone();
        let nu_data = self.nu_data.clone();
        spawn_blocking(move || -> Result<NuData> {
            let mut nu_data = nu_data.lock().unwrap();
            let mut nu_data = nu_data
                .as_mut()
                .ok_or_else(|| anyhow!("nu not calculated yet"))?;

            let edge_truncation = edge_truncation
                .unwrap_or_else(|| default_edge_truncation_from_mean(nu_data.nu_nan_mean));
            if edge_truncation == nu_data.edge_truncation {
                return Ok(nu_data.clone());
            }

            let setting_storage = setting_storage.lock().unwrap();
            let plot_path = setting_storage.plot_path()?;
            let nu_plot_base64 = draw_area(plot_path, nu_data.nu2.view(), edge_truncation)?;

            nu_data.edge_truncation = edge_truncation;
            nu_data.nu_plot_base64 = nu_plot_base64;

            Ok(nu_data.clone())
        })
        .await?
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

fn default_edge_truncation_from_mean(mean: f64) -> (f64, f64) {
    (mean * 0.6, mean * 2.0)
}

// #[cfg(test)]
// mod tests {
//     use std::time::Duration;
//
//     use mockall::predicate::eq;
//
//     use super::*;
//     use crate::{
//         daq::Thermocouple,
//         setting::MockSettingStorage,
//         util,
//         video::{FilterMetadata, Green2Metadata},
//     };
//
//     // For unit tests.
//     const SAMPLE_VIDEO_PATH: &str = "./tests/almost_empty.avi";
//     // Too large, just for integration tests.
//     const VIDEO_PATH: &str =
//         "/home/yhj/Downloads/2021_YanHongjie/EXP/imp/videos/imp_20000_1_up.avi";
//     // For both unit and integration tests.
//     const DAQ_PATH: &str = "./tests/imp_20000_1.lvm";
//
//     #[tokio::test]
//     async fn test_create_setting_video_not_found() {
//         util::log::init();
//
//         let mut mock = MockSettingStorage::new();
//         mock.expect_create_setting().once().return_once(|_| Ok(()));
//         mock.expect_delete_setting().once().return_once(|| Ok(()));
//
//         let global_state = GlobalState::new(mock);
//         global_state
//             .create_setting(CreateSettingRequest {
//                 video_path: PathBuf::from("not_found.avi"),
//                 ..Default::default()
//             })
//             .await
//             .unwrap_err();
//     }
//
//     #[tokio::test]
//     async fn test_create_setting_daq_not_found() {
//         util::log::init();
//
//         let video_metadata = VideoMetadata {
//             path: PathBuf::from(SAMPLE_VIDEO_PATH),
//             frame_rate: 25,
//             nframes: 3,
//             shape: (1024, 1280),
//         };
//
//         let mut mock = MockSettingStorage::new();
//         mock.expect_create_setting().once().return_once(|_| Ok(()));
//         mock.expect_set_video_metadata()
//             .with(eq(video_metadata.clone()))
//             .return_once(|_| Ok(()));
//         mock.expect_video_metadata()
//             .returning(move || Ok(video_metadata.clone()));
//         mock.expect_delete_setting().once().return_once(|| Ok(()));
//
//         let global_state = GlobalState::new(mock);
//         global_state
//             .create_setting(CreateSettingRequest {
//                 video_path: PathBuf::from(SAMPLE_VIDEO_PATH),
//                 daq_path: PathBuf::from("not_found.lvm"),
//                 ..Default::default()
//             })
//             .await
//             .unwrap_err();
//     }
//
//     #[tokio::test]
//     async fn test_create_setting_ok() {
//         util::log::init();
//
//         let video_path = PathBuf::from(SAMPLE_VIDEO_PATH);
//         let daq_path = PathBuf::from(DAQ_PATH);
//
//         let video_metadata = VideoMetadata {
//             path: video_path.clone(),
//             frame_rate: 25,
//             nframes: 3,
//             shape: (1024, 1280),
//         };
//         let daq_metadata = DaqMetadata {
//             path: daq_path.clone(),
//             nrows: 2589,
//             ncols: 10,
//         };
//
//         let mut mock = MockSettingStorage::new();
//         mock.expect_create_setting().once().return_once(|_| Ok(()));
//         mock.expect_set_video_metadata()
//             .with(eq(video_metadata.clone()))
//             .return_once(|_| Ok(()));
//         mock.expect_video_metadata()
//             .returning(move || Ok(video_metadata.clone()));
//         mock.expect_set_daq_metadata()
//             .with(eq(daq_metadata))
//             .return_once(|_| Ok(()));
//
//         let global_state = GlobalState::new(mock);
//         global_state
//             .create_setting(CreateSettingRequest {
//                 video_path,
//                 daq_path,
//                 ..Default::default()
//             })
//             .await
//             .unwrap();
//
//         tokio::time::sleep(Duration::from_millis(50)).await;
//     }
//
//     #[tokio::test]
//     #[ignore]
//     async fn test_full_real_data_mock_db() {
//         util::log::init();
//
//         let save_root_dir = PathBuf::from("./var");
//         let name = "aaa";
//         let plot_path = save_root_dir.join(name).with_extension("png");
//         let nu_path = save_root_dir.join(name).with_extension("csv");
//         let video_path = PathBuf::from(VIDEO_PATH);
//         let daq_path = PathBuf::from(DAQ_PATH);
//         let nframes = 2444;
//         let cal_num = 2000;
//
//         let video_metadata = VideoMetadata {
//             path: video_path.clone(),
//             frame_rate: 25,
//             nframes,
//             shape: (1024, 1280),
//         };
//         let daq_metadata = DaqMetadata {
//             path: daq_path.clone(),
//             nrows: 2589,
//             ncols: 10,
//         };
//         let green2_metadata = Green2Metadata {
//             start_frame: 1,
//             cal_num,
//             area: (660, 20, 340, 1248),
//             video_path: video_path.clone(),
//         };
//         let start_index = StartIndex {
//             start_frame: 81,
//             start_row: 150,
//         };
//         let filter_method = FilterMethod::No;
//         let interpolation_method = InterpolationMethod::Horizontal;
//         let thermocouples = vec![
//             Thermocouple {
//                 column_index: 1,
//                 position: (0, 166),
//             },
//             Thermocouple {
//                 column_index: 2,
//                 position: (0, 355),
//             },
//             Thermocouple {
//                 column_index: 3,
//                 position: (0, 543),
//             },
//             Thermocouple {
//                 column_index: 4,
//                 position: (0, 731),
//             },
//             Thermocouple {
//                 column_index: 5,
//                 position: (0, 922),
//             },
//             Thermocouple {
//                 column_index: 6,
//                 position: (0, 1116),
//             },
//         ];
//         let physical_param = PhysicalParam {
//             gmax_temperature: 35.48,
//             solid_thermal_conductivity: 0.19,
//             solid_thermal_diffusivity: 1.091e-7,
//             characteristic_length: 0.015,
//             air_thermal_conductivity: 0.0276,
//         };
//         let iteration_method = IterationMethod::NewtonTangent {
//             h0: 50.0,
//             max_iter_num: 10,
//         };
//
//         let mut mock = MockSettingStorage::new();
//         mock.expect_plot_path()
//             .returning(move || Ok(plot_path.clone()));
//         mock.expect_nu_path().return_once(|| Ok(nu_path));
//         mock.expect_create_setting().once().return_once(|_| Ok(()));
//         mock.expect_set_video_metadata()
//             .with(eq(video_metadata.clone()))
//             .return_once(|_| Ok(()));
//         mock.expect_video_metadata()
//             .returning(move || Ok(video_metadata.clone()));
//         mock.expect_set_daq_metadata()
//             .with(eq(daq_metadata))
//             .return_once(|_| Ok(()));
//         {
//             let green2_metadata = green2_metadata.clone();
//             mock.expect_green2_metadata()
//                 .returning(move || Ok(green2_metadata.clone()));
//         }
//         mock.expect_filter_metadata().returning(move || {
//             Ok(FilterMetadata {
//                 filter_method,
//                 green2_metadata: green2_metadata.clone(),
//             })
//         });
//         mock.expect_interpolation_method()
//             .return_once(move || Ok(interpolation_method));
//         mock.expect_start_index()
//             .return_once(move || Ok(start_index));
//         mock.expect_thermocouples()
//             .return_once(|| Ok(thermocouples));
//         mock.expect_physical_param()
//             .return_once(move || Ok(physical_param));
//         mock.expect_iteration_method()
//             .return_once(move || Ok(iteration_method));
//
//         let global_state = GlobalState::new(mock);
//         global_state
//             .create_setting(CreateSettingRequest {
//                 video_path,
//                 daq_path,
//                 ..Default::default()
//             })
//             .await
//             .unwrap();
//
//         global_state
//             .read_single_frame_base64(nframes - 1)
//             .await
//             .unwrap();
//
//         global_state.spawn_build_green2().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         loop {
//             match global_state.video_manager.build_green2_progress() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("building green2...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//
//         global_state.spawn_detect_peak().await.unwrap();
//         tokio::time::sleep(Duration::from_millis(100)).await;
//         loop {
//             match global_state.video_manager.detect_peak_progress_bar() {
//                 Progress::Uninitialized => {}
//                 Progress::InProgress { total, count } => {
//                     println!("detecting peaks...... {count}/{total}");
//                 }
//                 Progress::Finished { .. } => break,
//             }
//             tokio::time::sleep(Duration::from_millis(500)).await;
//         }
//
//         global_state.daq_manager.interpolate().await.unwrap();
//         global_state.solve().await.unwrap();
//         let nu_data = global_state.get_nu(None).await.unwrap();
//         dbg!(nu_data.edge_truncation);
//         let nu_data = global_state.get_nu(Some((40.0, 300.0))).await.unwrap();
//         dbg!(nu_data.edge_truncation);
//     }
// }
