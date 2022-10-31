use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use ndarray::{ArcArray2, Array2};
use serde::{Deserialize, Serialize};
use tlc_video::{FilterMethod, Progress, VideoMeta};
use tokio::sync::oneshot;
use tracing::trace;

use crate::{
    daq::{DaqMeta, InterpMethod, Thermocouple},
    setting::{self, StartIndex},
    solve::{IterationMethod, PhysicalParam},
};

pub enum Request {
    CreateSetting {
        create_setting: Box<SettingData>,
        responder: Responder<()>,
    },
    SwitchSetting {
        setting_id: i64,
        responder: Responder<()>,
    },
    DeleteSetting {
        setting_id: i64,
        responder: Responder<()>,
    },
    GetName {
        responder: Responder<String>,
    },
    SetName {
        name: String,
        responder: Responder<()>,
    },
    GetSaveRootDir {
        responder: Responder<PathBuf>,
    },
    SetSaveRootDir {
        save_root_dir: PathBuf,
        responder: Responder<()>,
    },
    GetVideoMeta {
        responder: Responder<VideoMeta>,
    },
    SetVideoPath {
        video_path: PathBuf,
        responder: Responder<()>,
    },
    GetReadVideoProgress {
        responder: Responder<Progress>,
    },
    DecodeFrameBase64 {
        frame_index: usize,
        responder: Responder<String>,
    },
    GetDaqMeta {
        responder: Responder<DaqMeta>,
    },
    SetDaqPath {
        daq_path: PathBuf,
        responder: Responder<()>,
    },
    GetDaqRaw {
        responder: Responder<ArcArray2<f64>>,
    },
    GetStartIndex {
        responder: Responder<StartIndex>,
    },
    SynchronizeVideoAndDaq {
        start_frame: usize,
        start_row: usize,
        responder: Responder<()>,
    },
    SetStartFrame {
        start_frame: usize,
        responder: Responder<()>,
    },
    SetStartRow {
        start_row: usize,
        responder: Responder<()>,
    },
    GetArea {
        responder: Responder<(u32, u32, u32, u32)>,
    },
    SetArea {
        area: (u32, u32, u32, u32),
        responder: Responder<()>,
    },
    GetBuildGreen2Progress {
        responder: Responder<Progress>,
    },
    GetFilterMethod {
        responder: Responder<FilterMethod>,
    },
    SetFilterMethod {
        filter_method: FilterMethod,
        responder: Responder<()>,
    },
    GetDetectPeakProgress {
        responder: Responder<Progress>,
    },
    FilterPoint {
        position: (usize, usize),
        responder: Responder<Vec<u8>>,
    },
    GetThermocouples {
        responder: Responder<Vec<Thermocouple>>,
    },
    SetThermocouples {
        thermocouples: Vec<Thermocouple>,
        responder: Responder<()>,
    },
    GetInterpMethod {
        responder: Responder<InterpMethod>,
    },
    SetInterpMethod {
        interp_method: InterpMethod,
        responder: Responder<()>,
    },
    InterpFrame {
        frame_index: usize,
        responder: Responder<Array2<f64>>,
    },
    GetIterationMethod {
        responder: Responder<IterationMethod>,
    },
    SetIterationMethod {
        iteration_method: IterationMethod,
        responder: Responder<()>,
    },
    GetPhysicalParam {
        responder: Responder<PhysicalParam>,
    },
    SetGmaxTemperature {
        gmax_temperature: f64,
        responder: Responder<()>,
    },
    SetSolidThermalConductivity {
        solid_thermal_conductivity: f64,
        responder: Responder<()>,
    },
    SetSolidThermalDiffusivity {
        solid_thermal_diffusivity: f64,
        responder: Responder<()>,
    },
    SetCharacteristicLength {
        characteristic_length: f64,
        responder: Responder<()>,
    },
    SetAirThermalConductivity {
        air_thermal_conductivity: f64,
        responder: Responder<()>,
    },
    GetNu {
        edge_truncation: Option<(f64, f64)>,
        responder: Responder<NuView>,
    },
}

#[derive(Debug, Deserialize)]
pub struct SettingData {
    pub name: String,
    pub save_root_dir: PathBuf,
    pub video_path: Option<PathBuf>,
    pub daq_path: Option<PathBuf>,
    pub start_frame: Option<usize>,
    pub start_row: Option<usize>,
    pub area: Option<(u32, u32, u32, u32)>,
    pub thermocouples: Option<Vec<Thermocouple>>,
    pub interp_method: Option<InterpMethod>,
    pub filter_method: Option<FilterMethod>,
    pub iteration_method: Option<IterationMethod>,
    pub physical_param: PhysicalParam,
}

impl From<SettingData> for setting::CreateRequest {
    fn from(s: SettingData) -> setting::CreateRequest {
        setting::CreateRequest {
            name: s.name,
            save_root_dir: s.save_root_dir,
            video_path: s.video_path,
            daq_path: s.daq_path,
            start_frame: s.start_frame,
            start_row: s.start_row,
            area: s.area,
            thermocouples: s.thermocouples,
            interp_method: s.interp_method,
            filter_method: s.filter_method.unwrap_or_default(),
            iteration_method: s.iteration_method.unwrap_or_default(),
            physical_param: s.physical_param,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NuView {
    pub nu2: ArcArray2<f64>,
    pub nu_nan_mean: f64,
    pub nu_plot_base64: String,
    pub edge_truncation: (f64, f64),
}

pub struct Responder<T> {
    name: String,
    payload: Option<String>,
    start_time: Instant,
    tx: oneshot::Sender<Result<T>>,
}

impl<T> Responder<T> {
    pub fn new(
        name: &str,
        parameter: Option<String>,
        tx: oneshot::Sender<Result<T>>,
    ) -> Responder<T> {
        Responder {
            name: name.to_owned(),
            payload: parameter,
            tx,
            start_time: Instant::now(),
        }
    }

    pub fn respond(self, result: Result<T>) {
        if self.tx.send(result).is_err() {
            panic!("failed to send back response");
        }

        let name = self.name;
        let payload = self.payload;
        let elapsed = self.start_time.elapsed();
        trace!(name, ?payload, ?elapsed);
    }

    pub fn respond_ok(self, v: T) {
        self.respond(Ok(v))
    }

    pub fn respond_err(self, e: anyhow::Error) {
        self.respond(Err(e))
    }

    #[cfg(test)]
    pub fn simple(tx: oneshot::Sender<Result<T>>) -> Responder<T> {
        Responder {
            name: "".to_owned(),
            payload: None,
            start_time: Instant::now(),
            tx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_respond_log_output() {
        tlc_util::log::init();

        let (tx, _rx) = oneshot::channel::<Result<()>>();
        let payload = "some_payload: aaa".to_owned();
        let responder = Responder::new("some_event", Some(payload), tx);
        responder.respond_ok(());
    }
}
