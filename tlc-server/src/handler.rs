use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{to_value, Value};
use tlc_core::{
    FilterMethod, InterpMethod, IterMethod, ParallelDatabase, PhysicalParam, Thermocouple,
};
use tokio::{sync::Mutex, task::spawn_blocking};

type Db = Arc<Mutex<tlc_core::Database>>;

pub struct AppError(anyhow::Error);
pub type AppResult<T> = Result<T, AppError>;

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

pub async fn console() {
    unimplemented!()
}

pub async fn get_name(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_name()?)?))
}

pub async fn set_name(Path(name): Path<String>, State(db): State<Db>) -> AppResult<()> {
    Ok(db.lock().await.set_name(name)?)
}

pub async fn get_save_root_dir(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_save_root_dir()?)?))
}

pub async fn set_save_root_dir(
    State(db): State<Db>,
    Path(save_root_dir): Path<PathBuf>,
) -> AppResult<()> {
    Ok(db.lock().await.set_save_root_dir(save_root_dir)?)
}

pub async fn get_video_path(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_video_path()?)?))
}

pub async fn set_video_path(
    State(db): State<Db>,
    Path(video_path): Path<PathBuf>,
) -> AppResult<()> {
    Ok(db.lock().await.set_video_path(video_path)?)
}

pub async fn get_video_nframes(State(db): State<Db>) -> AppResult<Json<Value>> {
    let db = db.lock().await.snapshot();
    let nframes = spawn_blocking(move || db.get_video_nframes()).await??;
    Ok(Json(to_value(nframes)?))
}

pub async fn get_video_frame_rate(State(db): State<Db>) -> AppResult<Json<Value>> {
    let db = db.lock().await.snapshot();
    let frame_rate = spawn_blocking(move || db.get_video_frame_rate()).await??;
    Ok(Json(to_value(frame_rate)?))
}

pub async fn get_video_shape(State(db): State<Db>) -> AppResult<Json<Value>> {
    let db = db.lock().await.snapshot();
    let shape = spawn_blocking(move || db.get_video_shape()).await??;
    Ok(Json(to_value(shape)?))
}

pub async fn decode_frame_base64(
    State(db): State<Db>,
    Path(frame_index): Path<usize>,
) -> AppResult<String> {
    let db = db.lock().await.snapshot();
    Ok(tlc_core::decode_frame_base64(db, frame_index).await?)
}

pub async fn get_daq_path(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_daq_path()?)?))
}

pub async fn set_daq_path(State(db): State<Db>, Path(daq_path): Path<PathBuf>) -> AppResult<()> {
    Ok(db.lock().await.set_daq_path(daq_path)?)
}

pub async fn get_daq_data(State(db): State<Db>) -> AppResult<Json<Value>> {
    let db = db.lock().await.snapshot();
    let daq_data = spawn_blocking(move || db.get_daq_data()).await??;
    Ok(Json(to_value(daq_data)?))
}

#[derive(Deserialize)]
pub struct SynchronizeRequest {
    start_frame: usize,
    start_row: usize,
}

pub async fn synchronize_video_and_daq(
    State(db): State<Db>,
    Query(SynchronizeRequest {
        start_frame,
        start_row,
    }): Query<SynchronizeRequest>,
) -> AppResult<()> {
    Ok(db
        .lock()
        .await
        .synchronize_video_and_daq(start_frame, start_row)?)
}

pub async fn get_start_frame(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_start_frame()?)?))
}

pub async fn set_start_frame(
    State(db): State<Db>,
    Path(start_frame): Path<usize>,
) -> AppResult<()> {
    Ok(db.lock().await.set_start_frame(start_frame)?)
}

pub async fn get_start_row(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_start_row()?)?))
}

pub async fn set_start_row(State(db): State<Db>, Path(start_row): Path<usize>) -> AppResult<()> {
    Ok(db.lock().await.set_start_row(start_row)?)
}

pub async fn get_area(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_area()?)?))
}

pub async fn set_area(
    State(db): State<Db>,
    Json(area): Json<(u32, u32, u32, u32)>,
) -> AppResult<()> {
    Ok(db.lock().await.set_area(area)?)
}

pub async fn get_filter_method(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_filter_method()?)?))
}

pub async fn set_filter_method(
    State(db): State<Db>,
    Json(filter_method): Json<FilterMethod>,
) -> AppResult<()> {
    Ok(db.lock().await.set_filter_method(filter_method)?)
}

pub async fn filter_point(
    State(db): State<Db>,
    Json(point): Json<(usize, usize)>,
) -> AppResult<Vec<u8>> {
    let db = db.lock().await.snapshot();
    Ok(spawn_blocking(move || db.filter_point(point)).await??)
}

pub async fn get_thermocouples(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(
        db.lock().await.get_thermocouples()?.to_vec(),
    )?))
}

pub async fn set_thermocouples(
    State(db): State<Db>,
    Json(thermocouples): Json<Vec<Thermocouple>>,
) -> AppResult<()> {
    Ok(db.lock().await.set_thermocouples(thermocouples)?)
}

pub async fn get_interp_method(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_interp_method()?)?))
}

pub async fn set_interp_method(
    State(db): State<Db>,
    Json(interp_method): Json<InterpMethod>,
) -> AppResult<()> {
    Ok(db.lock().await.set_interp_method(interp_method)?)
}

pub async fn interp_frame(
    State(db): State<Db>,
    Path(frame_index): Path<usize>,
) -> AppResult<Json<Value>> {
    let db = db.lock().await.snapshot();
    let interped_frame = spawn_blocking(move || db.interp_frame(frame_index)).await??;
    Ok(Json(to_value(interped_frame)?))
}

pub async fn get_iter_method(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_iter_method()?)?))
}

pub async fn set_iter_method(
    State(db): State<Db>,
    Json(iter_method): Json<IterMethod>,
) -> AppResult<()> {
    Ok(db.lock().await.set_iter_method(iter_method)?)
}

pub async fn get_physical_param(State(db): State<Db>) -> AppResult<Json<Value>> {
    Ok(Json(to_value(db.lock().await.get_physical_param()?)?))
}

pub async fn set_physical_param(
    State(db): State<Db>,
    Json(physical_param): Json<PhysicalParam>,
) -> AppResult<()> {
    Ok(db.lock().await.set_physical_param(physical_param)?)
}

pub async fn get_nu_data(State(db): State<Db>) -> AppResult<Json<Value>> {
    let db = db.lock().await.snapshot();
    let nu_data = spawn_blocking(move || db.get_nu_data()).await??;
    Ok(Json(to_value(nu_data)?))
}

pub async fn get_nu_plot(
    State(db): State<Db>,
    Json(trunc): Json<Option<(f64, f64)>>,
) -> AppResult<String> {
    let db = db.lock().await.snapshot();
    Ok(spawn_blocking(move || db.get_nu_plot(trunc)).await??)
}

pub async fn save_data(State(db): State<Db>) -> AppResult<()> {
    let db = db.lock().await.snapshot();
    Ok(spawn_blocking(move || db.save_data()).await??)
}
