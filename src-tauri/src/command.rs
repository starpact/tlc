use std::path::PathBuf;

use anyhow::Result;
use ndarray::{ArcArray2, Array2};
use serde::Serialize;
use tauri::async_runtime::spawn_blocking;
use tlc_util::progress_bar::Progress;
use tlc_video::{FilterMethod, VideoMeta};

use crate::{
    daq::{DaqMeta, InterpMethod, Thermocouple},
    setting::StartIndex,
    solve::{IterationMethod, PhysicalParam},
    state::{GlobalState, NuView, SettingData},
};

type State<'a> = tauri::State<'a, GlobalState>;

type TlcResult<T> = Result<T, String>;

trait IntoTlcResult<T> {
    fn to(self) -> TlcResult<T>;
}

impl<T: Serialize> IntoTlcResult<T> for Result<T> {
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())
    }
}

impl<T: Serialize> IntoTlcResult<T> for core::result::Result<Result<T>, tauri::Error> {
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
    }
}

async fn wrap_blocking<F, T>(state: State<'_>, f: F) -> TlcResult<T>
where
    T: Serialize + Send + 'static,
    F: FnOnce(GlobalState) -> Result<T> + Send + 'static,
{
    let state = (*state).clone();
    spawn_blocking(move || f(state)).await.to()
}

#[tauri::command]
pub async fn create_setting(state: State<'_>, setting_data: SettingData) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.create_setting(setting_data)).await
}

#[tauri::command]
pub async fn switch_setting(state: State<'_>, setting_id: i64) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.switch_setting(setting_id)).await
}

#[tauri::command]
pub async fn delete_setting(state: State<'_>, setting_id: i64) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.delete_setting(setting_id)).await
}

#[tauri::command]
pub async fn get_name(state: State<'_>) -> TlcResult<String> {
    wrap_blocking(state, move |s| s.get_name()).await
}

#[tauri::command]
pub async fn set_name(state: State<'_>, name: String) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_name(name)).await
}

#[tauri::command]
pub async fn get_save_root_dir(state: State<'_>) -> TlcResult<PathBuf> {
    wrap_blocking(state, move |s| s.get_save_root_dir()).await
}

#[tauri::command]
pub async fn set_save_root_dir(state: State<'_>, save_root_dir: PathBuf) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_save_root_dir(save_root_dir)).await
}

#[tauri::command]
pub async fn get_video_path(state: State<'_>) -> TlcResult<PathBuf> {
    wrap_blocking(state, move |s| s.get_video_path()).await
}

#[tauri::command]
pub async fn set_video_path(state: State<'_>, video_path: PathBuf) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_video_path(video_path)).await
}

#[tauri::command]
pub fn get_video_meta(state: State<'_>) -> TlcResult<VideoMeta> {
    state.get_video_meta().to()
}

#[tauri::command]
pub fn get_read_video_progress(state: State<'_>) -> Progress {
    state.get_read_video_progress()
}

#[tauri::command]
pub async fn decode_frame_base64(state: State<'_>, frame_index: usize) -> TlcResult<String> {
    wrap_blocking(state, move |s| s.decode_frame_base64(frame_index)).await
}

#[tauri::command]
pub async fn get_daq_path(state: State<'_>) -> TlcResult<PathBuf> {
    wrap_blocking(state, move |s| s.get_daq_path()).await
}

#[tauri::command]
pub async fn set_daq_path(state: State<'_>, daq_path: PathBuf) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_daq_path(daq_path)).await
}

#[tauri::command]
pub fn get_daq_meta(state: State<'_>) -> TlcResult<DaqMeta> {
    state.get_daq_meta().to()
}

#[tauri::command]
pub fn get_daq_raw(state: State<'_>) -> TlcResult<ArcArray2<f64>> {
    state.get_daq_raw().to()
}

#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    state: State<'_>,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| {
        s.synchronize_video_and_daq(start_frame, start_row)
    })
    .await
}

#[tauri::command]
pub async fn get_start_index(state: State<'_>) -> TlcResult<StartIndex> {
    wrap_blocking(state, move |s| s.get_start_index()).await
}

#[tauri::command]
pub async fn set_start_frame(state: State<'_>, start_frame: usize) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_start_frame(start_frame)).await
}

#[tauri::command]
pub async fn set_start_row(state: State<'_>, start_row: usize) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_start_row(start_row)).await
}

#[tauri::command]
pub async fn get_area(state: State<'_>) -> TlcResult<(u32, u32, u32, u32)> {
    wrap_blocking(state, move |s| s.get_area()).await
}

#[tauri::command]
pub async fn set_area(state: State<'_>, area: (u32, u32, u32, u32)) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_area(area)).await
}

#[tauri::command]
pub fn get_build_green2_progress(state: State<'_>) -> Progress {
    state.get_build_green2_progress()
}

#[tauri::command]
pub async fn get_filter_method(state: State<'_>) -> TlcResult<FilterMethod> {
    wrap_blocking(state, move |s| s.get_filter_method()).await
}

#[tauri::command]
pub async fn set_filter_method(state: State<'_>, filter_method: FilterMethod) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_filter_method(filter_method)).await
}

#[tauri::command]
pub fn get_detect_peak_progress(state: State<'_>) -> Progress {
    state.get_detect_peak_progress()
}

#[tauri::command]
pub async fn filter_point(state: State<'_>, position: (usize, usize)) -> TlcResult<Vec<u8>> {
    wrap_blocking(state, move |s| s.filter_point(position)).await
}

#[tauri::command]
pub async fn get_thermocouples(state: State<'_>) -> TlcResult<Vec<Thermocouple>> {
    wrap_blocking(state, move |s| s.get_thermocouples()).await
}

#[tauri::command]
pub async fn set_thermocouples(
    state: State<'_>,
    thermocouples: Vec<Thermocouple>,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_thermocouples(thermocouples)).await
}

#[tauri::command]
pub async fn get_interp_method(state: State<'_>) -> TlcResult<InterpMethod> {
    wrap_blocking(state, move |s| s.get_interp_method()).await
}

#[tauri::command]
pub async fn set_interp_method(state: State<'_>, interp_method: InterpMethod) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_interp_method(interp_method)).await
}

#[tauri::command]
pub async fn interp_frame(state: State<'_>, frame_index: usize) -> TlcResult<Array2<f64>> {
    wrap_blocking(state, move |s| s.interp_frame(frame_index)).await
}

#[tauri::command]
pub async fn get_iteration_method(state: State<'_>) -> TlcResult<IterationMethod> {
    wrap_blocking(state, move |s| s.get_iteration_method()).await
}

#[tauri::command]
pub async fn set_iteration_method(
    state: State<'_>,
    iteration_method: IterationMethod,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_iteration_method(iteration_method)).await
}

#[tauri::command]
pub async fn get_physical_param(state: State<'_>) -> TlcResult<PhysicalParam> {
    wrap_blocking(state, move |s| s.get_physical_param()).await
}

#[tauri::command]
pub async fn set_gmax_temperature(state: State<'_>, gmax_temperature: f64) -> TlcResult<()> {
    wrap_blocking(state, move |s| s.set_gmax_temperature(gmax_temperature)).await
}

#[tauri::command]
pub async fn set_solid_thermal_conductivity(
    state: State<'_>,
    solid_thermal_conductivity: f64,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| {
        s.set_solid_thermal_conductivity(solid_thermal_conductivity)
    })
    .await
}

#[tauri::command]
pub async fn set_solid_thermal_diffusivity(
    state: State<'_>,
    solid_thermal_diffusivity: f64,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| {
        s.set_solid_thermal_diffusivity(solid_thermal_diffusivity)
    })
    .await
}

#[tauri::command]
pub async fn set_characteristic_length(
    state: State<'_>,
    characteristic_length: f64,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| {
        s.set_characteristic_length(characteristic_length)
    })
    .await
}

#[tauri::command]
pub async fn set_air_thermal_conductivity(
    state: State<'_>,
    air_thermal_conductivity: f64,
) -> TlcResult<()> {
    wrap_blocking(state, move |s| {
        s.set_air_thermal_conductivity(air_thermal_conductivity)
    })
    .await
}

#[tauri::command]
pub fn get_solve_progress(state: State<'_>) -> Progress {
    state.get_solve_progress()
}

#[tauri::command]
pub async fn get_nu(state: State<'_>, edge_truncation: Option<(f64, f64)>) -> TlcResult<NuView> {
    wrap_blocking(state, move |s| s.get_nu(edge_truncation)).await
}
