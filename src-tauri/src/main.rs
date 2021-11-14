mod command;
mod controller;
mod util;

use ffmpeg_next as ffmpeg;

use tracing::{error, Level};

use crate::controller::TLCController;
use command::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    ffmpeg::init().expect("failed to init ffmpeg");

    let tlc_controller = TLCController::new().await;

    tauri::Builder::default()
        .manage(tlc_controller)
        .invoke_handler(tauri::generate_handler![
            load_config,
            get_save_info,
            set_video_path,
            set_daq_path,
            get_frame,
            set_start_frame,
            set_region,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("uncaught error: {}", e));
}
