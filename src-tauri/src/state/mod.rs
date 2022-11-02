mod main_loop;
mod outcome_handler;
mod request_handler;
mod task;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use crossbeam::channel::{bounded, Receiver, Sender};
pub use main_loop::main_loop;
use rusqlite::Connection;
use tlc_util::time::now_as_millis;
use tlc_video::{GmaxId, Green2Id, VideoController, VideoData, VideoId};

use crate::{
    daq::{DaqData, DaqId, InterpId, InterpMethod, Interpolator, Thermocouple},
    setting::{Setting, SettingSnapshot, StartIndex},
    solve::{NuData, SolveId},
};
use outcome_handler::Outcome;

use self::task::TaskController;

struct GlobalState {
    setting: Setting,
    db: Connection,

    outcome_sender: Sender<Outcome>,
    outcome_receiver: Receiver<Outcome>,

    task_controller: TaskController,

    video_data: Option<VideoData>,
    video_controller: VideoController,

    daq_data: Option<DaqData>,

    nu_data: Option<NuData>,
}

impl GlobalState {
    fn new(db: Connection) -> Self {
        let (outcome_sender, outcome_receiver) = bounded(0);
        Self {
            setting: Setting::default(),
            db,
            outcome_sender,
            outcome_receiver,
            task_controller: TaskController::default(),
            video_data: None,
            daq_data: None,
            video_controller: VideoController::default(),
            nu_data: None,
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
        let StartIndex {
            start_frame,
            start_row,
        } = self.start_index()?;
        let nframes = self.video_data()?.video_meta().nframes;
        let nrows = self.daq_data()?.daq_meta().nrows;
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

    fn setting_snapshot(&self) -> Result<SettingSnapshot> {
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
            completed_at: now_as_millis(),
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
        Ok(self.output_file_stem()?.with_extension("toml"))
    }
}
