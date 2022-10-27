use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    sync::Mutex,
};

use anyhow::Result;
use crossbeam::queue::ArrayQueue;
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
    format::Pixel::RGB24,
    software::{scaling, scaling::flag::Flags},
    util::frame::video::Video,
};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::{Array2, Axis};
use rayon::{prelude::*, ThreadPool};
use thread_local::ThreadLocal;
use tokio::sync::oneshot;
use tracing::instrument;

use crate::ProgressBar;

const DEFAULT_NUM_THREADS: usize = 4;

/// `DecoderManager` maintains a thread-local style decoder pool to avoid frequent
/// initialization of decoder. It should be used with thread pool with a small
/// number of threads so that the thread-local decoders can be reused efficiently.
pub struct DecoderManager {
    parameters: Mutex<Parameters>,
    decoders: ThreadLocal<RefCell<Decoder>>,

    thread_pool: ThreadPool,
    ring_buffer: ArrayQueue<(Packet, oneshot::Sender<Result<String>>)>,
}

impl DecoderManager {
    pub(crate) fn new(parameters: Parameters) -> DecoderManager {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(DEFAULT_NUM_THREADS)
            .build()
            .expect("failed to create rayon thread pool");
        let ring_buffer = ArrayQueue::new(DEFAULT_NUM_THREADS);

        DecoderManager {
            parameters: Mutex::new(parameters),
            decoders: ThreadLocal::new(),
            thread_pool,
            ring_buffer,
        }
    }

    /// When user drags the progress bar quickly, the decoding can not keep up
    /// and there will be a significant lag. Actually, we do not have to decode
    /// every frames, and the key is how to give up decoding some frames properly.
    /// The naive solution to avoid too much backlog is maintaining the number of
    /// pending tasks and directly abort current decoding if it already exceeds the
    /// limit. But FIFO is not perfect for this use case because it's better to give
    /// priority to newer frames, e.g. we should at least guarantee decoding the frame
    /// where the progress bar **stops**.
    #[instrument(skip(self, packet))]
    pub fn decode_frame_base64(&self, packet: Packet) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        let _aborted = self.ring_buffer.force_push((packet, tx));
        rx.blocking_recv()?
    }

    fn decode_frame_base64_inner(&self, packet: &Packet) -> Result<String> {
        let mut decoder = self.decoder()?;
        let (w, h) = decoder.shape();
        let mut buf = Vec::new();
        let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
        jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

        Ok(base64::encode(buf))
    }

    #[instrument(skip(self, packets, progress_bar), err)]
    pub fn decode_all(
        &self,
        packets: Vec<Packet>,
        start_frame: usize,
        cal_num: usize,
        shape: (usize, usize, usize, usize),
        area: (usize, usize, usize, usize),
        progress_bar: ProgressBar,
    ) -> Result<Array2<u8>> {
        let byte_w = shape.1 * 3;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        progress_bar.start(cal_num as u32)?;
        let mut green2 = Array2::zeros((cal_num, cal_h * cal_w));
        packets
            .par_iter()
            .skip(start_frame)
            .zip(green2.axis_iter_mut(Axis(0)).into_iter())
            .try_for_each(|(packet, mut row)| -> Result<()> {
                let mut decoder = self.decoder()?;
                let dst_frame = decoder.decode(packet)?;

                // each frame is stored in a u8 array:
                // |r g b r g b...r g b|r g b r g b...r g b|......|r g b r g b...r g b|
                // |.......row_0.......|.......row_1.......|......|.......row_n.......|
                let rgb = dst_frame.data(0);
                let mut row_iter = row.iter_mut();

                for i in (0..).step_by(byte_w).skip(tl_y).take(cal_h) {
                    for j in (i..).skip(1).step_by(3).skip(tl_x).take(cal_w) {
                        // Bounds check can be removed by optimization so no need to use unsafe.
                        // Same performance as `unwrap_unchecked` + `get_unchecked`.
                        if let Some(b) = row_iter.next() {
                            *b = rgb[j];
                        }
                    }
                }

                // Cancel point.
                // This does not add any noticeable overhead.
                progress_bar.add(1)?;

                Ok(())
            })?;

        Ok(green2)
    }

    fn decoder(&self) -> Result<RefMut<Decoder>> {
        let decoder = self.decoders.get_or_try(|| -> Result<RefCell<Decoder>> {
            let decoder = Decoder::new(self.parameters.lock().unwrap().clone())?;
            Ok(RefCell::new(decoder))
        })?;

        Ok(decoder.borrow_mut())
    }
}

/// Decoder is bound to a specific video and can decode any packet of this video.
struct Decoder {
    codec_ctx: ffmpeg::decoder::Video,
    sws_ctx: SendableSwsCtx,
    /// `src_frame` and `dst_frame` are used to avoid frequent allocation.
    /// This can speed up decoding by about 10%.
    src_frame: Video,
    dst_frame: Video,
}

impl Decoder {
    fn new(parameters: Parameters) -> Result<Self> {
        let codec_ctx = codec::Context::from_parameters(parameters)?
            .decoder()
            .video()?;
        let (h, w) = (codec_ctx.height(), codec_ctx.width());
        let sws_ctx =
            scaling::Context::get(codec_ctx.format(), w, h, RGB24, w, h, Flags::BILINEAR)?;

        Ok(Self {
            codec_ctx,
            sws_ctx: SendableSwsCtx(sws_ctx),
            src_frame: Video::empty(),
            dst_frame: Video::empty(),
        })
    }

    fn decode(&mut self, packet: &Packet) -> Result<&Video> {
        self.codec_ctx.send_packet(packet)?;
        self.codec_ctx.receive_frame(&mut self.src_frame)?;
        self.sws_ctx.run(&self.src_frame, &mut self.dst_frame)?;

        Ok(&self.dst_frame)
    }

    fn shape(&self) -> (u32, u32) {
        (self.codec_ctx.height(), self.codec_ctx.width())
    }
}

/// Wrap `Context` to pass between threads(because of the raw pointer).
struct SendableSwsCtx(scaling::Context);

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for SendableSwsCtx {}

impl Deref for SendableSwsCtx {
    type Target = scaling::Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendableSwsCtx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
