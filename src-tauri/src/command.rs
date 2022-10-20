use std::path::PathBuf;

use anyhow::Result;
use ndarray::{ArcArray2, Array2};
use serde::Serialize;

use crate::{
    daq::{DaqMetadata, InterpolationMethod},
    setting::{SqliteSettingStorage, StartIndex},
    solve::IterationMethod,
    state::{CreateSettingRequest, GlobalState, NuData},
    video::{FilterMethod, Progress, VideoMetadata},
};

type State<'a> = tauri::State<'a, GlobalState<SqliteSettingStorage>>;

type TlcResult<T> = Result<T, String>;

trait IntoTlcResult<T> {
    fn to(self) -> TlcResult<T>;
}

impl<T: Serialize> IntoTlcResult<T> for Result<T> {
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| format!("{e:?}"))
    }
}

#[tauri::command]
pub async fn create_setting(request: CreateSettingRequest, state: State<'_>) -> TlcResult<()> {
    state.create_setting(request).await.to()
}

#[tauri::command]
pub async fn switch_setting(setting_id: i64, state: State<'_>) -> TlcResult<()> {
    state.switch_setting(setting_id).await.to()
}

#[tauri::command]
pub async fn get_save_root_dir(state: State<'_>) -> TlcResult<PathBuf> {
    state.get_save_root_dir().await.to()
}

#[tauri::command]
pub async fn set_save_root_dir(save_root_dir: PathBuf, state: State<'_>) -> TlcResult<()> {
    state.set_save_root_dir(save_root_dir).await.to()
}

#[tauri::command]
pub async fn get_video_metadata(state: State<'_>) -> TlcResult<VideoMetadata> {
    state.get_video_metadata().await.to()
}

#[tauri::command]
pub async fn set_video_path(video_path: PathBuf, state: State<'_>) -> TlcResult<()> {
    state.set_video_path(video_path).await.to()
}

#[tauri::command]
pub async fn get_daq_metadata(state: State<'_>) -> TlcResult<DaqMetadata> {
    state.get_daq_metadata().await.to()
}

#[tauri::command]
pub async fn set_daq_path(daq_path: PathBuf, state: State<'_>) -> TlcResult<()> {
    state.set_daq_path(daq_path).await.to()
}

#[tauri::command]
pub async fn read_single_frame_base64(frame_index: usize, state: State<'_>) -> TlcResult<String> {
    state.read_single_frame_base64(frame_index).await.to()
}

#[tauri::command]
pub async fn get_daq_raw(state: State<'_>) -> TlcResult<ArcArray2<f64>> {
    state.get_daq_raw().await.to()
}

#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .synchronize_video_and_daq(start_frame, start_row)
        .await
        .to()
}

#[tauri::command]
pub async fn get_start_index(state: State<'_>) -> TlcResult<StartIndex> {
    state.get_start_index().await.to()
}

#[tauri::command]
pub async fn set_start_frame(start_frame: usize, state: State<'_>) -> TlcResult<()> {
    state.set_start_frame(start_frame).await.to()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, state: State<'_>) -> TlcResult<()> {
    state.set_start_row(start_row).await.to()
}

#[tauri::command]
pub async fn get_area(state: State<'_>) -> TlcResult<(usize, usize, usize, usize)> {
    state.get_area().await.to()
}

#[tauri::command]
pub async fn set_area(state: State<'_>, area: (usize, usize, usize, usize)) -> TlcResult<()> {
    state.set_area(area).await.to()
}

#[tauri::command]
pub fn set_thermocouples() -> TlcResult<()> {
    todo!()
}

#[tauri::command]
pub async fn build_green2(state: State<'_>) -> TlcResult<()> {
    state.spawn_build_green2().await.to()
}

#[tauri::command]
pub async fn get_build_green2_progress(state: State<'_>) -> TlcResult<Progress> {
    Ok(state.get_build_green2_progress())
}

#[tauri::command]
pub async fn get_filter_method(state: State<'_>) -> TlcResult<FilterMethod> {
    state.get_filter_method().await.to()
}

#[tauri::command]
pub async fn set_filter_method(filter_method: FilterMethod, state: State<'_>) -> TlcResult<()> {
    state.set_filter_method(filter_method).await.to()
}

#[tauri::command]
pub async fn filter_single_point(position: (usize, usize), state: State<'_>) -> TlcResult<Vec<u8>> {
    state.filter_single_point(position).await.to()
}

#[tauri::command]
pub async fn detect_peak(state: State<'_>) -> TlcResult<()> {
    state.spawn_detect_peak().await.to()
}

#[tauri::command]
pub async fn get_detect_peak_progress(state: State<'_>) -> TlcResult<Progress> {
    Ok(state.get_detect_peak_progress())
}

#[tauri::command]
pub async fn get_interpolation_method(state: State<'_>) -> TlcResult<InterpolationMethod> {
    state.get_interpolation_method().await.to()
}

#[tauri::command]
pub async fn set_interpolation_method(
    interpolation_method: InterpolationMethod,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .set_interpolation_method(interpolation_method)
        .await
        .to()
}

#[tauri::command]
pub async fn interpolate_single_frame(
    frame_index: usize,
    state: State<'_>,
) -> TlcResult<Array2<f64>> {
    state.interpolate_single_frame(frame_index).await.to()
}

#[tauri::command]
pub async fn interpolate(state: State<'_>) -> TlcResult<()> {
    state.interpolate().await.to()
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
pub async fn solve(state: State<'_>) -> TlcResult<()> {
    state.solve().await.to()
}

#[tauri::command]
pub async fn get_nu(edge_truncation: Option<(f64, f64)>, state: State<'_>) -> TlcResult<NuData> {
    state.get_nu(edge_truncation).await.to()
}
