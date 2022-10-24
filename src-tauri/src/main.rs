#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]

mod commands;
mod daq;
mod old_state;
mod post_processing;
mod request;
mod setting;
mod solve;
mod state;
mod util;
mod video;

use crossbeam::channel::bounded;
use setting::SqliteSettingStorage;
use tracing::error;

use commands::*;
use old_state::*;

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

fn main() {
    util::log::init();

    ffmpeg::init().expect("Failed to init ffmpeg");

    let (request_sender, request_receiver) = bounded(3);
    std::thread::spawn(move || state::main_loop(request_receiver));

    let setting_storage = SqliteSettingStorage::new(SQLITE_FILEPATH);
    let global_state = GlobalState::new(setting_storage);

    tauri::Builder::default()
        .manage(global_state)
        .manage(request_sender)
        .invoke_handler(tauri::generate_handler![
            create_setting,
            switch_setting,
            get_save_root_dir,
            set_save_root_dir,
            get_video_meta,
            set_video_path,
            get_daq_meta,
            set_daq_path,
            read_single_frame_base64,
            get_daq_raw,
            synchronize_video_and_daq,
            get_start_index,
            set_start_frame,
            set_start_row,
            get_area,
            set_area,
            set_thermocouples,
            build_green2,
            get_build_green2_progress,
            get_filter_method,
            set_filter_method,
            filter_single_point,
            detect_peak,
            get_detect_peak_progress,
            get_interpolation_method,
            set_interp_method,
            interp_single_frame,
            get_iteration_method,
            set_iteration_method,
            set_gmax_temperature,
            set_solid_thermal_conductivity,
            set_solid_thermal_diffusivity,
            set_characteristic_length,
            set_air_thermal_conductivity,
            get_nu,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("error while running application: {e}"));
}
