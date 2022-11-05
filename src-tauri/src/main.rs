#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]

mod command;
mod daq;
mod post_processing;
mod request;
mod setting;
mod solve;
mod state;

use setting::new_db;
use state::GlobalState;
use tracing::error;

use command::*;

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

fn main() {
    tlc_util::log::init();
    tlc_video::init();

    let global_state = GlobalState::new(new_db(SQLITE_FILEPATH));

    tauri::Builder::default()
        .manage(global_state)
        .invoke_handler(tauri::generate_handler![
            create_setting,
            switch_setting,
            delete_setting,
            get_name,
            set_name,
            get_save_root_dir,
            set_save_root_dir,
            get_video_path,
            get_video_meta,
            set_video_path,
            get_read_video_progress,
            get_daq_path,
            set_daq_path,
            get_daq_meta,
            decode_frame_base64,
            get_daq_raw,
            synchronize_video_and_daq,
            get_start_index,
            set_start_frame,
            set_start_row,
            get_area,
            set_area,
            get_thermocouples,
            set_thermocouples,
            get_build_green2_progress,
            get_filter_method,
            set_filter_method,
            filter_point,
            get_detect_peak_progress,
            get_detect_peak_progress,
            get_interp_method,
            set_interp_method,
            interp_frame,
            get_iteration_method,
            set_iteration_method,
            get_physical_param,
            set_gmax_temperature,
            set_solid_thermal_conductivity,
            set_solid_thermal_diffusivity,
            set_characteristic_length,
            set_air_thermal_conductivity,
            get_solve_progress,
            get_nu,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("error while running application: {e}"));
}
