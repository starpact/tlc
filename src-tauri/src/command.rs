use std::path::PathBuf;

use ndarray::{ArcArray2, Array2};
use salsa::ParallelDatabase;
use tauri::async_runtime::{spawn_blocking, Mutex};
use tlc_core::{
    Database, FilterMethod, InterpMethod, IterMethod, NuData, PhysicalParam, Thermocouple,
};

type TlcResult<T> = Result<T, String>;

type Db<'a> = tauri::State<'a, Mutex<Database>>;

#[tauri::command]
pub async fn get_name(db: Db<'_>) -> TlcResult<String> {
    Ok(db.lock().await.get_name()?.to_owned())
}

#[tauri::command]
pub async fn set_name(name: String, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_name(name)
}

#[tauri::command]
pub async fn get_save_root_dir(db: Db<'_>) -> TlcResult<PathBuf> {
    Ok(db.lock().await.get_save_root_dir()?.to_owned())
}

#[tauri::command]
pub async fn set_save_root_dir(save_root_dir: PathBuf, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_save_root_dir(save_root_dir)
}

#[tauri::command]
pub async fn get_video_path(db: Db<'_>) -> TlcResult<PathBuf> {
    Ok(db.lock().await.get_video_path()?.to_owned())
}

#[tauri::command]
pub async fn set_video_path(video_path: PathBuf, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_video_path(video_path)
}

#[tauri::command]
pub async fn get_video_nframes(db: Db<'_>) -> TlcResult<usize> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.get_video_nframes()).await.to()?
}

#[tauri::command]
pub async fn get_video_frame_rate(db: Db<'_>) -> TlcResult<usize> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.get_video_frame_rate())
        .await
        .to()?
}

#[tauri::command]
pub async fn get_video_shape(db: Db<'_>) -> TlcResult<(u32, u32)> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.get_video_shape()).await.to()?
}

#[tauri::command]
pub async fn decode_frame_base64(frame_index: usize, db: Db<'_>) -> TlcResult<String> {
    let db = db.lock().await.snapshot();
    tlc_core::decode_frame_base64(db, frame_index).await
}

#[tauri::command]
pub async fn get_daq_path(db: Db<'_>) -> TlcResult<PathBuf> {
    Ok(db.lock().await.get_daq_path()?.to_owned())
}

#[tauri::command]
pub async fn set_daq_path(daq_path: PathBuf, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_daq_path(daq_path)
}

#[tauri::command]
pub async fn get_daq_data(db: Db<'_>) -> TlcResult<ArcArray2<f64>> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.get_daq_data()).await.to()?
}

#[tauri::command]
pub async fn synchronize_video_and_daq(
    start_frame: usize,
    start_row: usize,
    db: Db<'_>,
) -> TlcResult<()> {
    db.lock()
        .await
        .synchronize_video_and_daq(start_frame, start_row)
}

#[tauri::command]
pub async fn get_start_frame(db: Db<'_>) -> TlcResult<usize> {
    db.lock().await.get_start_frame()
}

#[tauri::command]
pub async fn set_start_frame(start_frame: usize, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_start_frame(start_frame)
}

#[tauri::command]
pub async fn get_start_row(db: Db<'_>) -> TlcResult<usize> {
    db.lock().await.get_start_row()
}

#[tauri::command]
pub async fn set_start_row(start_row: usize, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_start_row(start_row)
}

#[tauri::command]
pub async fn get_area(db: Db<'_>) -> TlcResult<(u32, u32, u32, u32)> {
    db.lock().await.get_area()
}

#[tauri::command]
pub async fn set_area(area: (u32, u32, u32, u32), db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_area(area)
}

#[tauri::command]
pub async fn get_filter_method(db: Db<'_>) -> TlcResult<FilterMethod> {
    db.lock().await.get_filter_method()
}

#[tauri::command]
pub async fn set_filter_method(filter_method: FilterMethod, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_filter_method(filter_method)
}

#[tauri::command]
pub async fn filter_point(point: (usize, usize), db: Db<'_>) -> TlcResult<Vec<u8>> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.filter_point(point)).await.to()?
}

#[tauri::command]
pub async fn get_thermocouples(db: Db<'_>) -> TlcResult<Vec<Thermocouple>> {
    Ok(db.lock().await.get_thermocouples()?.to_vec())
}

#[tauri::command]
pub async fn set_thermocouples(thermocouples: Vec<Thermocouple>, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_thermocouples(thermocouples)
}

#[tauri::command]
pub async fn get_interp_method(db: Db<'_>) -> TlcResult<InterpMethod> {
    db.lock().await.get_interp_method()
}

#[tauri::command]
pub async fn set_interp_method(interp_method: InterpMethod, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_interp_method(interp_method)
}

#[tauri::command]
pub async fn interp_frame(frame_index: usize, db: Db<'_>) -> TlcResult<Array2<f64>> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.interp_frame(frame_index))
        .await
        .to()?
}

#[tauri::command]
pub async fn get_iter_method(db: Db<'_>) -> TlcResult<IterMethod> {
    db.lock().await.get_iter_method()
}

#[tauri::command]
pub async fn set_iter_method(iter_method: IterMethod, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_iter_method(iter_method)
}

#[tauri::command]
pub async fn get_physical_param(db: Db<'_>) -> TlcResult<PhysicalParam> {
    db.lock().await.get_physical_param()
}

#[tauri::command]
pub async fn set_physical_param(physical_param: PhysicalParam, db: Db<'_>) -> TlcResult<()> {
    db.lock().await.set_physical_param(physical_param)
}

#[tauri::command]
pub async fn get_nu_data(db: Db<'_>) -> TlcResult<NuData> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.get_nu_data()).await.to()?
}

#[tauri::command]
pub async fn get_nu_plot(trunc: Option<(f64, f64)>, db: Db<'_>) -> TlcResult<String> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.get_nu_plot(trunc)).await.to()?
}

#[tauri::command]
pub async fn save_data(db: Db<'_>) -> TlcResult<()> {
    let db = db.lock().await.snapshot();
    spawn_blocking(move || db.save_data()).await.to()?
}

trait IntoTlcResult<T> {
    fn to(self) -> TlcResult<T>;
}

impl<T, E> IntoTlcResult<T> for Result<T, E>
where
    E: ToString,
{
    fn to(self) -> TlcResult<T> {
        self.map_err(|e| e.to_string())
    }
}
