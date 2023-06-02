mod decode;
mod detect_peak;
mod read;
#[cfg(test)]
pub mod tests;

use std::{path::PathBuf, sync::Arc};

use anyhow::bail;
pub use ffmpeg::codec::{packet::Packet, Parameters};
use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};

use crate::{util::impl_eq_always_false, CalNumId, StartIndexId};
pub use decode::DecoderManager;
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
    pub frame_rate: usize,
    pub shape: (u32, u32),
    /// > [For video, one packet should typically contain one compressed frame](
    /// https://libav.org/documentation/doxygen/master/structAVPacket.html).
    ///
    /// There are two key points:
    /// 1. Will *one* packet contain more than *one* frame? As videos used
    /// in TLC experiments are lossless and have high-resolution, we can assert
    /// that one packet only contains one frame, which make multi-threaded
    /// decoding [much easier](https://www.cnblogs.com/TaigaCon/p/10220356.html).
    /// 2. Why not cache the frame data, which should be more straight forward?
    /// This is because packet is *compressed*. Specifically, a typical video
    /// in our experiments of 1.9GB will expend to 9.1GB if decompressed to rgb
    /// byte array, which may cause some trouble on PC.
    pub packets: Packets,
    /// Manage thread-local decoders.
    pub decoder_manager: DecoderManager,
}

#[derive(Clone)]
pub(crate) struct Packets(pub Arc<Vec<Packet>>);

impl_eq_always_false!(Packets);

impl std::fmt::Debug for Packets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Packets")
            .field(&format!("[..] len = {}", self.0.len()))
            .finish()
    }
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
    let (video_meta, parameters, packets) = read::read_video(path).map_err(|e| e.to_string())?;
    let VideoMeta {
        frame_rate, shape, ..
    } = video_meta;
    let packets = Packets(Arc::new(packets));
    let decoder_manager = DecoderManager::new(parameters, 4);
    let video_data_id = VideoDataId::new(db, frame_rate, shape, packets, decoder_manager);
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
    let decoder_manager = video_data_id.decoder_manager(db);
    let packets = video_data_id.packets(db).0;
    let start_frame = start_index_id.start_frame(db);
    let cal_num = cal_num_id.cal_num(db);
    let area = area_id.area(db);
    let green2 = decoder_manager
        .decode_all(packets, start_frame, cal_num, area)
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

/// `decode_frame_base64` is excluded from salsa database.
/// `decode_frame_base64` is nondeterministic as whether decoding can succeed depends whether
/// there is enough idle worker at the moment.
/// Meanwhile, `decode_frame_base64` is not part of the overall computation but just for display.
/// It can already yield the final output, so there is no benefit to extract out the impure part and
/// make it deterministic.
pub(crate) async fn decode_frame_base64(
    decoder_manager: DecoderManager,
    packets: Arc<Vec<Packet>>,
    frame_index: usize,
) -> anyhow::Result<String> {
    let nframes = packets.len();
    if frame_index >= packets.len() {
        bail!("frame_index({frame_index}) exceeds nframes({nframes})");
    }
    decoder_manager
        .decode_frame_base64(packets, frame_index)
        .await
}
