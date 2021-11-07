use std::cell::{Ref, RefCell};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use ffmpeg_next as ffmpeg;

use anyhow::{anyhow, Result};
use derivative::Derivative;
use ffmpeg::codec::{self, packet::Packet};
use ffmpeg::format::context::input::{Input, PacketIter};
use ffmpeg::format::Pixel::RGB24;
use ffmpeg::software::scaling::{self, flag::Flags};
use ffmpeg::util::frame::video::Video;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use thread_local::ThreadLocal;
use tracing::debug;

#[derive(Derivative, Default)]
#[derivative(Debug)]
pub struct VideoCache {
    /// Identifies the current video.
    pub path: Option<PathBuf>,
    /// Total packet/frame number of the current video.
    /// This is used to validate the `frame_index` parameter of `get_frame`.
    pub total: usize,
    /// Get thread-local decoder.
    #[derivative(Debug = "ignore")]
    pub decoder_cache: DecoderCache,
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
    #[derivative(Debug = "ignore")]
    pub packets: Vec<Packet>,
}

#[derive(Default)]
pub struct DecoderCache {
    video_ctx: Mutex<codec::Context>,
    decoders: ThreadLocal<Decoder>,
}

impl VideoCache {
    pub fn reset<P: AsRef<Path>>(
        &mut self,
        path: P,
        video_ctx: codec::Context,
        total_frames: usize,
    ) {
        self.path = Some(path.as_ref().to_owned());
        self.decoder_cache.reset(video_ctx);
        self.total = total_frames;
        self.packets.clear();
    }

    pub fn path_changed<P: AsRef<Path>>(&self, path: P) -> bool {
        let old = match self.path {
            Some(ref path) => path,
            None => return true,
        };
        let new = path.as_ref();

        old != new
    }

    pub fn decode_all(&self) -> Result<()> {
        self.packets
            .par_iter()
            .try_for_each(|packet| -> Result<()> {
                let decoder = self.decoder_cache.get_decoder()?;
                let buf = decoder.decode(packet)?.data(0).to_vec();

                debug!("{}", buf.len());

                // the data of each frame store in one u8 array:
                // ||r g b r g b...r g b|......|r g b r g b...r g b||
                // ||.......row_0.......|......|.......row_n.......||

                Ok(())
            })?;

        Ok(())
    }
}

impl DecoderCache {
    pub fn get_decoder(&self) -> Result<&Decoder> {
        self.decoders
            .get_or_try(|| Decoder::new(&*self.video_ctx.lock()))
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
        .ok_or(anyhow!("video stream not found"))?;

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
    decoder: RefCell<ffmpeg::decoder::Video>,
    sws_ctx: RefCell<SendableSWSCtx>,
    src_frame: RefCell<Video>,
    dst_frame: RefCell<Video>,
}

impl Decoder {
    fn new(video_ctx: &codec::Context) -> Result<Self> {
        let decoder = video_ctx.clone().decoder().video()?;
        let (h, w) = (decoder.height(), decoder.width());
        let sws_ctx = scaling::Context::get(decoder.format(), w, h, RGB24, w, h, Flags::BILINEAR)?;

        Ok(Self {
            decoder: RefCell::new(decoder),
            sws_ctx: RefCell::new(SendableSWSCtx(sws_ctx)),
            src_frame: RefCell::new(Video::empty()),
            dst_frame: RefCell::new(Video::empty()),
        })
    }

    pub fn decode(&self, packet: &Packet) -> Result<Ref<Video>> {
        let mut decoder = self.decoder.borrow_mut();
        let mut sws_ctx = self.sws_ctx.borrow_mut();
        let mut src_frame = self.src_frame.borrow_mut();
        let mut dst_frame = self.dst_frame.borrow_mut();

        decoder.send_packet(packet)?;
        decoder.receive_frame(&mut src_frame)?;
        sws_ctx.run(&src_frame, &mut dst_frame)?;
        drop(dst_frame);

        Ok(self.dst_frame.borrow())
    }
}
