use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::atomic::{AtomicI64, Ordering},
};

use anyhow::{anyhow, bail, Result};
use ffmpeg_next as ffmpeg;
use ffmpeg_next::{
    codec,
    codec::packet::Packet,
    format::{
        context::input::{Input, PacketIter},
        Pixel::RGB24,
    },
    software::{scaling, scaling::flag::Flags},
    util::frame::video::Video,
};
use ndarray::prelude::*;
use parking_lot::Mutex;
use rayon::prelude::*;
use thread_local::ThreadLocal;
use tracing::debug;

use crate::{controller::cfg::G2Param, util::timing};

#[derive(Default)]
pub struct VideoCache {
    state: State,
    /// Cache thread-local decoder.
    decoder_cache: DecoderCache,

    /// > [For video, one packet should typically contain one compressed frame
    /// ](https://libav.org/documentation/doxygen/master/structAVPacket.html).
    ///
    /// There are two key points:
    /// 1. Will *one* packet contain more than *one* frame? As videos used
    /// in TLC experiments are lossless and have high-resolution, we can assert
    /// that one packet only contains one frame, which make multi-threaded
    /// decoding [much easier](https://www.cnblogs.com/TaigaCon/p/10220356.html).
    /// 2. Why not cache the frame data, which should be more straight forward?
    /// This is because packet is *compressed*. Specifically, a typical video
    /// in our experiments of 1.9GB will expend to 9.1GB if decoded to rgb byte
    /// array, which may cause some trouble on PC.
    pub packets: Vec<Packet>,
}

enum State {
    Uninitialized,
    Reading {
        /// Identifies the current video.
        path: PathBuf,
        /// Total packet/frame number of the current video, which is
        /// used to validate the `frame_index` parameter of `get_frame`.
        total_frames: usize,
    },
    Finished,
}

impl Default for State {
    fn default() -> Self {
        Self::Uninitialized
    }
}

#[derive(Default)]
struct DecoderCache {
    video_ctx: Mutex<codec::Context>,
    decoders: ThreadLocal<RefCell<Decoder>>,
}

#[derive(Debug)]
pub struct VideoMeta {
    pub path: PathBuf,
    pub frame_rate: usize,
    pub total_frames: usize,
    pub shape: (u32, u32),
}

impl VideoCache {
    pub fn init<P: AsRef<Path>>(
        &mut self,
        path: P,
        video_ctx: codec::Context,
        total_frames: usize,
    ) {
        self.decoder_cache.reset(video_ctx);
        self.state = State::Reading {
            path: path.as_ref().to_owned(),
            total_frames,
        };
        self.packets.clear();
    }

    pub fn get_decoder(&self) -> Result<RefMut<Decoder>> {
        self.decoder_cache.get_decoder()
    }

    pub fn target_changed<P: AsRef<Path>>(&self, original_path: P) -> bool {
        match self.state {
            State::Reading { ref path, .. } => original_path.as_ref() != path.as_path(),
            _ => false,
        }
    }

    pub fn worth_waiting(&self, frame_index: usize) -> Result<()> {
        match self.state {
            State::Reading { total_frames, .. } => {
                if total_frames < frame_index {
                    // This is an invalid `frame_index` from frontend and will never get the frame.
                    // So directly abort current thread. Then `rx` will be dropped and `tx` outside
                    // will stop pending(returning an `RecvError`).
                    bail!("frame_index({}) out of range", frame_index)
                }
                Ok(())
            }
            State::Finished => Ok(()),
            State::Uninitialized => bail!("video path unset"),
        }
    }

    pub fn mark_finished(&mut self) {
        self.state = State::Finished;
    }

    pub fn finished(&self) -> bool {
        matches!(self.state, State::Finished)
    }

    pub fn build_g2(&self, g2_param: G2Param, progress: &AtomicI64) -> Result<Array2<u8>> {
        let _timing = timing::start("building g2");
        debug!("{:#?}", g2_param);

        let G2Param {
            start_frame,
            frame_num,
            area: (tl_y, tl_x, h, w),
        } = g2_param;
        let byte_w = self.decoder_cache.get_decoder()?.shape().1 as usize * 3;
        let [tl_y, tl_x, h, w] = [tl_y as usize, tl_x as usize, h as usize, w as usize];

        let mut g2 = Array2::zeros((frame_num, h * w));

        self.packets
            .par_iter()
            .skip(start_frame)
            .zip(g2.axis_iter_mut(Axis(0)).into_iter())
            .try_for_each(|(packet, mut row)| -> Result<()> {
                let mut decoder = self.decoder_cache.get_decoder()?;
                let dst_frame = decoder.decode(packet)?;

                // the data of each frame store in a u8 array:
                // |r g b r g b...r g b|r g b r g b...r g b|......|r g b r g b...r g b|
                // |.......row_0.......|.......row_1.......|......|.......row_n.......|
                let rgb = dst_frame.data(0);
                let mut it = row.iter_mut();

                for i in (0..).step_by(byte_w).skip(tl_y).take(h) {
                    for j in (i..).skip(1).step_by(3).skip(tl_x).take(w) {
                        // Bounds check can be removed by optimization so no need to use unsafe.
                        // Same performance as `unwrap_unchecked` + `get_unchecked`.
                        if let Some(b) = it.next() {
                            *b = rgb[j];
                        }
                    }
                }

                if progress.fetch_add(1, Ordering::SeqCst) < 0 {
                    bail!("aborted");
                }

                Ok(())
            })?;

        Ok(g2)
    }
}

impl DecoderCache {
    fn get_decoder(&self) -> Result<RefMut<Decoder>> {
        let decoder = self.decoders.get_or_try(|| -> Result<RefCell<Decoder>> {
            let decoder = Decoder::new(self.video_ctx.lock())?;
            Ok(RefCell::new(decoder))
        })?;

        Ok(decoder.borrow_mut())
    }

    fn reset(&mut self, video_ctx: codec::Context) {
        self.video_ctx = Mutex::new(video_ctx);
        self.decoders.clear();
    }
}

pub struct VideoPacketIter<'a> {
    video_stream_index: usize,
    inner: PacketIter<'a>,
}

impl<'a> Iterator for VideoPacketIter<'a> {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .find(|(stream, _)| stream.index() == self.video_stream_index)
            .map(|(_, packet)| packet)
    }
}

pub fn open_video(input: &mut Input) -> Result<(usize, usize, codec::Context, VideoPacketIter)> {
    let video_stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("video stream not found"))?;

    let video_ctx = video_stream.codec();

    let rational = video_stream.avg_frame_rate();
    let frame_rate = (rational.0 as f64 / rational.1 as f64).round() as usize;
    let total_frames = video_stream.frames() as usize;

    Ok((
        frame_rate,
        total_frames,
        video_ctx,
        VideoPacketIter {
            video_stream_index: video_stream.index(),
            inner: input.packets(),
        },
    ))
}

/// wrap `Context` to pass between threads(because of the raw pointer)
struct SendableSWSCtx(scaling::Context);

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for SendableSWSCtx {}

impl Deref for SendableSWSCtx {
    type Target = scaling::Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendableSWSCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct Decoder {
    inner: ffmpeg::decoder::Video,
    sws_ctx: SendableSWSCtx,
    /// `src_frame` and `dst_frame` are used to avoid frequent allocation.
    /// This can speed up decoding by about 10%.
    src_frame: Video,
    dst_frame: Video,
}

impl Decoder {
    fn new<C: Deref<Target = codec::Context>>(video_ctx: C) -> Result<Self> {
        let decoder = video_ctx.clone().decoder().video()?;
        let (h, w) = (decoder.height(), decoder.width());
        let sws_ctx = scaling::Context::get(decoder.format(), w, h, RGB24, w, h, Flags::BILINEAR)?;

        Ok(Self {
            inner: decoder,
            sws_ctx: SendableSWSCtx(sws_ctx),
            src_frame: Video::empty(),
            dst_frame: Video::empty(),
        })
    }

    pub fn decode(&mut self, packet: &Packet) -> Result<&Video> {
        self.inner.send_packet(packet)?;
        self.inner.receive_frame(&mut self.src_frame)?;
        self.sws_ctx.run(&self.src_frame, &mut self.dst_frame)?;

        Ok(&self.dst_frame)
    }

    pub fn shape(&self) -> (u32, u32) {
        (self.inner.height(), self.inner.width())
    }
}
