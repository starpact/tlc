mod decode;
mod detect_peak;
mod io;
#[cfg(test)]
pub mod tests;

use std::{path::PathBuf, sync::Arc};

pub use ffmpeg::codec::{packet::Packet, Parameters};
use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};

use crate::{state::StartIndexId, CalNumId};
use decode::VideoData;
pub use detect_peak::FilterMethod;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub(crate) struct VideoMeta {
    pub frame_rate: usize,
    pub nframes: usize,
    /// (video_height, video_width)
    pub shape: (u32, u32),
}

#[salsa::input]
pub(crate) struct VideoPathId {
    #[return_ref]
    pub path: PathBuf,
}

#[salsa::tracked]
pub(crate) struct VideoDataId {
    pub video_data: Arc<VideoData>,
}

#[salsa::interned]
pub(crate) struct AreaId {
    pub area: (u32, u32, u32, u32),
}

#[salsa::tracked]
pub(crate) struct Green2Id {
    pub green2: ArcArray2<u8>,
}

#[salsa::interned]
pub(crate) struct FilterMethodId {
    pub filter_method: FilterMethod,
}

#[salsa::interned]
pub(crate) struct PointId {
    point: (usize, usize),
}

#[salsa::tracked]
pub(crate) struct GmaxFrameIndexesId {
    pub gmax_frame_indexes: Arc<Vec<usize>>,
}

pub(crate) fn init() {
    ffmpeg::init().expect("failed to init ffmpeg");
}

/// Reading from a file path is not actually deterministic because existence and content of the file
/// can change. We should track changes of the file outside of salsa system and set the input before
/// `read_video` to force re-execution when needed.
/// Same to `read_daq`.
#[salsa::tracked]
pub(crate) fn read_video(
    db: &dyn crate::Db,
    video_path_id: VideoPathId,
) -> Result<VideoDataId, String> {
    let path = video_path_id.path(db);
    let (parameters, frame_rate, packets) = io::read_video(path).map_err(|e| e.to_string())?;
    let decoder = VideoData::new(parameters, frame_rate, packets, 4).map_err(|e| e.to_string())?;
    let video_data_id = VideoDataId::new(db, Arc::new(decoder));
    Ok(video_data_id)
}

#[salsa::tracked]
pub(crate) fn decode_all(
    db: &dyn crate::Db,
    video_data_id: VideoDataId,
    start_index_id: StartIndexId,
    cal_num_id: CalNumId,
    area_id: AreaId,
) -> Result<Green2Id, String> {
    let video_data = video_data_id.video_data(db);
    let start_frame = start_index_id.start_frame(db);
    let cal_num = cal_num_id.cal_num(db);
    let area = area_id.area(db);
    let green2 = video_data
        .decode_all(start_frame, cal_num, area)
        .map_err(|e| e.to_string())?;
    Ok(Green2Id::new(db, green2.into_shared()))
}

#[salsa::tracked]
pub(crate) fn filter_detect_peak(
    db: &dyn crate::Db,
    green2_id: Green2Id,
    filter_method_id: FilterMethodId,
) -> GmaxFrameIndexesId {
    let green2 = green2_id.green2(db);
    let filter_method = filter_method_id.filter_method(db);
    let gmax_frame_indexes = detect_peak::filter_detect_peak(green2, filter_method);
    GmaxFrameIndexesId::new(db, Arc::new(gmax_frame_indexes))
}

#[salsa::tracked]
pub(crate) fn filter_point(
    db: &dyn crate::Db,
    green2_id: Green2Id,
    filter_method_id: FilterMethodId,
    area_id: AreaId,
    point_id: PointId,
) -> Result<Vec<u8>, String> {
    let green2 = green2_id.green2(db);
    let filter_method = filter_method_id.filter_method(db);
    let area = area_id.area(db);
    let point = point_id.point(db);
    let temp_history =
        detect_peak::filter_point(green2, filter_method, area, point).map_err(|e| e.to_string())?;
    Ok(temp_history)
}
