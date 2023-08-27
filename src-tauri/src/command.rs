use std::path::PathBuf;

use ndarray::{ArcArray2, Array2};
use tlc_core::{FilterMethod, InterpMethod, IterMethod, NuData, PhysicalParam, Thermocouple};

type TlcResult<T> = Result<T, String>;

type State<'a> = tauri::State<'a, tlc_core::State>;

#[tauri::command]
pub async fn get_name(state: State<'_>) -> TlcResult<String> {
    state.get_name().to()
}

#[tauri::command]
pub async fn set_name(name: String, state: State<'_>) -> TlcResult<()> {
    state.set_name(name).to()
}

#[tauri::command]
pub async fn get_save_root_dir(state: State<'_>) -> TlcResult<PathBuf> {
    Ok(state.get_save_root_dir().to()?.to_owned())
}

#[tauri::command]
pub async fn set_save_root_dir(save_root_dir: PathBuf, state: State<'_>) -> TlcResult<()> {
    state.set_save_root_dir(save_root_dir).to()
}

#[tauri::command]
pub async fn get_video_path(state: State<'_>) -> TlcResult<PathBuf> {
    Ok(state.get_video_path().to()?.to_owned())
}

#[tauri::command]
pub async fn set_video_path(video_path: PathBuf, state: State<'_>) -> TlcResult<()> {
    state.set_video_path(video_path).to()
}

#[tauri::command]
pub async fn get_video_nframes(state: State<'_>) -> TlcResult<usize> {
    state.get_video_nframes().to()
}

#[tauri::command]
pub async fn get_video_frame_rate(state: State<'_>) -> TlcResult<usize> {
    state.get_video_frame_rate().to()
}

#[tauri::command]
pub async fn get_video_shape(state: State<'_>) -> TlcResult<(u32, u32)> {
    state.get_video_shape().to()
}

#[tauri::command]
pub async fn decode_frame_base64(frame_index: usize, state: State<'_>) -> TlcResult<String> {
    state.decode_frame_base64(frame_index).await.to()
}

#[tauri::command]
pub async fn get_daq_path(state: State<'_>) -> TlcResult<PathBuf> {
    state.get_daq_path().to()
}

#[tauri::command]
pub async fn set_daq_path(daq_path: PathBuf, state: State<'_>) -> TlcResult<()> {
    state.set_daq_path(daq_path).to()
}

#[tauri::command]
pub async fn get_daq_data(state: State<'_>) -> TlcResult<ArcArray2<f64>> {
    state.get_daq_data().to()
}

#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    state: State<'_>,
) -> TlcResult<()> {
    state.synchronize_video_and_daq(start_frame, start_row).to()
}

#[tauri::command]
pub async fn get_start_frame(state: State<'_>) -> TlcResult<usize> {
    state.get_start_frame().to()
}

#[tauri::command]
pub async fn set_start_frame(start_frame: usize, state: State<'_>) -> TlcResult<()> {
    state.set_start_frame(start_frame).to()
}

#[tauri::command]
pub async fn get_start_row(state: State<'_>) -> TlcResult<usize> {
    state.get_start_row().to()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, state: State<'_>) -> TlcResult<()> {
    state.set_start_row(start_row).to()
}

#[tauri::command]
pub async fn get_area(state: State<'_>) -> TlcResult<(u32, u32, u32, u32)> {
    state.get_area().to()
}

#[tauri::command]
pub async fn set_area(area: (u32, u32, u32, u32), state: State<'_>) -> TlcResult<()> {
    state.set_area(area).to()
}

#[tauri::command]
pub async fn get_filter_method(state: State<'_>) -> TlcResult<FilterMethod> {
    state.get_filter_method().to()
}

#[tauri::command]
pub async fn set_filter_method(filter_method: FilterMethod, state: State<'_>) -> TlcResult<()> {
    state.set_filter_method(filter_method).to()
}

#[tauri::command]
pub async fn filter_point(point: (usize, usize), state: State<'_>) -> TlcResult<Vec<u8>> {
    state.filter_point(point).to()
}

#[tauri::command]
pub async fn get_thermocouples(state: State<'_>) -> TlcResult<Vec<Thermocouple>> {
    Ok(state.get_thermocouples().to()?.to_vec())
}

#[tauri::command]
pub async fn set_thermocouples(
    thermocouples: Box<[Thermocouple]>,
    state: State<'_>,
) -> TlcResult<()> {
    state.set_thermocouples(thermocouples).to()
}

#[tauri::command]
pub async fn get_interp_method(state: State<'_>) -> TlcResult<InterpMethod> {
    state.get_interp_method().to()
}

#[tauri::command]
pub async fn set_interp_method(interp_method: InterpMethod, state: State<'_>) -> TlcResult<()> {
    state.set_interp_method(interp_method).to()
}

#[tauri::command]
pub async fn interp_frame(frame_index: usize, state: State<'_>) -> TlcResult<Array2<f64>> {
    state.interp_frame(frame_index).to()
}

#[tauri::command]
pub async fn get_iter_method(state: State<'_>) -> TlcResult<IterMethod> {
    state.get_iter_method().to()
}

#[tauri::command]
pub async fn set_iter_method(iter_method: IterMethod, state: State<'_>) -> TlcResult<()> {
    state.set_iter_method(iter_method).to()
}

#[tauri::command]
pub async fn get_physical_param(state: State<'_>) -> TlcResult<PhysicalParam> {
    state.get_physical_param().to()
}

#[tauri::command]
pub async fn set_physical_param(physical_param: PhysicalParam, state: State<'_>) -> TlcResult<()> {
    state.set_physical_param(physical_param).to()
}

#[tauri::command]
pub async fn get_nu_data(state: State<'_>) -> TlcResult<NuData> {
    state.get_nu_data().to()
}

#[tauri::command]
pub async fn get_nu_plot(trunc: Option<(f64, f64)>, state: State<'_>) -> TlcResult<String> {
    state.get_nu_plot(trunc).to()
}

#[tauri::command]
pub async fn save_data(state: State<'_>) -> TlcResult<()> {
    state.save_data().to()
}

trait IntoTlcResult<T> {
    fn to(self) -> TlcResult<T>;
}

impl<T, E> IntoTlcResult<T> for Result<T, E>
where
    E: ToString,
{
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())
    }
}
