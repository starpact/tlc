#![feature(assert_matches)]

mod controller;
mod decode;
mod detect_peak;
mod read_video;

use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
pub use ffmpeg::codec::{packet::Packet, Parameters};
use ndarray::ArcArray2;
use serde::{Deserialize, Serialize};

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

    pub const VIDEO_PATH_SAMPLE: &str = "../tests/almost_empty.avi";
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
