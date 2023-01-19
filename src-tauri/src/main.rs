#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod command;

use std::sync::Mutex;

use command::*;

fn main() {
    tlc_core::init();
    let db = Mutex::new(tlc_core::Database::default());

    tauri::Builder::default()
        .manage(db)
        .invoke_handler(tauri::generate_handler![
            get_name,
            set_name,
            get_save_root_dir,
            set_save_root_dir,
            get_video_path,
            get_video_nframes,
            get_video_frame_rate,
            get_video_shape,
            set_video_path,
            get_daq_path,
            set_daq_path,
            decode_frame_base64,
            get_daq_data,
            synchronize_video_and_daq,
            get_start_frame,
            set_start_frame,
            get_start_row,
            set_start_row,
            get_area,
            set_area,
            get_thermocouples,
            set_thermocouples,
            get_filter_method,
            set_filter_method,
            filter_point,
            get_interp_method,
            set_interp_method,
            interp_frame,
            get_iter_method,
            set_iter_method,
            get_physical_param,
            set_physical_param,
            get_nu_data,
        ])
        .run(tauri::generate_context!())
        .unwrap()
}
