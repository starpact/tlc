use std::fmt;
use std::path::{Path, PathBuf};

use ndarray::Array2;
use serde::Serialize;
use tauri::State;

use crate::controller::{FilterMethod, TLCController};

pub type TLCResult<T> = Result<T, String>;

trait IntoTLCResult<T> {
    fn to(self) -> TLCResult<T>;
}

impl<T: Serialize, E: fmt::Debug> IntoTLCResult<T> for Result<T, E> {
    fn to(self) -> TLCResult<T> {
        self.map_err(|e| format!("{:?}", e))
    }
}

#[tauri::command]
pub async fn load_config(path: &Path, state: State<'_, TLCController>) -> TLCResult<()> {
    state.load_config(path).await.to()
}

#[tauri::command]
pub async fn get_save_root_dir(state: State<'_, TLCController>) -> TLCResult<PathBuf> {
    state.get_save_root_dir().await.to()
}

#[tauri::command]
pub async fn set_save_root_dir(path: &Path, state: State<'_, TLCController>) -> TLCResult<()> {
    state.set_save_root_dir(path).await;
    Ok(())
}

#[tauri::command]
pub async fn set_video_path(path: &Path, state: State<'_, TLCController>) -> TLCResult<()> {
    state.set_video_path(path).await.to()
}

#[tauri::command]
pub async fn get_frame(frame_index: usize, state: State<'_, TLCController>) -> TLCResult<String> {
    state.get_frame(frame_index).await.to()
}

#[tauri::command]
pub async fn set_daq_path(path: &Path, state: State<'_, TLCController>) -> TLCResult<()> {
    state.set_daq_path(path).await.to()
}

#[tauri::command]
pub async fn get_daq(state: State<'_, TLCController>) -> TLCResult<Array2<f64>> {
    Ok(state.get_daq().await)
}

#[tauri::command]
pub async fn set_start_frame(start_frame: usize, state: State<'_, TLCController>) -> TLCResult<()> {
    state.set_start_frame(start_frame).await.to()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, state: State<'_, TLCController>) -> TLCResult<()> {
    state.set_start_row(start_row).await.to()
}

#[tauri::command]
pub async fn set_area(
    area: (u32, u32, u32, u32),
    state: State<'_, TLCController>,
) -> TLCResult<()> {
    state.set_area(area).await.to()
}

#[tauri::command]
pub async fn set_filter_method(
    filter_method: FilterMethod,
    state: State<'_, TLCController>,
) -> TLCResult<()> {
    state.set_filter_method(filter_method).await.to()
}
