use std::{fmt::Debug, path::PathBuf, time::Instant};

use crate::{
    daq::{DaqMeta, InterpMethod, Thermocouple},
    setting::{self, StartIndex},
    solve::{IterationMethod, PhysicalParam},
};
use anyhow::Result;
use crossbeam::channel::Sender;
use function_name::named;
use ndarray::{ArcArray2, Array2};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::trace;

use crate::{
    util::progress_bar::Progress,
    video::{FilterMethod, VideoMeta},
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
    GetVideoPath {
        responder: Responder<PathBuf>,
    },
    SetVideoPath {
        video_path: PathBuf,
        responder: Responder<()>,
    },
    GetVideoMeta {
        responder: Responder<VideoMeta>,
    },
    GetReadVideoProgress {
        responder: Responder<Progress>,
    },
    DecodeFrameBase64 {
        frame_index: usize,
        responder: Responder<String>,
    },
    GetDaqPath {
        responder: Responder<PathBuf>,
    },
    SetDaqPath {
        daq_path: PathBuf,
        responder: Responder<()>,
    },
    GetDaqMeta {
        responder: Responder<DaqMeta>,
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
    GetSolveProgress {
        responder: Responder<Progress>,
    },
    GetNu {
        edge_truncation: Option<(f64, f64)>,
        responder: Responder<NuView>,
    },
}

use Request::*;

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
    start_time: Instant,
    tx: oneshot::Sender<Result<T>>,
}

impl<T: Debug + Serialize> Responder<T> {
    fn new(name: &str, tx: oneshot::Sender<Result<T>>) -> Responder<T> {
        Responder {
            name: name.to_owned(),
            tx,
            start_time: Instant::now(),
        }
    }

    pub(crate) fn respond(self, result: Result<T>) {
        self.respond_inner(result, true);
    }

    pub(crate) fn respond_no_result_log(self, result: Result<T>) {
        self.respond_inner(result, false);
    }

    pub(crate) fn respond_ok(self, v: T) {
        self.respond(Ok(v))
    }

    pub(crate) fn respond_ok_no_result_log(self, v: T) {
        self.respond_no_result_log(Ok(v))
    }

    pub(crate) fn respond_err(self, e: anyhow::Error) {
        self.respond(Err(e))
    }

    fn respond_inner(self, result: Result<T>, print_result: bool) {
        let name = self.name;
        let elapsed = self.start_time.elapsed();
        match &result {
            Ok(result) => {
                if print_result {
                    trace!(name, ?result, ?elapsed, "respond_ok");
                } else {
                    trace!(name, ?elapsed, "respond_ok");
                }
            }
            Err(e) => trace!(name, %e, ?elapsed, "respond_err"),
        }

        if self.tx.send(result).is_err() {
            panic!("failed to send back response");
        }
    }
}

#[named]
pub async fn create_setting(
    create_setting: SettingData,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(CreateSetting {
        create_setting: Box::new(create_setting),
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn switch_setting(setting_id: i64, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SwitchSetting {
        setting_id,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn delete_setting(setting_id: i64, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(DeleteSetting {
        setting_id,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_name(request_sender: &Sender<Request>) -> Result<String> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetName {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_name(name: String, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetName {
        name,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_save_root_dir(request_sender: &Sender<Request>) -> Result<PathBuf> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetSaveRootDir {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_save_root_dir(
    save_root_dir: PathBuf,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetSaveRootDir {
        save_root_dir,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_video_path(request_sender: &Sender<Request>) -> Result<PathBuf> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetVideoPath {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_video_path(video_path: PathBuf, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetVideoPath {
        video_path,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_video_meta(request_sender: &Sender<Request>) -> Result<VideoMeta> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetVideoMeta {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_read_video_progress(request_sender: &Sender<Request>) -> Result<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetReadVideoProgress {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn decode_frame_base64(
    frame_index: usize,
    request_sender: &Sender<Request>,
) -> Result<String> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(DecodeFrameBase64 {
        frame_index,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_daq_path(request_sender: &Sender<Request>) -> Result<PathBuf> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDaqPath {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_daq_path(daq_path: PathBuf, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetDaqPath {
        daq_path,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_daq_meta(request_sender: &Sender<Request>) -> Result<DaqMeta> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDaqMeta {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_daq_raw(request_sender: &Sender<Request>) -> Result<ArcArray2<f64>> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDaqRaw {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SynchronizeVideoAndDaq {
        start_frame,
        start_row,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_start_index(request_sender: &Sender<Request>) -> Result<StartIndex> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetStartIndex {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_start_frame(start_frame: usize, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetStartFrame {
        start_frame,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_start_row(start_row: usize, request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetStartRow {
        start_row,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_area(request_sender: &Sender<Request>) -> Result<(u32, u32, u32, u32)> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetArea {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_area(area: (u32, u32, u32, u32), request_sender: &Sender<Request>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetArea {
        area,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_build_green2_progress(request_sender: &Sender<Request>) -> Result<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetBuildGreen2Progress {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_filter_method(request_sender: &Sender<Request>) -> Result<FilterMethod> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetFilterMethod {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_filter_method(
    filter_method: FilterMethod,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetFilterMethod {
        filter_method,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_detect_peak_progress(request_sender: &Sender<Request>) -> Result<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDetectPeakProgress {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn filter_point(
    position: (usize, usize),
    request_sender: &Sender<Request>,
) -> Result<Vec<u8>> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(FilterPoint {
        position,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_thermocouples(request_sender: &Sender<Request>) -> Result<Vec<Thermocouple>> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetThermocouples {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_thermocouples(
    thermocouples: Vec<Thermocouple>,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetThermocouples {
        thermocouples,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_interp_method(request_sender: &Sender<Request>) -> Result<InterpMethod> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetInterpMethod {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_interp_method(
    interp_method: InterpMethod,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetInterpMethod {
        interp_method,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn interp_frame(
    frame_index: usize,
    request_sender: &Sender<Request>,
) -> Result<Array2<f64>> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(InterpFrame {
        frame_index,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_iteration_method(request_sender: &Sender<Request>) -> Result<IterationMethod> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetIterationMethod {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_iteration_method(
    iteration_method: IterationMethod,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetIterationMethod {
        iteration_method,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_physical_param(request_sender: &Sender<Request>) -> Result<PhysicalParam> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetPhysicalParam {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_gmax_temperature(
    gmax_temperature: f64,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetGmaxTemperature {
        gmax_temperature,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_solid_thermal_conductivity(
    solid_thermal_conductivity: f64,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetSolidThermalConductivity {
        solid_thermal_conductivity,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_solid_thermal_diffusivity(
    solid_thermal_diffusivity: f64,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetSolidThermalDiffusivity {
        solid_thermal_diffusivity,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_characteristic_length(
    characteristic_length: f64,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetCharacteristicLength {
        characteristic_length,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn set_air_thermal_conductivity(
    air_thermal_conductivity: f64,
    request_sender: &Sender<Request>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(SetAirThermalConductivity {
        air_thermal_conductivity,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_solve_progress(request_sender: &Sender<Request>) -> Result<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetSolveProgress {
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[named]
pub async fn get_nu(
    edge_truncation: Option<(f64, f64)>,
    request_sender: &Sender<Request>,
) -> Result<NuView> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetNu {
        edge_truncation,
        responder: Responder::new(function_name!(), tx),
    });
    rx.await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_respond_log_output() {
        let (tx, _rx) = oneshot::channel::<Result<()>>();
        let responder = Responder::new("some_event", tx);
        responder.respond_ok(());
    }

    impl Default for SettingData {
        fn default() -> SettingData {
            SettingData {
                name: "test_case".to_owned(),
                save_root_dir: PathBuf::from("/tmp"),
                video_path: None,
                daq_path: None,
                start_frame: None,
                start_row: None,
                area: None,
                thermocouples: None,
                interp_method: None,
                filter_method: None,
                iteration_method: None,
                physical_param: PhysicalParam::default(),
            }
        }
    }
}
