use std::fmt;
use std::path::Path;

use tauri::State;
use tokio::sync::RwLock;

use crate::config::{SaveInfo, TLCConfig};
use crate::data::TLCData;
use serde::Serialize;

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
pub async fn get_save_info(
    config: State<'_, RwLock<anyhow::Result<TLCConfig>>>,
) -> TLCResult<SaveInfo> {
    let config = config.read().await;
    let config = config.as_ref().to()?;

    config.get_save_info().to()
}

#[tauri::command]
pub async fn load_config<'a>(
    path: &'a Path,
    config: State<'_, RwLock<anyhow::Result<TLCConfig>>>,
    data: State<'_, RwLock<TLCData>>,
) -> TLCResult<()> {
    *config.write().await = Ok(TLCConfig::from_path(path).await.to()?);

    // If the config is reloaded, all data are invalidated.
    *data.write().await = TLCData::default();

    Ok(())
}

#[tauri::command]
pub async fn set_video_path<'a>(
    path: &'a Path,
    config: State<'_, RwLock<anyhow::Result<TLCConfig>>>,
    data: State<'_, RwLock<TLCData>>,
) -> TLCResult<()> {
    config
        .write()
        .await
        .as_mut()
        .to()?
        .set_video_path(path)
        .to()?;

    data.read().await.read_video(path);

    Ok(())
}

#[tauri::command]
pub async fn get_frame(frame_index: usize, data: State<'_, RwLock<TLCData>>) -> TLCResult<usize> {
    data.read().await.get_frame(frame_index).await.to()
}
