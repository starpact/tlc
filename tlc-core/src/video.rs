mod controller;
mod decode;
mod detect_peak;
mod read_video;
#[cfg(test)]
mod test;

use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
pub use ffmpeg::codec::{packet::Packet, Parameters};
use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};

use crate::{
    util::{impl_eq_always_false, progress_bar::ProgressBar},
    CalNumId,
};
pub use controller::VideoController;
pub use decode::{DecoderManager, Green2Id};
pub use detect_peak::{filter_detect_peak, filter_point, FilterMethod, GmaxId};
pub use read_video::read_video;

pub struct VideoData {
    video_meta: VideoMeta,

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
    packets: Vec<Arc<Packet>>,

    /// Manage thread-local decoders.
    decoder_manager: DecoderManager,

    /// Green value 2d matrix(cal_num, pix_num).
    green2: Option<ArcArray2<u8>>,

    /// Frame index of peak temperature.
    gmax_frame_indexes: Option<Arc<Vec<usize>>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoId {
    pub video_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct VideoMeta {
    pub frame_rate: usize,
    pub nframes: usize,
    /// (video_height, video_width)
    pub shape: (u32, u32),
}

pub fn init() {
    ffmpeg::init().expect("failed to init ffmpeg");
}

impl VideoData {
    pub fn new(video_meta: VideoMeta, parameters: Parameters) -> VideoData {
        const FRAME_BACKLOG_CAPACITY: usize = 2;
        const NUM_DECODE_FRAME_WORKERS: usize = 4;

        let packets = Vec::with_capacity(video_meta.nframes);
        let decoder_manager =
            DecoderManager::new(parameters, FRAME_BACKLOG_CAPACITY, NUM_DECODE_FRAME_WORKERS);

        VideoData {
            video_meta,
            packets,
            decoder_manager,
            green2: None,
            gmax_frame_indexes: None,
        }
    }

    pub fn video_meta(&self) -> VideoMeta {
        self.video_meta
    }

    pub fn packet(&self, frame_index: usize) -> Result<Arc<Packet>> {
        self.packets
            .get(frame_index)
            .cloned()
            .ok_or_else(|| anyhow!("packet(frame_index = {frame_index}) not loaded yet"))
    }

    pub fn packets(&self) -> Result<Vec<Arc<Packet>>> {
        if self.packets.len() < self.video_meta.nframes {
            bail!("loading packets not finished yet");
        }
        Ok(self.packets.clone())
    }

    pub fn decoder_manager(&self) -> DecoderManager {
        self.decoder_manager.clone()
    }

    pub fn green2(&self) -> Option<ArcArray2<u8>> {
        self.green2.clone()
    }

    pub fn set_green2(&mut self, green2: Option<ArcArray2<u8>>) -> &mut Self {
        self.green2 = green2;
        self
    }

    pub fn gmax_frame_indexes(&self) -> Option<Arc<Vec<usize>>> {
        self.gmax_frame_indexes.clone()
    }

    pub fn set_gmax_frame_indexes(
        &mut self,
        gmax_frame_indexes: Option<Arc<Vec<usize>>>,
    ) -> &mut Self {
        self.gmax_frame_indexes = gmax_frame_indexes;
        self
    }

    pub fn push_packet(&mut self, packet: Arc<Packet>) -> Result<()> {
        if packet.dts() != Some(self.packets.len() as i64) {
            bail!("wrong packet");
        }
        self.packets.push(packet);

        Ok(())
    }
}

#[cfg(test)]
mod test_util {
    use crate::VideoMeta;

    pub const VIDEO_PATH_SAMPLE: &str = "./testdata/almost_empty.avi";
    pub const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";

    pub fn video_meta_sample() -> VideoMeta {
        VideoMeta {
            frame_rate: 25,
            nframes: 3,
            shape: (1024, 1280),
        }
    }

    pub fn video_meta_real() -> VideoMeta {
        VideoMeta {
            frame_rate: 25,
            nframes: 2444,
            shape: (1024, 1280),
        }
    }
}

#[salsa::input]
pub(crate) struct VideoPathId {
    pub path: PathBuf,
}

#[salsa::tracked]
pub(crate) struct VideoDataId {
    pub frame_rate: usize,
    pub shape: (u32, u32),
    pub packets: Packets,
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
pub(crate) struct StartFrameId {
    pub start_frame: usize,
}

#[salsa::interned]
pub(crate) struct Area {
    pub area: (u32, u32, u32, u32),
}

#[salsa::tracked]
pub(crate) struct Green2 {
    pub green2: ArcArray2<u8>,
}

#[salsa::interned]
pub(crate) struct FilterMethodId {
    pub filter_method: FilterMethod,
}

#[salsa::tracked]
pub(crate) struct FilteredGreen2 {
    pub filtered_green2: ArcArray2<u8>,
}

/// Reading from a file path is not actually deterministic because existence and content of the file
/// can change. We should track changes of the file outside of salsa system and set the input before
/// `read_video` to force re-execution when needed.
/// Same to `read_daq`.
#[salsa::tracked]
pub(crate) fn _read_video(
    db: &dyn crate::Db,
    video_path_id: VideoPathId,
) -> Result<VideoDataId, String> {
    let path = video_path_id.path(db);
    let (video_meta, parameters, rx) =
        read_video(path, ProgressBar::default()).map_err(|e| e.to_string())?;
    let VideoMeta {
        frame_rate, shape, ..
    } = video_meta;
    let packets = Packets(Arc::new(rx.into_iter().collect())); // TODO
    let decoder_manager = DecoderManager::new(parameters, 2, 4);
    let video_data_id = VideoDataId::new(db, frame_rate, shape, packets, decoder_manager);

    Ok(video_data_id)
}

#[salsa::tracked]
pub(crate) fn _decode_all(
    db: &dyn crate::Db,
    video_data_id: VideoDataId,
    start_frame_id: StartFrameId,
    cal_num_id: CalNumId,
    area_id: Area,
) -> Result<Green2, String> {
    let decoder_manager = video_data_id.decoder_manager(db);
    let packets = video_data_id.packets(db).0;
    let packets = packets
        .iter()
        .map(|packet| Arc::new(packet.clone())) // TODO
        .collect();
    let start_frame = start_frame_id.start_frame(db);
    let cal_num = cal_num_id.cal_num(db);
    let area = area_id.area(db);
    let green2 = decoder_manager
        .decode_all(packets, start_frame, cal_num, area, ProgressBar::default())
        .map_err(|e| e.to_string())?;

    Ok(Green2::new(db, green2.into_shared()))
}

/// `decode_frame_base64` is nondeterministic as whether decoding can succeed depends
/// whether there is enough idle worker as the moment.
/// Meanwhile, `decode_frame_base64` can already yield the final output, there is no
/// benefit to extract out the impure part and make it deterministic.
/// So `decode_frame_base64` is excluded from salsa system.
pub(crate) fn _decode_frame_base64(
    db: &dyn crate::Db,
    video_data_id: VideoDataId,
    frame_index: usize,
) -> anyhow::Result<String> {
    let decoder_manager = video_data_id.decoder_manager(db);
    let packets = video_data_id.packets(db).0;
    if frame_index >= packets.len() {
        bail!(
            "frame index out of bounds: {} > {}",
            frame_index,
            packets.len()
        );
    }

    decoder_manager.decode_frame_base64(Arc::new(packets[frame_index].clone())) // TODO
}
