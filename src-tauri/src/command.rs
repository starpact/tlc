use std::path::Path;

use anyhow::Result;
use ndarray::ArcArray2;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::{
    daq::DaqMetadata,
    state::GlobalState,
    util::progress_bar::Progress,
    video::{FilterMethod, VideoMetadata},
};

type State<'a> = tauri::State<'a, &'static RwLock<GlobalState>>;

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
pub async fn load_config(config_path: &Path, state: State<'_>) -> TlcResult<()> {
    state.write().await.load_config(config_path).await.to()
}

#[tauri::command]
pub async fn get_video_metadata(state: State<'_>) -> TlcResult<VideoMetadata> {
    state.read().await.get_video_metadata().to()
}

#[tauri::command]
pub async fn set_video_path(video_path: &Path, state: State<'_>) -> TlcResult<VideoMetadata> {
    state.write().await.set_video_path(video_path).await.to()
}

#[tauri::command]
pub async fn get_daq_metadata(state: State<'_>) -> TlcResult<DaqMetadata> {
    state.read().await.get_daq_metadata().to()
}

#[tauri::command]
pub async fn set_daq_path(daq_path: &Path, state: State<'_>) -> TlcResult<DaqMetadata> {
    state.write().await.set_daq_path(daq_path).await.to()
}

#[tauri::command]
pub async fn read_single_frame_base64(frame_index: usize, state: State<'_>) -> TlcResult<String> {
    state
        .read()
        .await
        .read_single_frame_base64(frame_index)
        .await
        .to()
}

#[tauri::command]
pub async fn get_daq_data(state: State<'_>) -> TlcResult<ArcArray2<f64>> {
    state.read().await.get_daq_data().to()
}

#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    state: State<'_>,
) -> TlcResult<()> {
    state
        .write()
        .await
        .synchronize_video_and_daq(start_frame, start_row)
        .to()
}

#[tauri::command]
pub async fn get_start_frame(state: State<'_>) -> TlcResult<usize> {
    state.read().await.get_start_frame().to()
}

#[tauri::command]
pub async fn set_start_frame(start_frame: usize, state: State<'_>) -> TlcResult<()> {
    state.write().await.set_start_frame(start_frame).to()
}

#[tauri::command]
pub async fn get_start_row(state: State<'_>) -> TlcResult<usize> {
    state.read().await.get_start_row().to()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, state: State<'_>) -> TlcResult<()> {
    state.write().await.set_start_row(start_row).to()
}

#[tauri::command]
pub async fn build_green2(state: State<'_>) -> TlcResult<()> {
    state.read().await.spawn_build_green2().to()
}

#[tauri::command]
pub async fn get_build_green2_progress(state: State<'_>) -> TlcResult<Progress> {
    Ok(state.read().await.get_build_green2_progress())
}

#[tauri::command]
pub async fn set_filter_method(filter_method: FilterMethod, state: State<'_>) -> TlcResult<()> {
    state.write().await.set_filter_method(filter_method).to()
}

#[tauri::command]
pub async fn filter(state: State<'_>) -> TlcResult<()> {
    state.write().await.filter().to()
}

#[tauri::command]
pub async fn filter_single_point(position: (usize, usize), state: State<'_>) -> TlcResult<Vec<u8>> {
    state.read().await.filter_single_point(position).await.to()
}

#[tauri::command]
pub async fn get_filter_green2_progress(state: State<'_>) -> TlcResult<Progress> {
    Ok(state.read().await.get_filter_green2_progress())
}

#[tauri::command]
pub async fn solve(state: State<'_>) -> TlcResult<()> {
    state.write().await.solve().await.to()
}
