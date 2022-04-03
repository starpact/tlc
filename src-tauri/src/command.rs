use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::{state::TlcState, video::VideoMetadata};

type State<'a> = tauri::State<'a, RwLock<TlcState>>;

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
pub async fn read_frame(frame_index: usize, state: State<'_>) -> TlcResult<String> {
    state.read().await.read_frame(frame_index).await.to()
}
