mod config;
mod data;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
use tracing::error;

use crate::command::{Command, Response};

pub struct TLCHandler {
    /// All the configs that determine how we calculate and store data.
    cfg: TLCConfig,
    data: TLCData,
}

#[derive(Debug, Serialize, Deserialize)]
struct TLCConfig {
    #[serde(default)]
    storage: TLCStorage,
}

/// TLCStorage manages information that is needed when work with the file system.
/// 1. Where to read data
/// 2. Where to save data
#[derive(Debug, Serialize, Deserialize, Default)]
struct TLCStorage {
    /// Path of TLC video file.
    ///
    video_path: Option<PathBuf>,
    /// Path of TLC data acquisition file.
    daq_path: Option<PathBuf>,

    /// Directory where you save your data.
    /// * case_name: as the video_path varies from case to case, we can use file stem
    /// of it as the case_name
    /// * config_path: {root_dir}/config/{case_name}.toml
    /// * nu_path: {root_dir}/nu/{case_name}.csv
    /// * plot_path: {root_dir}/plot/{case_name}.png
    save_root_dir: Option<PathBuf>,
}

struct TLCData {
    packets: Vec<u8>,
}

pub async fn serve(mut rx: mpsc::Receiver<(Command, oneshot::Sender<Response>)>) {
    let mut tlc_handler = TLCHandler::default();

    while let Some((cmd, tx)) = rx.recv().await {
        if tlc_handler.is_err() {
            if let Command::ReloadConfig(ref path) = cmd {
                tlc_handler = TLCHandler::from_path(path);
            }
        }

        match tlc_handler {
            Ok(ref mut tlc_handler) => tx.send(tlc_handler.handle(cmd)),
            Err(ref e) => tx.send(Err(e.to_string())),
        }
        .unwrap_or_else(|e| error!("Failed to send back result: {:?}", e));
    }
}

impl TLCHandler {
    fn default() -> Result<Self> {
        Self::from_path(TLCConfig::DEFAULT_CONFIG_PATH)
    }

    fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = TLCConfig::from_path(path)?;
        let data = TLCData {
            packets: Vec::new(),
        };

        Ok(TLCHandler { data, cfg: config })
    }

    fn handle(&mut self, cmd: Command) -> Response {
        use Command::*;
        match cmd {
            GetSaveInfo => self.cfg.storage.get_save_info().to(),
            SetVideoPath(path) => self.set_video_path(path).to(),
            ReloadConfig(path) => self.reload_config(path).to(),
            SaveConfigToPath(path) => self.cfg.save_to_path(path).to(),
        }
    }
}

impl TLCConfig {
    const DEFAULT_CONFIG_PATH: &'static str = "./config/default.toml";

    fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let buf = std::fs::read(path)?;
        Ok(toml::from_slice::<TLCConfig>(&buf)?)
    }
}

trait IntoResponse {
    fn to(self) -> Response;
}

impl<T: Serialize> IntoResponse for Result<T> {
    fn to(self) -> Response {
        match self {
            Ok(t) => Ok(serde_json::to_string(&t).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }
}
