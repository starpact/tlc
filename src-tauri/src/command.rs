use std::path::PathBuf;
use tauri::State;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, serde::Deserialize)]
pub enum Command {
    ReloadConfig(String),
    GetSaveInfo,
    SetVideoPath(PathBuf),
    SaveConfigToPath(PathBuf),
}

pub type Response = Result<String, String>;

#[tauri::command]
pub async fn get_save_info(
    state: State<'_, mpsc::Sender<(Command, oneshot::Sender<Response>)>>,
) -> Response {
    handle(Command::GetSaveInfo, state)?
        .await
        .map_err(|e| e.to_string())?
}

fn handle(
    cmd: Command,
    state: State<'_, mpsc::Sender<(Command, oneshot::Sender<Response>)>>,
) -> Result<oneshot::Receiver<Response>, String> {
    let (tx, rx) = oneshot::channel();
    state.try_send((cmd, tx)).map_err(|e| e.to_string())?;

    Ok(rx)
}
