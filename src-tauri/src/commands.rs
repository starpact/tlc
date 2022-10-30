use std::path::PathBuf;

use anyhow::Result;
use crossbeam::channel::Sender;
use function_name::named;
use ndarray::{ArcArray2, Array2};
use serde::Serialize;
use tlc_video::{FilterMethod, Progress, VideoMeta};
use tokio::sync::oneshot::{self, error::RecvError};

use crate::{
    daq::{DaqMeta, InterpMethod},
    old_state::{CreateSettingRequest, GlobalState, NuData},
    request::{
        Request::{self, *},
        Responder,
    },
    setting::{SqliteSettingStorage, StartIndex},
    solve::IterationMethod,
};

type State<'a> = tauri::State<'a, GlobalState<SqliteSettingStorage>>;

type RequestSender<'a> = tauri::State<'a, Sender<Request>>;

type TlcResult<T> = Result<T, String>;

#[tauri::command]
pub async fn create_setting(request: CreateSettingRequest, state: State<'_>) -> TlcResult<()> {
    state.create_setting(request).await.to()
}

#[tauri::command]
pub async fn switch_setting(setting_id: i64, state: State<'_>) -> TlcResult<()> {
    state.switch_setting(setting_id).await.to()
}

#[named]
#[tauri::command]
pub async fn get_save_root_dir(request_sender: RequestSender<'_>) -> TlcResult<PathBuf> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetSaveRootDir {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_save_root_dir(
    save_root_dir: PathBuf,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("save_root_dir: {save_root_dir:?}");
    let _ = request_sender.try_send(SetSaveRootDir {
        save_root_dir,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_video_meta(request_sender: RequestSender<'_>) -> TlcResult<VideoMeta> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetVideoMeta {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_video_path(
    video_path: PathBuf,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("video_path: {video_path:?}");
    let _ = request_sender.try_send(SetVideoPath {
        video_path,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_read_video_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetReadVideoProgress {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn decode_frame_base64(
    frame_index: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<String> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("frame_index: {frame_index}");
    let _ = request_sender.try_send(DecodeFrameBase64 {
        frame_index,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_daq_meta(request_sender: RequestSender<'_>) -> TlcResult<DaqMeta> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDaqMeta {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_daq_path(daq_path: PathBuf, request_sender: RequestSender<'_>) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("daq_path: {daq_path:?}");
    let _ = request_sender.try_send(SetDaqPath {
        daq_path,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_daq_raw(request_sender: RequestSender<'_>) -> TlcResult<ArcArray2<f64>> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDaqRaw {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("start_frame: {start_frame}, start_row: {start_row}");
    let _ = request_sender.try_send(SynchronizeVideoAndDaq {
        start_frame,
        start_row,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_start_index(request_sender: RequestSender<'_>) -> TlcResult<StartIndex> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetStartIndex {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_start_frame(
    start_frame: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("start_frame: {start_frame}");
    let _ = request_sender.try_send(SetStartFrame {
        start_frame,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_start_row(start_row: usize, request_sender: RequestSender<'_>) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("start_row: {start_row}");
    let _ = request_sender.try_send(SetStartRow {
        start_row,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_area(request_sender: RequestSender<'_>) -> TlcResult<(u32, u32, u32, u32)> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetArea {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_area(
    area: (u32, u32, u32, u32),
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("area: {area:?}");
    let _ = request_sender.try_send(SetArea {
        area,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_build_green2_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetBuildGreen2Progress {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn get_filter_method(request_sender: RequestSender<'_>) -> TlcResult<FilterMethod> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetFilterMethod {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn set_filter_method(
    filter_method: FilterMethod,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("filter_method: {filter_method:?}");
    let _ = request_sender.try_send(SetFilterMethod {
        filter_method,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[tauri::command]
pub async fn filter_single_point(position: (usize, usize), state: State<'_>) -> TlcResult<Vec<u8>> {
    todo!()
}

#[named]
#[tauri::command]
pub async fn get_detect_peak_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    let (tx, rx) = oneshot::channel();
    let _ = request_sender.try_send(GetDetectPeakProgress {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.to()
}

#[tauri::command]
pub fn set_thermocouples() -> TlcResult<()> {
    todo!()
}

#[tauri::command]
pub async fn get_interpolation_method(state: State<'_>) -> TlcResult<InterpMethod> {
    state.get_interpolation_method().await.to()
}

#[named]
#[tauri::command]
pub async fn set_interp_method(
    interp_method: InterpMethod,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("interp_method: {interp_method:?}");
    let _ = request_sender.try_send(SetInterpMethod {
        interp_method,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[named]
#[tauri::command]
pub async fn interp_single_frame(
    frame_index: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<Array2<f64>> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("frame_index: {frame_index}");
    let _ = request_sender.try_send(InterpSingleFrame {
        frame_index,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.to()
}

#[tauri::command]
pub async fn get_iteration_method(state: State<'_>) -> TlcResult<IterationMethod> {
    state.get_iteration_method().await.to()
}

#[tauri::command]
pub async fn set_iteration_method(
    state: State<'_>,
    iteration_method: IterationMethod,
) -> TlcResult<()> {
    state.set_iteration_method(iteration_method).await.to()
}

#[tauri::command]
pub async fn set_gmax_temperature(gmax_temperature: f64, state: State<'_>) -> TlcResult<()> {
    state.set_gmax_temperature(gmax_temperature).await.to()
}

#[tauri::command]
pub async fn set_solid_thermal_conductivity(
    solid_thermal_conductivity: f64,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .set_solid_thermal_conductivity(solid_thermal_conductivity)
        .await
        .to()
}

#[tauri::command]
pub async fn set_solid_thermal_diffusivity(
    solid_thermal_diffusivity: f64,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .set_solid_thermal_diffusivity(solid_thermal_diffusivity)
        .await
        .to()
}

#[tauri::command]
pub async fn set_characteristic_length(
    characteristic_length: f64,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .set_characteristic_length(characteristic_length)
        .await
        .to()
}

#[tauri::command]
pub async fn set_air_thermal_conductivity(
    air_thermal_conductivity: f64,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .set_air_thermal_conductivity(air_thermal_conductivity)
        .await
        .to()
}

#[tauri::command]
pub async fn get_nu(edge_truncation: Option<(f64, f64)>, state: State<'_>) -> TlcResult<NuData> {
    state.get_nu(edge_truncation).await.to()
}

trait IntoTlcResult<T> {
    fn to(self) -> TlcResult<T>;
}

impl<T: Serialize> IntoTlcResult<T> for Result<T> {
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())
    }
}

impl<T: Serialize> IntoTlcResult<T> for core::result::Result<Result<T>, RecvError> {
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())?.to()
    }
}
