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

    let global_state: &'static _ = Box::leak(Box::new(RwLock::new(GlobalState::new())));
    tokio::spawn(async {
        global_state.write().await.try_load_data().await;
    });

    tauri::Builder::default()
        .manage(RwLock::new(global_state))
        .invoke_handler(tauri::generate_handler![
            set_video_path,
            set_daq_path,
            read_frame,
            synchronize_video_and_daq,
            set_start_frame,
            set_start_row
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("Uncaught error: {}", e));
}
