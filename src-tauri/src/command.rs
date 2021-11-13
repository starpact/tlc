use std::fmt;
use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::handler::SaveInfo;
use crate::handler::TLCHandler;

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
pub async fn load_config(path: &Path, state: State<'_, TLCHandler>) -> TLCResult<()> {
    state.load_config(path).await.to()
}

#[tauri::command]
pub async fn get_save_info(state: State<'_, TLCHandler>) -> TLCResult<SaveInfo> {
    state.get_save_info().await.to()
}

#[tauri::command]
pub async fn set_video_path(path: &Path, state: State<'_, TLCHandler>) -> TLCResult<()> {
    state.set_video_path(path).await.to()
}

#[tauri::command]
pub async fn set_daq_path(path: &Path, state: State<'_, TLCHandler>) -> TLCResult<()> {
    state.set_daq_path(path).await.to()
}

#[tauri::command]
pub async fn get_frame(frame_index: usize, state: State<'_, TLCHandler>) -> TLCResult<String> {
    state.get_frame(frame_index).await.to()
}

#[tauri::command]
pub async fn set_region(region: [u32; 4], state: State<'_, TLCHandler>) -> TLCResult<()> {
    state.set_region(region).await.to()
}

#[tauri::command]
pub async fn set_start_frame(start_frame: usize, state: State<'_, TLCHandler>) -> TLCResult<()> {
    state.set_start_frame(start_frame).await.to()
}
