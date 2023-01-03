use std::path::PathBuf;

use anyhow::Result;
use crossbeam::channel::Sender;
use ndarray::{ArcArray2, Array2};
use serde::Serialize;

use tlc_core::{
    request::{self, NuView, Request, SettingData},
    DaqMeta, FilterMethod, InterpMethod, IterationMethod, PhysicalParam, Progress, StartIndex,
    Thermocouple, VideoMeta,
};

type RequestSender<'a> = tauri::State<'a, Sender<Request>>;

type TlcResult<T> = Result<T, String>;

#[tauri::command]
pub async fn create_setting(
    create_setting: SettingData,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::create_setting(create_setting, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn switch_setting(setting_id: i64, request_sender: RequestSender<'_>) -> TlcResult<()> {
    request::switch_setting(setting_id, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn delete_setting(setting_id: i64, request_sender: RequestSender<'_>) -> TlcResult<()> {
    request::delete_setting(setting_id, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_name(request_sender: RequestSender<'_>) -> TlcResult<String> {
    request::get_name(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_name(name: String, request_sender: RequestSender<'_>) -> TlcResult<()> {
    request::set_name(name, &request_sender).await.to()
}

#[tauri::command]
pub async fn get_save_root_dir(request_sender: RequestSender<'_>) -> TlcResult<PathBuf> {
    request::get_save_root_dir(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_save_root_dir(
    save_root_dir: PathBuf,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_save_root_dir(save_root_dir, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_video_path(request_sender: RequestSender<'_>) -> TlcResult<PathBuf> {
    request::get_video_path(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_video_path(
    video_path: PathBuf,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_video_path(video_path, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_video_meta(request_sender: RequestSender<'_>) -> TlcResult<VideoMeta> {
    request::get_video_meta(&request_sender).await.to()
}

#[tauri::command]
pub async fn get_read_video_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    request::get_read_video_progress(&request_sender).await.to()
}

#[tauri::command]
pub async fn decode_frame_base64(
    frame_index: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<String> {
    request::decode_frame_base64(frame_index, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_daq_path(request_sender: RequestSender<'_>) -> TlcResult<PathBuf> {
    request::get_daq_path(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_daq_path(daq_path: PathBuf, request_sender: RequestSender<'_>) -> TlcResult<()> {
    request::set_daq_path(daq_path, &request_sender).await.to()
}

#[tauri::command]
pub async fn get_daq_meta(request_sender: RequestSender<'_>) -> TlcResult<DaqMeta> {
    request::get_daq_meta(&request_sender).await.to()
}

#[tauri::command]
pub async fn get_daq_raw(request_sender: RequestSender<'_>) -> TlcResult<ArcArray2<f64>> {
    request::get_daq_raw(&request_sender).await.to()
}

#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::synchronize_video_and_daq(start_frame, start_row, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_start_index(request_sender: RequestSender<'_>) -> TlcResult<StartIndex> {
    request::get_start_index(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_start_frame(
    start_frame: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_start_frame(start_frame, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, request_sender: RequestSender<'_>) -> TlcResult<()> {
    request::set_start_row(start_row, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_area(request_sender: RequestSender<'_>) -> TlcResult<(u32, u32, u32, u32)> {
    request::get_area(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_area(
    area: (u32, u32, u32, u32),
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_area(area, &request_sender).await.to()
}

#[tauri::command]
pub async fn get_build_green2_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    request::get_build_green2_progress(&request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_filter_method(request_sender: RequestSender<'_>) -> TlcResult<FilterMethod> {
    request::get_filter_method(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_filter_method(
    filter_method: FilterMethod,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_filter_method(filter_method, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_detect_peak_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    request::get_detect_peak_progress(&request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn filter_point(
    position: (usize, usize),
    request_sender: RequestSender<'_>,
) -> TlcResult<Vec<u8>> {
    request::filter_point(position, &request_sender).await.to()
}

#[tauri::command]
pub async fn get_thermocouples(request_sender: RequestSender<'_>) -> TlcResult<Vec<Thermocouple>> {
    request::get_thermocouples(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_thermocouples(
    thermocouples: Vec<Thermocouple>,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_thermocouples(thermocouples, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_interp_method(request_sender: RequestSender<'_>) -> TlcResult<InterpMethod> {
    request::get_interp_method(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_interp_method(
    interp_method: InterpMethod,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_interp_method(interp_method, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn interp_frame(
    frame_index: usize,
    request_sender: RequestSender<'_>,
) -> TlcResult<Array2<f64>> {
    request::interp_frame(frame_index, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_iteration_method(request_sender: RequestSender<'_>) -> TlcResult<IterationMethod> {
    request::get_iteration_method(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_iteration_method(
    iteration_method: IterationMethod,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_iteration_method(iteration_method, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_physical_param(request_sender: RequestSender<'_>) -> TlcResult<PhysicalParam> {
    request::get_physical_param(&request_sender).await.to()
}

#[tauri::command]
pub async fn set_gmax_temperature(
    gmax_temperature: f64,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_gmax_temperature(gmax_temperature, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn set_solid_thermal_conductivity(
    solid_thermal_conductivity: f64,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_solid_thermal_conductivity(solid_thermal_conductivity, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn set_solid_thermal_diffusivity(
    solid_thermal_diffusivity: f64,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_solid_thermal_diffusivity(solid_thermal_diffusivity, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn set_characteristic_length(
    characteristic_length: f64,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_characteristic_length(characteristic_length, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn set_air_thermal_conductivity(
    air_thermal_conductivity: f64,
    request_sender: RequestSender<'_>,
) -> TlcResult<()> {
    request::set_air_thermal_conductivity(air_thermal_conductivity, &request_sender)
        .await
        .to()
}

#[tauri::command]
pub async fn get_solve_progress(request_sender: RequestSender<'_>) -> TlcResult<Progress> {
    request::get_solve_progress(&request_sender).await.to()
}

#[tauri::command]
pub async fn get_nu(
    edge_truncation: Option<(f64, f64)>,
    request_sender: RequestSender<'_>,
) -> TlcResult<NuView> {
    request::get_nu(edge_truncation, &request_sender).await.to()
}

trait IntoTlcResult<T> {
    fn to(self) -> TlcResult<T>;
}

impl<T: Serialize> IntoTlcResult<T> for Result<T> {
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())
    }
}
