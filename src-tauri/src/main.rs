#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]
#![feature(let_chains)]

mod command;
mod daq;
mod db;
mod error;
mod plot;
mod setting;
mod solve;
mod state;
mod util;
mod video;

use ffmpeg_next as ffmpeg;
use setting::SettingStorageSqlite;
use tracing::error;

use command::*;
use state::*;

fn main() {
    util::log::init();

    ffmpeg::init().expect("Failed to init ffmpeg");

    tauri::Builder::default()
        .manage(GlobalState::new(SettingStorageSqlite::new()))
        .invoke_handler(tauri::generate_handler![
            //
            // First decide where to store the data.
            get_save_root_dir,
            set_save_root_dir,
            //
            // Get `video_metadata` and load video.
            get_video_metadata,
            set_video_path,
            //
            // Get `daq_metadata` and load daq.
            get_daq_metadata,
            set_daq_path,
            //
            // Drag the progress bar to "light up" point.
            // The frame index is only maintained by frontend.
            read_single_frame_base64,
            //
            // Choose the "voltage change" point.
            // The row index is only maintained by frontend.
            get_daq_data,
            //
            // Now we can synchronize video and daq.
            synchronize_video_and_daq,
            get_start_index,
            //
            // Adjust the start frame.
            // Start row will change correspondingly.
            set_start_frame,
            //
            // Adjust the start row.
            // Start frame will change correspondingly.
            set_start_row,
            //
            // Choose the area that we want to calculate.
            get_area,
            set_area,
            //
            // Mark locations of thermocouples.
            set_thermocouples,
            //
            // We can build `green2` now.
            build_green2,
            get_build_green2_progress,
            //
            // Filter.
            filter_method,
            set_filter_method,
            // See the effect of different filter methods.
            filter_single_point,
            // Filter all.
            filter_green2,
            get_filter_green2_progress,
            //
            // Interpolate.
            set_interpolation_method,
            // See the effect of different interpolation methods.
            interpolate_single_point,
            // Interpolate all.
            interpolate,
            //
            get_iteration_method,
            set_iteration_method,
            //
            // Solve.
            set_iteration_method,
            solve,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("Uncaught error: {}", e));
}
