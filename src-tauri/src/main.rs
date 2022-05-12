#![feature(test)]
#![feature(array_windows)]
#![feature(once_cell)]

mod command;
mod config;
mod daq;
mod filter;
mod interpolation;
mod solve;
mod state;
mod util;
mod video;

use ffmpeg_next as ffmpeg;
use tokio::sync::RwLock;
use tracing::error;

use command::*;
use state::*;

#[tokio::main]
async fn main() {
    util::log::init();

    ffmpeg::init().expect("Failed to init ffmpeg");

    let tlc_state = TlcState::new().await;

    tauri::Builder::default()
        .manage(RwLock::new(tlc_state))
        .invoke_handler(tauri::generate_handler![
            set_video_path,
            read_frame,
            set_start_frame,
            set_start_row
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("Uncaught error: {}", e));
}
