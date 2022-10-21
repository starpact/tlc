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
mod post;
mod setting;
mod solve;
mod state;
mod util;
mod video;

use setting::SqliteSettingStorage;
use tracing::error;

use command::*;
use state::*;

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

fn main() {
    util::log::init();

    ffmpeg::init().expect("Failed to init ffmpeg");

    let setting_storage = SqliteSettingStorage::new(SQLITE_FILEPATH);
    let global_state = GlobalState::new(setting_storage);

    tauri::Builder::default()
        .manage(global_state)
        .invoke_handler(tauri::generate_handler![
            create_setting,
            switch_setting,
            get_save_root_dir,
            set_save_root_dir,
            get_video_metadata,
            set_video_path,
            get_daq_metadata,
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
            set_interpolation_method,
            interpolate,
            interpolate_single_frame,
            get_iteration_method,
            set_iteration_method,
            set_gmax_temperature,
            set_solid_thermal_conductivity,
            set_solid_thermal_diffusivity,
            set_characteristic_length,
            set_air_thermal_conductivity,
            solve,
            get_nu,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("error while running application: {e}"));
}
