mod main_loop;
mod output_handler;
mod request_handler;
mod task;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use crossbeam::channel::{bounded, Receiver, Sender};
use rusqlite::Connection;
use tlc_video::{GmaxId, Green2Id, VideoController, VideoData, VideoId};

use crate::{
    daq::{DaqData, DaqId, InterpId, InterpMethod, Interpolator, Thermocouple},
    setting::{Setting, SettingSnapshot, StartIndex},
    solve::{NuData, SolveController, SolveId},
};
pub use main_loop::main_loop;
pub use task::Output;

use self::task::TaskRegistry;

pub struct GlobalState {
    setting: Setting,
    db: Connection,

    output_sender: Sender<Output>,
    output_receiver: Receiver<Output>,

    task_registry: TaskRegistry,

    video_data: Option<VideoData>,
    video_controller: VideoController,

    daq_data: Option<DaqData>,

    nu_data: Option<NuData>,
    solve_controller: SolveController,
}

impl GlobalState {
    pub fn new(db: Connection) -> GlobalState {
        let (output_sender, output_receiver) = bounded(0);
        GlobalState {
            setting: Setting::default(),
            db,
            output_sender,
            output_receiver,
            task_registry: TaskRegistry::default(),
            video_data: None,
            daq_data: None,
            video_controller: VideoController::default(),
            nu_data: None,
            solve_controller: SolveController::default(),
        }
    }

    fn video_data(&self) -> Result<&VideoData> {
        self.video_data
            .as_ref()
            .ok_or_else(|| anyhow!("video not loaded yet"))
    }

    fn daq_data(&self) -> Result<&DaqData> {
        self.daq_data
            .as_ref()
            .ok_or_else(|| anyhow!("daq not loaded yet"))
    }

    fn video_id(&self) -> Result<VideoId> {
        Ok(VideoId {
            video_path: self.setting.video_path(&self.db)?,
        })
    }

    fn daq_id(&self) -> Result<DaqId> {
        Ok(DaqId {
            daq_path: self.setting.daq_path(&self.db)?,
        })
    }

    fn green2_id(&self) -> Result<Green2Id> {
        let video_id = self.video_id()?;
        let nframes = self.video_data()?.video_meta().nframes;
        let nrows = self.daq_data()?.daq_meta().nrows;
        let StartIndex {
            start_frame,
            start_row,
        } = self.start_index()?;
        let cal_num = (nframes - start_frame).min(nrows - start_row);
        let area = self.area()?;

        Ok(Green2Id {
            video_id,
            start_frame,
            cal_num,
            area,
        })
    }

    fn gmax_id(&self) -> Result<GmaxId> {
        let green2_id = self.green2_id()?;
        let filter_method = self.setting.filter_method(&self.db)?;

        Ok(GmaxId {
            green2_id,
            filter_method,
        })
    }

    fn interp_id(&self) -> Result<InterpId> {
        let daq_path = self.setting.daq_path(&self.db)?;
        let start_row = self.start_index()?.start_row;
        let Green2Id { cal_num, area, .. } = self.green2_id()?;
        let thermocouples = self.thermocouples()?;
        let interp_method = self.interp_method()?;

        Ok(InterpId {
            daq_id: DaqId { daq_path },
            start_row,
            cal_num,
            area,
            interp_method,
            thermocouples,
        })
    }

    fn solve_id(&self) -> Result<SolveId> {
        Ok(SolveId {
            gmax_id: self.gmax_id()?,
            interp_id: self.interp_id()?,
            frame_rate: self.video_data()?.video_meta().frame_rate,
            iteration_method: self.setting.iteration_method(&self.db)?,
            physical_param: self.setting.physical_param(&self.db)?,
        })
    }

    fn start_index(&self) -> Result<StartIndex> {
        self.setting
            .start_index(&self.db)?
            .ok_or_else(|| anyhow!("video and daq not synchronized yet"))
    }

    fn area(&self) -> Result<(u32, u32, u32, u32)> {
        self.setting
            .area(&self.db)?
            .ok_or_else(|| anyhow!("area not selected yet"))
    }

    fn interp_method(&self) -> Result<InterpMethod> {
        self.setting
            .interp_method(&self.db)?
            .ok_or_else(|| anyhow!("interp method unset"))
    }

    fn thermocouples(&self) -> Result<Vec<Thermocouple>> {
        self.setting
            .thermocouples(&self.db)?
            .ok_or_else(|| anyhow!("thermocouples unset"))
    }

    fn interpolator(&self) -> Result<Interpolator> {
        Ok(self
            .daq_data()?
            .interpolator()
            .ok_or_else(|| anyhow!("not interpolated yet"))?
            .clone())
    }

    fn setting_snapshot(&self, nu_nan_mean: f64) -> Result<SettingSnapshot> {
        let StartIndex {
            start_frame,
            start_row,
        } = self.start_index()?;
        let setting_snapshot = SettingSnapshot {
            name: self.setting.name(&self.db)?,
            save_root_dir: self.setting.save_root_dir(&self.db)?,
            video_path: self.setting.video_path(&self.db)?,
            video_meta: self.video_data()?.video_meta(),
            daq_path: self.setting.daq_path(&self.db)?,
            daq_meta: self.daq_data()?.daq_meta(),
            start_frame,
            start_row,
            area: self.area()?,
            thermocouples: self.thermocouples()?,
            filter_method: self.setting.filter_method(&self.db)?,
            interp_method: self.interp_method()?,
            iteration_method: self.setting.iteration_method(&self.db)?,
            physical_param: self.setting.physical_param(&self.db)?,
            nu_nan_mean,
            completed_at: time::OffsetDateTime::now_local()?,
        };

        Ok(setting_snapshot)
    }

    fn output_file_stem(&self) -> Result<PathBuf> {
        let save_root_dir = self.setting.save_root_dir(&self.db)?;
        let name = self.setting.name(&self.db)?;
        Ok(save_root_dir.join(name))
    }

    fn nu_path(&self) -> Result<PathBuf> {
        Ok(self.output_file_stem()?.with_extension("csv"))
    }

    fn plot_path(&self) -> Result<PathBuf> {
        Ok(self.output_file_stem()?.with_extension("png"))
    }

    fn setting_snapshot_path(&self) -> Result<PathBuf> {
        Ok(self.output_file_stem()?.with_extension("json"))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        thread::{sleep, spawn},
        time::Duration,
    };

    use tlc_util::progress_bar::Progress;
    use tlc_video::{FilterMethod, VideoMeta};

    use crate::{
        daq::DaqMeta,
        main_loop,
        request::{self, NuView, SettingData},
        setting::new_db_in_memory,
        solve::{IterationMethod, PhysicalParam},
    };

    use super::*;

    pub const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";
    pub const DAQ_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/daq/imp_20000_1.lvm";

    #[ignore]
    #[tokio::test]
    async fn test_real() {
        tlc_util::log::init();
        let (tx, rx) = bounded(3);
        spawn(move || main_loop(new_db_in_memory(), rx));

        request::create_setting(SettingData::default(), &tx)
            .await
            .unwrap();
        assert_eq!(request::get_name(&tx).await.unwrap(), "test_case");
        assert_eq!(
            request::get_save_root_dir(&tx).await.unwrap(),
            PathBuf::from("/tmp")
        );

        assert_eq!(
            request::get_read_video_progress(&tx).await.unwrap(),
            Progress::Uninitialized
        );
        request::set_video_path(PathBuf::from(VIDEO_PATH_REAL), &tx)
            .await
            .unwrap();
        assert_eq!(
            request::get_video_path(&tx).await.unwrap(),
            PathBuf::from(VIDEO_PATH_REAL)
        );
        request::set_daq_path(PathBuf::from(DAQ_PATH_REAL), &tx)
            .await
            .unwrap();
        assert_eq!(
            request::get_daq_path(&tx).await.unwrap(),
            PathBuf::from(DAQ_PATH_REAL)
        );

        loop {
            let progress = request::get_read_video_progress(&tx).await.unwrap();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(200))
                }
                Progress::Finished { .. } => break,
            }
        }

        let video_meta = request::get_video_meta(&tx).await.unwrap();
        assert_eq!(
            video_meta,
            VideoMeta {
                frame_rate: 25,
                nframes: 2444,
                shape: (1024, 1280),
            }
        );
        assert_eq!(
            request::get_daq_meta(&tx).await.unwrap(),
            DaqMeta {
                nrows: 2589,
                ncols: 10,
            }
        );
        request::decode_frame_base64(video_meta.nframes, &tx)
            .await
            .unwrap_err();
        request::decode_frame_base64(video_meta.nframes - 1, &tx)
            .await
            .unwrap();
        request::set_area((660, 20, 340, 1248), &tx).await.unwrap();
        assert_eq!(
            request::get_build_green2_progress(&tx).await.unwrap(),
            Progress::Uninitialized
        );
        request::synchronize_video_and_daq(71, 140, &tx)
            .await
            .unwrap();
        sleep(Duration::from_millis(200));
        request::get_build_green2_progress(&tx).await.unwrap();

        // Will cancel the current computation.
        request::set_start_frame(81, &tx).await.unwrap();
        // Same parameters, evaluated task will be rejected by task registry.
        request::set_start_row(150, &tx).await.unwrap();

        loop {
            let progress = request::get_build_green2_progress(&tx).await.unwrap();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(500))
                }
                Progress::Finished { .. } => break,
            }
        }

        request::set_filter_method(FilterMethod::Median { window_size: 10 }, &tx)
            .await
            .unwrap();

        loop {
            let progress = request::get_detect_peak_progress(&tx).await.unwrap();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(500))
                }
                Progress::Finished { .. } => break,
            }
        }

        let thermocouples = vec![
            Thermocouple {
                column_index: 1,
                position: (0, 166),
            },
            Thermocouple {
                column_index: 2,
                position: (0, 355),
            },
            Thermocouple {
                column_index: 3,
                position: (0, 543),
            },
            Thermocouple {
                column_index: 4,
                position: (0, 731),
            },
            Thermocouple {
                column_index: 1,
                position: (0, 922),
            },
            Thermocouple {
                column_index: 6,
                position: (0, 1116),
            },
        ];

        request::set_thermocouples(thermocouples.clone(), &tx)
            .await
            .unwrap();
        assert_eq!(
            request::get_thermocouples(&tx).await.unwrap(),
            thermocouples
        );
        assert_eq!(
            request::get_solve_progress(&tx).await.unwrap(),
            Progress::Uninitialized
        );
        request::set_interp_method(InterpMethod::Horizontal, &tx)
            .await
            .unwrap();
        assert_eq!(
            request::get_interp_method(&tx).await.unwrap(),
            InterpMethod::Horizontal
        );

        loop {
            let progress = request::get_solve_progress(&tx).await.unwrap();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(200))
                }
                Progress::Finished { .. } => break,
            }
        }

        let NuView {
            nu_nan_mean,
            edge_truncation,
            ..
        } = request::get_nu(None, &tx).await.unwrap();
        dbg!(nu_nan_mean, edge_truncation);
    }

    #[ignore]
    #[tokio::test]
    async fn test_complete_setting_auto_compute_all() {
        tlc_util::log::init();
        let (tx, rx) = bounded(3);
        spawn(move || main_loop(new_db_in_memory(), rx));

        let setting_data = SettingData {
            name: "test_case".to_owned(),
            save_root_dir: PathBuf::from("/tmp"),
            video_path: Some(PathBuf::from(VIDEO_PATH_REAL)),
            daq_path: Some(PathBuf::from(DAQ_PATH_REAL)),
            start_frame: Some(81),
            start_row: Some(150),
            area: Some((660, 20, 340, 1248)),
            thermocouples: Some(vec![
                Thermocouple {
                    column_index: 1,
                    position: (0, 166),
                },
                Thermocouple {
                    column_index: 2,
                    position: (0, 355),
                },
                Thermocouple {
                    column_index: 3,
                    position: (0, 543),
                },
                Thermocouple {
                    column_index: 4,
                    position: (0, 731),
                },
                Thermocouple {
                    column_index: 1,
                    position: (0, 922),
                },
                Thermocouple {
                    column_index: 6,
                    position: (0, 1116),
                },
            ]),
            interp_method: Some(InterpMethod::Horizontal),
            filter_method: Some(FilterMethod::Median { window_size: 10 }),
            iteration_method: Some(IterationMethod::default()),
            physical_param: PhysicalParam::default(),
        };
        request::create_setting(setting_data, &tx).await.unwrap();

        loop {
            let progress = request::get_solve_progress(&tx).await.unwrap();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(200))
                }
                Progress::Finished { .. } => break,
            }
        }
    }
}
