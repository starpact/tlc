mod command;
mod config;
mod data;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{error, Level};

use command::*;
use config::TLCConfig;
use data::TLCData;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    let config = RwLock::new(TLCConfig::from_default_path().await);
    let data = RwLock::new(TLCData::default());

    on_setup(&config, &data).await;

    tauri::Builder::default()
        .manage(config)
        .manage(data)
        .invoke_handler(tauri::generate_handler![
            load_config,
            get_save_info,
            set_video_path,
            get_frame,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("uncaught error: {}", e));
}

async fn on_setup(config: &RwLock<Result<TLCConfig>>, data: &RwLock<TLCData>) {
    let cfg = config.read().await;
    let cfg = match cfg.as_ref() {
        Ok(cfg) => cfg,
        Err(_) => return,
    };

    if let Some(video_path) = cfg.get_video_path() {
        data.read().await.read_video(video_path);
    }
}
