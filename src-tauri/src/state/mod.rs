mod eval_task;
mod execute_task;
mod handle_output;
mod handler;
mod task;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use rusqlite::Connection;
use tlc_video::{FilterMethod, GmaxId, Green2Id, VideoController, VideoData, VideoId};

use crate::{
    daq::{DaqData, DaqId, InterpId, InterpMethod, Interpolator, Thermocouple},
    setting::{Setting, SettingSnapshot, StartIndex},
    solve::{IterationMethod, NuData, SolveController, SolveId},
};
pub use handler::{NuView, SettingData};
use task::TaskRegistry;

#[derive(Clone)]
pub struct GlobalState {
    inner: Arc<Mutex<GlobalStateInner>>,
}

impl GlobalState {
    pub fn new(db: Connection) -> GlobalState {
        GlobalState {
            inner: Arc::new(Mutex::new(GlobalStateInner::new(db))),
        }
    }
}

struct GlobalStateInner {
    setting: Setting,
    db: Connection,

    task_registry: TaskRegistry,

    video_data: Option<VideoData>,
    video_controller: VideoController,

    daq_data: Option<DaqData>,

    nu_data: Option<NuData>,
    solve_controller: SolveController,
}

impl GlobalStateInner {
    fn new(db: Connection) -> Self {
        Self {
            setting: Setting::default(),
            db,
            task_registry: TaskRegistry::default(),
            video_data: None,
            daq_data: None,
            video_controller: VideoController::default(),
            nu_data: None,
            solve_controller: SolveController::default(),
        }
    }

    fn create_setting(&mut self, setting_data: SettingData) -> Result<()> {
        self.setting.create_setting(&self.db, setting_data.into())
    }

    fn switch_setting(&mut self, setting_id: i64) -> Result<()> {
        self.setting.switch_setting(&self.db, setting_id)
    }

    fn delete_setting(&mut self, setting_id: i64) -> Result<()> {
        self.setting.delete_setting(&self.db, setting_id)
    }

    fn set_video_path(&mut self, video_path: &Path) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_video_path(&tx, video_path)?;
        self.setting.set_area(&tx, None)?;
        self.setting.set_start_index(&tx, None)?;
        self.setting.set_thermocouples(&tx, None)?;
        tx.commit()?;

        self.video_data = None;
        if let Some(daq_data) = self.daq_data.as_mut() {
            daq_data.set_interpolator(None);
        }
        self.nu_data = None;

        Ok(())
    }

    fn synchronize_video_and_daq(&mut self, start_frame: usize, start_row: usize) -> Result<()> {
        let nframes = self.video_data()?.video_meta().nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.daq_data()?.daq_meta().nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.setting.set_start_index(
            &self.db,
            Some(StartIndex {
                start_frame,
                start_row,
            }),
        )?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.daq_data
            .as_mut()
            .unwrap() // already checked above
            .set_interpolator(None);
        self.nu_data = None;

        Ok(())
    }

    fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        let nframes = self.video_data()?.video_meta().nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.daq_data()?.daq_meta().nrows;
        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.start_index()?;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }
        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.setting.set_start_index(
            &self.db,
            Some(StartIndex {
                start_frame,
                start_row,
            }),
        )?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.daq_data
            .as_mut()
            .unwrap() // already checked above
            .set_interpolator(None);
        self.nu_data = None;

        Ok(())
    }

    fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        let nrows = self.daq_data()?.daq_meta().nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }
        let nframes = self.video_data()?.video_meta().nframes;
        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.start_index()?;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_row");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({start_frame}) out of range({nframes})");
        }

        self.setting.set_start_index(
            &self.db,
            Some(StartIndex {
                start_frame,
                start_row,
            }),
        )?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        self.daq_data
            .as_mut()
            .unwrap() // already checked above
            .set_interpolator(None);
        self.nu_data = None;

        Ok(())
    }

    fn set_area(&mut self, area: (u32, u32, u32, u32)) -> Result<()> {
        let (h, w) = self.video_data()?.video_meta().shape;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_x + cal_w > w {
            bail!("area X out of range: top_left_x({tl_x}) + width({cal_w}) > video_width({w})");
        }
        if tl_y + cal_h > h {
            bail!("area Y out of range: top_left_y({tl_y}) + height({cal_h}) > video_height({h})");
        }

        self.setting.set_area(&self.db, Some(area))?;
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(None)
            .set_gmax_frame_indexes(None);
        if let Some(daq_data) = self.daq_data.as_mut() {
            daq_data.set_interpolator(None);
        }
        self.nu_data = None;

        Ok(())
    }

    fn set_filter_method(&mut self, filter_method: FilterMethod) -> Result<()> {
        self.setting.set_filter_method(&self.db, filter_method)?;
        if let Some(video_data) = self.video_data.as_mut() {
            video_data.set_gmax_frame_indexes(None);
        }
        self.nu_data = None;

        Ok(())
    }

    fn set_thermocouples(&mut self, thermocouples: &[Thermocouple]) -> Result<()> {
        if thermocouples.len() == 1 {
            bail!("there must be at least two thermocouples");
        }

        let tx = self.db.transaction()?;
        self.setting.set_thermocouples(&tx, Some(thermocouples))?;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    fn set_interp_method(&mut self, interp_method: InterpMethod) -> Result<()> {
        let tx = self.db.transaction()?;
        self.setting.set_interp_method(&tx, interp_method)?;
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(None);
        self.nu_data = None;
        tx.commit()?;

        Ok(())
    }

    fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> Result<()> {
        self.setting
            .set_iteration_method(&self.db, iteration_method)?;
        self.nu_data = None;

        Ok(())
    }

    fn set_gmax_temperature(&mut self, gmax_temperature: f64) -> Result<()> {
        self.setting
            .set_gmax_temperature(&self.db, gmax_temperature)?;
        self.nu_data = None;

        Ok(())
    }

    fn set_solid_thermal_conductivity(&mut self, solid_thermal_conductivity: f64) -> Result<()> {
        self.setting
            .set_solid_thermal_conductivity(&self.db, solid_thermal_conductivity)?;
        self.nu_data = None;

        Ok(())
    }

    fn set_solid_thermal_diffusivity(&mut self, solid_thermal_diffusivity: f64) -> Result<()> {
        self.setting
            .set_solid_thermal_diffusivity(&self.db, solid_thermal_diffusivity)?;
        self.nu_data = None;

        Ok(())
    }

    fn set_characteristic_length(&mut self, characteristic_length: f64) -> Result<()> {
        self.setting
            .set_characteristic_length(&self.db, characteristic_length)?;
        self.nu_data = None;

        Ok(())
    }

    fn set_air_thermal_conductivity(&mut self, air_thermal_conductivity: f64) -> Result<()> {
        self.setting
            .set_air_thermal_conductivity(&self.db, air_thermal_conductivity)?;
        self.nu_data = None;

        Ok(())
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
    use std::{thread::sleep, time::Duration};

    use tlc_util::progress_bar::Progress;
    use tlc_video::{FilterMethod, VideoMeta};

    use crate::{
        daq::DaqMeta,
        setting::new_db_in_memory,
        solve::{IterationMethod, PhysicalParam},
    };

    use super::*;

    pub const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";
    pub const DAQ_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/daq/imp_20000_1.lvm";

    #[ignore]
    #[test]
    fn test_real() {
        tlc_util::log::init();
        let global_state = GlobalState::new(new_db_in_memory());

        global_state.create_setting(SettingData::default()).unwrap();
        assert_eq!(global_state.get_name().unwrap(), "test_case");
        assert_eq!(
            global_state.get_save_root_dir().unwrap(),
            PathBuf::from("/tmp")
        );

        assert_eq!(
            global_state.get_read_video_progress(),
            Progress::Uninitialized
        );
        global_state
            .set_video_path(PathBuf::from(VIDEO_PATH_REAL))
            .unwrap();
        assert_eq!(
            global_state.get_video_path().unwrap(),
            PathBuf::from(VIDEO_PATH_REAL)
        );
        global_state
            .set_daq_path(PathBuf::from(DAQ_PATH_REAL))
            .unwrap();
        assert_eq!(
            global_state.get_daq_path().unwrap(),
            PathBuf::from(DAQ_PATH_REAL)
        );

        loop {
            let progress = global_state.get_read_video_progress();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(200))
                }
                Progress::Finished { .. } => break,
            }
        }

        let video_meta = global_state.get_video_meta().unwrap();
        assert_eq!(
            video_meta,
            VideoMeta {
                frame_rate: 25,
                nframes: 2444,
                shape: (1024, 1280),
            }
        );
        assert_eq!(
            global_state.get_daq_meta().unwrap(),
            DaqMeta {
                nrows: 2589,
                ncols: 10,
            }
        );
        global_state
            .decode_frame_base64(video_meta.nframes)
            .unwrap_err();
        global_state
            .decode_frame_base64(video_meta.nframes - 1)
            .unwrap();
        global_state.set_area((660, 20, 340, 1248)).unwrap();
        assert_eq!(
            global_state.get_build_green2_progress(),
            Progress::Uninitialized
        );
        global_state.synchronize_video_and_daq(71, 140).unwrap();
        sleep(Duration::from_millis(200));
        global_state.get_build_green2_progress();

        // Will cancel the current computation.
        global_state.set_start_frame(81).unwrap();
        // Same parameters, evaluated task will be rejected by task registry.
        global_state.set_start_row(150).unwrap();

        loop {
            let progress = global_state.get_build_green2_progress();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(500))
                }
                Progress::Finished { .. } => break,
            }
        }

        global_state
            .set_filter_method(FilterMethod::Median { window_size: 10 })
            .unwrap();

        loop {
            let progress = global_state.get_detect_peak_progress();
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

        global_state
            .set_thermocouples(thermocouples.clone())
            .unwrap();
        assert_eq!(global_state.get_thermocouples().unwrap(), thermocouples);
        assert_eq!(global_state.get_solve_progress(), Progress::Uninitialized);
        global_state
            .set_interp_method(InterpMethod::Horizontal)
            .unwrap();
        assert_eq!(
            global_state.get_interp_method().unwrap(),
            InterpMethod::Horizontal
        );

        loop {
            let progress = global_state.get_solve_progress();
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
        } = global_state.get_nu(None).unwrap();
        dbg!(nu_nan_mean, edge_truncation);
    }

    #[ignore]
    #[test]
    fn test_complete_setting_auto_compute_all() {
        tlc_util::log::init();
        let global_state = GlobalState::new(new_db_in_memory());

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

        global_state.create_setting(setting_data).unwrap();

        loop {
            let progress = global_state.get_solve_progress();
            match progress {
                Progress::Uninitialized | Progress::InProgress { .. } => {
                    sleep(Duration::from_millis(1000))
                }
                Progress::Finished { .. } => break,
            }
        }
    }
}
