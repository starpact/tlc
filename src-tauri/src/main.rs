#![feature(test, array_windows)]

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
use tracing::{error, Level};

use command::*;
use state::*;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .pretty()
        .init();

    ffmpeg::init().expect("Failed to init ffmpeg");

    let mut tlc_state = TlcState::new();

    tauri::Builder::default()
        .manage(RwLock::new(tlc_state))
        .invoke_handler(tauri::generate_handler![set_video_path, read_frame])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("Uncaught error: {}", e));
}
