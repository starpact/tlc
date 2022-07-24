#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]

mod command;
mod config;
mod daq;
mod plot;
mod solve;
mod state;
mod util;
mod video;

use ffmpeg_next as ffmpeg;
use tauri::async_runtime;
use tokio::sync::RwLock;
use tracing::error;

use command::*;
use state::*;

fn main() {
    util::log::init();

    ffmpeg::init().expect("Failed to init ffmpeg");

    let global_state: &'static _ = Box::leak(Box::new(RwLock::new(GlobalState::new())));

    async_runtime::spawn(async {
        let mut global_state = global_state.write().await;
        let _ = global_state.load_video().await;
        let _ = global_state.load_daq().await;
    });

    tauri::Builder::default()
        .manage(global_state)
        .invoke_handler(tauri::generate_handler![
            load_config,
            get_video_metadata,
            set_video_path,
            get_daq_metadata,
            set_daq_path,
            read_single_frame_base64,
            get_daq_data,
            synchronize_video_and_daq,
            get_start_frame,
            set_start_frame,
            get_start_row,
            set_start_row,
            build_green2,
            get_build_green2_progress,
            set_filter_method,
            filter,
            filter_single_point,
            get_filter_green2_progress,
            solve,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("Uncaught error: {}", e));
}
