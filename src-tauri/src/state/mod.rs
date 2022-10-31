mod outcome_handler;
mod reconcile;
mod request_handler;

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use crossbeam::{
    channel::{bounded, Receiver, Sender},
    select,
};
use ndarray::ArcArray2;
use rusqlite::Connection;
use tlc_util::time::now_as_millis;
use tlc_video::{GmaxMeta, Green2Meta, VideoController, VideoData, VideoMeta};
use tracing::{error, warn};

use crate::{
    daq::{DaqData, DaqMeta, InterpMeta, InterpMethod, Interpolator, Thermocouple},
    request::Request,
    setting::{new_db, Setting, SettingSnapshot, StartIndex},
    solve::NuData,
};

use self::outcome_handler::Outcome;

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

struct GlobalState {
    setting: Setting,
    db: Connection,

    outcome_sender: Sender<Outcome>,
    outcome_receiver: Receiver<Outcome>,

    video_data: Option<VideoData>,
    video_controller: VideoController,

    daq_data: Option<DaqData>,

    nu_data: Option<NuData>,
}

pub fn main_loop(request_receiver: Receiver<Request>) {
    let db = new_db(SQLITE_FILEPATH);
    let mut global_state = GlobalState::new(db);
    loop {
        if let Err(e) = global_state.handle(&request_receiver) {
            error!("{e}");
        }
    }
}

impl GlobalState {
    fn new(db: Connection) -> Self {
        let (outcome_sender, outcome_receiver) = bounded(3);
        Self {
            setting: Setting::default(),
            db,
            outcome_sender,
            outcome_receiver,
            video_data: None,
            daq_data: None,
            video_controller: VideoController::default(),
            nu_data: None,
        }
    }

    /// `handle` keeps receiving `Request`(frontend message) and `Outcome`(computation
    /// result), then make decision what to do next based on the current global state.
    /// It should NEVER block or do any heavy computations, all blocking/time-consuming
    /// tasks should be executed in other threads and send back results asynchronously
    /// through `outcome_sender`.
    fn handle(&mut self, request_receiver: &Receiver<Request>) -> Result<()> {
        select! {
            recv(request_receiver)  -> request => self.handle_request(request?),
            recv(self.outcome_receiver) -> outcome => self.handle_outcome(outcome?)?,
        }
        Ok(())
    }

    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(Sender<Outcome>) + Send + 'static,
    {
        let outcome_sender = self.outcome_sender.clone();
        std::thread::spawn(move || f(outcome_sender));
    }

    fn video_data(&self) -> Result<&VideoData> {
        self.video_data
            .as_ref()
            .ok_or_else(|| anyhow!("video not loaded yet"))
    }

    fn video_meta(&self) -> Result<VideoMeta> {
        let video_path = self.setting.video_path(&self.db)?;

        let video_meta = self.video_data()?.video_meta();
        if video_meta.path != video_path {
            bail!("new video not loaded yet");
        }

        Ok(video_meta.clone())
    }

    fn daq_data(&self) -> Result<&DaqData> {
        self.daq_data
            .as_ref()
            .ok_or_else(|| anyhow!("daq not loaded yet"))
    }

    fn daq_meta(&self) -> Result<DaqMeta> {
        let daq_path = self.setting.daq_path(&self.db)?;
        let daq_meta = self.daq_data()?.daq_meta();
        if daq_meta.path != daq_path {
            bail!("new daq not loaded yet");
        }

        Ok(daq_meta.clone())
    }

    fn daq_raw(&self) -> Result<ArcArray2<f64>> {
        let daq_path = self.setting.daq_path(&self.db)?;
        let daq_data = self.daq_data()?;
        if daq_data.daq_meta().path != daq_path {
            warn!("new daq not loaded yet, return old data anyway");
        }

        Ok(daq_data.daq_raw())
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

    fn set_solid_thermal_diffusivity(&mut self, solid_thermal_diffusivity: f64) -> Result<()> {
        self.setting
            .set_solid_thermal_diffusivity(&self.db, solid_thermal_diffusivity)?;
        self.nu_data = None;
        let _ = self.spawn_solve();
        Ok(())
    }

    fn green2_meta(&self) -> Result<Green2Meta> {
        let video_data = self.video_data()?;
        let video_meta = video_data.video_meta().clone();
        let StartIndex {
            start_frame,
            start_row,
        } = self.start_index()?;
        let nframes = video_meta.nframes;
        let nrows = self.daq_meta()?.nrows;
        let cal_num = (nframes - start_frame).min(nrows - start_row);
        let area = self.area()?;

        Ok(Green2Meta {
            video_meta,
            start_frame,
            cal_num,
            area,
        })
    }

    fn gmax_meta(&self) -> Result<GmaxMeta> {
        let green2_meta = self.green2_meta()?;
        let filter_method = self.setting.filter_method(&self.db)?;

        Ok(GmaxMeta {
            filter_method,
            green2_meta,
        })
    }

    fn interp_meta(&self) -> Result<InterpMeta> {
        let daq_path = self.setting.daq_path(&self.db)?;
        let daq_meta = self.daq_meta()?;
        if daq_meta.path != daq_path {
            bail!("daq path changed");
        }
        let start_row = self.start_index()?.start_row;
        let Green2Meta { cal_num, area, .. } = self.green2_meta()?;
        let thermocouples = self.thermocouples()?;
        let interp_method = self.interp_method()?;

        Ok(InterpMeta {
            daq_meta,
            start_row,
            cal_num,
            area,
            interp_method,
            thermocouples,
        })
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
            video_meta: self.video_meta()?,
            daq_meta: self.daq_meta()?,
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

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot;

    use super::*;
    use crate::{
        request::{Responder, SettingData},
        setting::new_db_in_memory,
        solve::PhysicalParam,
    };

    #[test]
    fn test1() {
        let sender = spawn_main_loop();

        let (tx, rx) = oneshot::channel();
        sender
            .send(Request::GetFilterMethod {
                responder: Responder::simple(tx),
            })
            .unwrap();
        let ret = rx.blocking_recv().unwrap().unwrap();
        println!("{ret:?}")
    }

    fn spawn_main_loop() -> Sender<Request> {
        let (request_sender, request_receiver) = bounded(3);
        let db = new_db_in_memory();
        let mut global_state = GlobalState::new(db);
        std::thread::spawn(move || loop {
            if let Err(e) = global_state.handle(&request_receiver) {
                error!("{e}");
            }
        });

        let (tx, rx) = oneshot::channel();
        request_sender
            .send(Request::CreateSetting {
                create_setting: Box::new(SettingData {
                    name: "test_case".to_owned(),
                    save_root_dir: PathBuf::from("fake_save_root_dir"),
                    video_path: None,
                    daq_path: None,
                    start_frame: None,
                    start_row: None,
                    area: None,
                    thermocouples: None,
                    interp_method: None,
                    filter_method: None,
                    iteration_method: None,
                    physical_param: PhysicalParam {
                        gmax_temperature: 35.48,
                        solid_thermal_conductivity: 0.19,
                        solid_thermal_diffusivity: 1.091e-7,
                        characteristic_length: 0.015,
                        air_thermal_conductivity: 0.0276,
                    },
                }),
                responder: Responder::simple(tx),
            })
            .unwrap();
        rx.blocking_recv().unwrap().unwrap();

        request_sender
    }
}
