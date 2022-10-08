use std::path::Path;

use anyhow::Result;
use ndarray::ArcArray2;
use serde::Serialize;

use crate::{
    daq::DaqMetadata,
    setting::StartIndex,
    solve::IterationMethod,
    state::GlobalState,
    util::progress_bar::Progress,
    video::{FilterMethod, VideoMetadata},
};

type State<'a> = tauri::State<'a, GlobalState>;

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
pub async fn get_save_root_dir(state: State<'_>) -> TlcResult<String> {
    state.get_save_root_dir().await.to()
}

#[tauri::command]
pub async fn set_save_root_dir(save_root_dir: String, state: State<'_>) -> TlcResult<()> {
    state.set_save_root_dir(save_root_dir).await.to()
}

#[tauri::command]
pub async fn get_video_metadata(state: State<'_>) -> TlcResult<VideoMetadata> {
    state.get_video_metadata().await.to()
}

#[tauri::command]
pub async fn set_video_path(video_path: &Path, state: State<'_>) -> TlcResult<()> {
    state.set_video_path(&video_path).await.to()
}

#[tauri::command]
pub async fn get_daq_metadata(state: State<'_>) -> TlcResult<DaqMetadata> {
    state.get_daq_metadata().await.to()
}

#[tauri::command]
pub async fn set_daq_path(daq_path: String, state: State<'_>) -> TlcResult<()> {
    state.set_daq_path(daq_path).await.to()
}

#[tauri::command]
pub async fn read_single_frame_base64(frame_index: usize, state: State<'_>) -> TlcResult<String> {
    state.read_single_frame_base64(frame_index).await.to()
}

#[tauri::command]
pub async fn get_daq_data(state: State<'_>) -> TlcResult<ArcArray2<f64>> {
    state.get_daq_data().await.to()
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
    state.spawn_build_green2().to()
}

#[tauri::command]
pub async fn get_build_green2_progress(state: State<'_>) -> TlcResult<Progress> {
    Ok(state.get_build_green2_progress())
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
pub async fn filter(state: State<'_>) -> TlcResult<()> {
    state.filter().await.to()
}

#[tauri::command]
pub async fn get_filter_green2_progress(state: State<'_>) -> TlcResult<Progress> {
    Ok(state.get_filter_green2_progress())
}

#[tauri::command]
pub async fn get_iteration_method(state: State<'_>) -> TlcResult<IterationMethod> {
    state.get_iteration_method().await.to()
}

#[tauri::command]
pub fn set_interpolation_method() -> TlcResult<()> {
    todo!()
}

#[tauri::command]
pub fn interpolate_single_point() -> TlcResult<()> {
    todo!()
}

#[tauri::command]
pub fn interpolate() -> TlcResult<()> {
    todo!()
}

#[tauri::command]
pub async fn set_iteration_method(
    state: State<'_>,
    iteration_method: IterationMethod,
) -> TlcResult<()> {
    state.set_iteration_method(iteration_method).await.to()
}

#[tauri::command]
pub async fn solve(state: State<'_>) -> TlcResult<()> {
    state.solve().await.to()
}
