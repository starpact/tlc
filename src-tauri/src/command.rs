use std::path::Path;

use anyhow::Result;
use ndarray::ArcArray2;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::{daq::DaqMetadata, state::GlobalState, video::VideoMetadata};

type State<'a> = tauri::State<'a, RwLock<GlobalState>>;

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
pub async fn set_video_path(video_path: &Path, state: State<'_>) -> TlcResult<VideoMetadata> {
    state.write().await.set_video_path(video_path).await.to()
}

#[tauri::command]
pub async fn set_daq_path(daq_path: &Path, state: State<'_>) -> TlcResult<DaqMetadata> {
    state.write().await.set_daq_path(daq_path).await.to()
}

#[tauri::command]
pub async fn read_frame(frame_index: usize, state: State<'_>) -> TlcResult<String> {
    state.read().await.read_single_frame(frame_index).await.to()
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
pub async fn set_start_frame(start_frame: usize, state: State<'_>) -> TlcResult<()> {
    state.write().await.set_start_frame(start_frame).to()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, state: State<'_>) -> TlcResult<()> {
    state.write().await.set_start_row(start_row).to()
}
