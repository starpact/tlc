use std::{path::PathBuf, sync::Mutex};

use ndarray::{ArcArray2, Array2};
use salsa::ParallelDatabase;
use tauri::async_runtime::spawn_blocking;
use tlc_core::{
    Database, FilterMethod, InterpMethod, IterMethod, NuData, PhysicalParam, Thermocouple,
};

type TlcResult<T> = Result<T, String>;

type Db<'a> = tauri::State<'a, Mutex<Database>>;

#[tauri::command]
pub fn get_name(db: Db<'_>) -> TlcResult<String> {
    Ok(db
        .lock()
        .unwrap()
        .get_name()
        .ok_or("name unset".to_owned())?
        .to_owned())
}

#[tauri::command]
pub fn set_name(name: String, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_name(name)
}

#[tauri::command]
pub fn get_save_root_dir(db: Db<'_>) -> TlcResult<PathBuf> {
    Ok(db
        .lock()
        .unwrap()
        .get_save_root_dir()
        .ok_or("save root dir unset")?
        .to_owned())
}

#[tauri::command]
pub fn set_save_root_dir(save_root_dir: PathBuf, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_save_root_dir(save_root_dir)
}

#[tauri::command]
pub fn get_video_path(db: Db<'_>) -> TlcResult<PathBuf> {
    Ok(db
        .lock()
        .unwrap()
        .get_video_path()
        .ok_or("video path unset".to_owned())?
        .to_owned())
}

#[tauri::command]
pub fn set_video_path(video_path: PathBuf, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_video_path(video_path)
}

#[tauri::command]
pub async fn get_video_nframes(db: Db<'_>) -> TlcResult<usize> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.get_video_nframes())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_video_frame_rate(db: Db<'_>) -> TlcResult<usize> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.get_video_frame_rate())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_video_shape(db: Db<'_>) -> TlcResult<(u32, u32)> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.get_video_shape())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn decode_frame_base64(frame_index: usize, db: Db<'_>) -> TlcResult<String> {
    let db = db.lock().unwrap().snapshot();
    tlc_core::decode_frame_base64(db, frame_index).await
}

#[tauri::command]
pub fn get_daq_path(db: Db<'_>) -> TlcResult<PathBuf> {
    Ok(db
        .lock()
        .unwrap()
        .get_daq_path()
        .ok_or("daq path unset".to_owned())?
        .to_owned())
}

#[tauri::command]
pub fn set_daq_path(daq_path: PathBuf, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_daq_path(daq_path)
}

#[tauri::command]
pub async fn get_daq_data(db: Db<'_>) -> TlcResult<ArcArray2<f64>> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.get_daq_data())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    db: Db<'_>,
) -> TlcResult<()> {
    db.lock()
        .unwrap()
        .synchronize_video_and_daq(start_frame, start_row)
}

#[tauri::command]
pub fn get_start_frame(db: Db<'_>) -> TlcResult<usize> {
    db.lock().unwrap().get_start_frame()
}

#[tauri::command]
pub fn set_start_frame(start_frame: usize, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_start_frame(start_frame)
}

#[tauri::command]
pub fn get_start_row(db: Db<'_>) -> TlcResult<usize> {
    db.lock().unwrap().get_start_row()
}

#[tauri::command]
pub fn set_start_row(start_row: usize, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_start_row(start_row)
}

#[tauri::command]
pub fn get_area(db: Db<'_>) -> TlcResult<(u32, u32, u32, u32)> {
    db.lock().unwrap().get_area().ok_or("area unset".to_owned())
}

#[tauri::command]
pub fn set_area(area: (u32, u32, u32, u32), db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_area(area)
}

#[tauri::command]
pub fn get_filter_method(db: Db<'_>) -> TlcResult<FilterMethod> {
    db.lock()
        .unwrap()
        .get_filter_method()
        .ok_or("filter method unset".to_owned())
}

#[tauri::command]
pub fn set_filter_method(filter_method: FilterMethod, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_filter_method(filter_method)
}

#[tauri::command]
pub async fn filter_point(point: (usize, usize), db: Db<'_>) -> TlcResult<Vec<u8>> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.filter_point(point))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_thermocouples(db: Db<'_>) -> TlcResult<Vec<Thermocouple>> {
    Ok(db
        .lock()
        .unwrap()
        .get_thermocouples()
        .ok_or("thermocouples unset".to_owned())?
        .to_vec())
}

#[tauri::command]
pub fn set_thermocouples(thermocouples: Vec<Thermocouple>, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_thermocouples(thermocouples)
}

#[tauri::command]
pub fn get_interp_method(db: Db<'_>) -> TlcResult<InterpMethod> {
    db.lock()
        .unwrap()
        .get_interp_method()
        .ok_or("interp method unset".to_owned())
}

#[tauri::command]
pub fn set_interp_method(interp_method: InterpMethod, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_interp_method(interp_method)
}

#[tauri::command]
pub async fn interp_frame(frame_index: usize, db: Db<'_>) -> TlcResult<Array2<f64>> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.interp_frame(frame_index))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_iter_method(db: Db<'_>) -> TlcResult<IterMethod> {
    db.lock()
        .unwrap()
        .get_iter_method()
        .ok_or("iter method unset".to_string())
}

#[tauri::command]
pub fn set_iter_method(iter_method: IterMethod, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_iter_method(iter_method)
}

#[tauri::command]
pub fn get_physical_param(db: Db<'_>) -> TlcResult<PhysicalParam> {
    db.lock()
        .unwrap()
        .get_physical_param()
        .ok_or("physical param unset".to_owned())
}

#[tauri::command]
pub fn set_physical_param(physical_param: PhysicalParam, db: Db<'_>) -> TlcResult<()> {
    db.lock().unwrap().set_physical_param(physical_param)
}

#[tauri::command]
pub async fn get_nu_data(trunc: Option<(f64, f64)>, db: Db<'_>) -> TlcResult<NuData> {
    let db = db.lock().unwrap().snapshot();
    spawn_blocking(move || db.get_nu_data(trunc))
        .await
        .map_err(|e| e.to_string())?
}
