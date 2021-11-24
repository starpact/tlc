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
            //
            load_config,
            //
            get_save_root_dir,
            set_save_root_dir,
            //
            get_frame,
            get_video_meta,
            set_video_path,
            //
            get_daq,
            get_daq_meta,
            set_daq_path,
            //
            set_start_frame,
            set_start_row,
            set_area,
            //
            set_filter_method,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("uncaught error: {}", e));
}
