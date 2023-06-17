use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose, Engine};
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
    format::Pixel::RGB24,
    software::{scaling, scaling::flag::Flags},
    util::frame::video::Video,
};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::prelude::*;
use rayon::{prelude::*, ThreadPool, ThreadPoolBuilder};
use thread_local::ThreadLocal;
use tokio::sync::{oneshot, Semaphore};
use tracing::instrument;

use crate::util::impl_eq_always_false;

#[derive(Clone)]
pub struct DecoderManager {
    inner: Arc<DecoderManagerInner>,
}

impl std::fmt::Debug for DecoderManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecoderManager").finish()
    }
}

impl_eq_always_false!(DecoderManager);

impl DecoderManager {
    pub(crate) fn new(parameters: Parameters, num_decode_frame_workers: usize) -> DecoderManager {
        assert!(num_decode_frame_workers > 0);
        let backlog = Mutex::new(None);
        let sem = Semaphore::new(num_decode_frame_workers);
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(num_decode_frame_workers)
            .build()
            .unwrap();

        DecoderManager {
            inner: Arc::new(DecoderManagerInner {
                parameters: Mutex::new(parameters),
                decoders: ThreadLocal::new(),
                backlog,
                sem,
                thread_pool,
            }),
        }
    }

    pub async fn decode_frame_base64(
        &self,
        packets: Arc<Vec<Packet>>,
        frame_index: usize,
    ) -> anyhow::Result<String> {
        let (tx, rx) = oneshot::channel();
        if let Ok(_permit) = self.inner.sem.try_acquire() {
            self.spawn_decode_frame_base64(packets, frame_index, tx);
            return rx.await?;
        }
        // When the old value which contains a tx is dropped, its corresponding
        // rx(which is awaiting) will be disconnected.
        *self.inner.backlog.lock().unwrap() = Some(tx);
        let _permit = self.inner.sem.acquire().await.unwrap();
        if let Some(tx) = self.inner.backlog.lock().unwrap().take() {
            // `tx` here may be from another more recent task.
            self.spawn_decode_frame_base64(packets, frame_index, tx);
        }
        rx.await?
    }

    pub fn decode_all(
        &self,
        packets: Arc<Vec<Packet>>,
        start_frame: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
    ) -> anyhow::Result<Array2<u8>> {
        self.inner.decode_all(packets, start_frame, cal_num, area)
    }

    fn spawn_decode_frame_base64(
        &self,
        packets: Arc<Vec<Packet>>,
        frame_index: usize,
        tx: oneshot::Sender<anyhow::Result<String>>,
    ) {
        let inner = self.inner.clone();
        // Semaphore with same number of permits guarantees it will never block here.
        self.inner.thread_pool.spawn(move || {
            tx.send(inner.decode_frame_base64(&packets[frame_index]))
                .unwrap();
        });
    }
}

/// `DecoderManager` maintains a thread-local style decoder pool to avoid frequent
/// initialization of decoder. It should be used by a small number of threads so
/// that the thread-local decoders can be reused efficiently.
struct DecoderManagerInner {
    parameters: Mutex<Parameters>,
    decoders: ThreadLocal<RefCell<Decoder>>,

    /// When user drags the progress bar quickly, the decoding can not keep up and
    /// there will be a significant lag. However, we actually do not have to decode
    /// every frames, and the key is how to give up decoding some frames properly.
    /// The naive solution to avoid too much backlog is maintaining the number of
    /// pending tasks and directly abort current decoding if it already exceeds the
    /// limit. But FIFO is not perfect for this use case because it's better to give
    /// priority to newer frames, e.g. we should at least guarantee decoding the frame
    /// where the progress bar **stops**.
    /// `backlog` only stores sender of the most recent frame, as a simplified version of
    /// ringbuffer.
    backlog: Mutex<Option<oneshot::Sender<anyhow::Result<String>>>>,
    sem: Semaphore,
    thread_pool: ThreadPool,
}

impl DecoderManagerInner {
    fn decoder(&self) -> anyhow::Result<RefMut<Decoder>> {
        let decoder = self
            .decoders
            .get_or_try(|| -> anyhow::Result<RefCell<Decoder>> {
                let decoder = Decoder::new(self.parameters.lock().unwrap().clone())?;
                Ok(RefCell::new(decoder))
            })?;
        Ok(decoder.borrow_mut())
    }

    #[instrument(skip(self, packet))]
    fn decode_frame_base64(&self, packet: &Packet) -> anyhow::Result<String> {
        let mut decoder = self.decoder()?;
        let (w, h) = decoder.shape();
        let mut buf = Vec::new();
        // `JpegEncoder` could be reused by thread-local to avoid allocation but
        // as most time are spent on decoding/encoding, it's probably not necessary.
        let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
        jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;
        Ok(general_purpose::STANDARD.encode(buf))
    }

    #[instrument(skip(self, packets), err)]
    fn decode_all(
        &self,
        packets: Arc<Vec<Packet>>,
        start_frame: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
    ) -> anyhow::Result<Array2<u8>> {
        let byte_w = self.decoder()?.shape().1 as usize * 3;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        let (tl_y, tl_x, cal_h, cal_w) =
            (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
        let mut green2 = Array2::zeros((cal_num, cal_h * cal_w));
        packets
            .par_iter()
            .skip(start_frame)
            .zip(green2.axis_iter_mut(Axis(0)))
            .try_for_each(|(packet, mut row)| -> anyhow::Result<()> {
                let mut decoder = self.decoder()?;
                let dst_frame = decoder.decode(packet)?;

                // Each frame is stored in a u8 array:
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
                Ok(())
            })?;
        Ok(green2)
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
    fn new(parameters: Parameters) -> anyhow::Result<Self> {
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

    fn decode(&mut self, packet: &Packet) -> anyhow::Result<&Video> {
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

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicU32, Ordering::Relaxed},
            Arc,
        },
        time::Duration,
    };

    use crate::video::{
        read::read_video,
        tests::{video_meta_real, video_meta_sample, VIDEO_PATH_REAL, VIDEO_PATH_SAMPLE},
        DecoderManager,
    };

    #[tokio::test]
    async fn test_decode_frame_sample() {
        _decode_frame(VIDEO_PATH_SAMPLE).await;
    }

    #[ignore]
    #[tokio::test]
    async fn test_decode_frame_read() {
        _decode_frame(VIDEO_PATH_REAL).await;
    }

    #[test]
    fn test_decode_all_sample() {
        _decode_all(VIDEO_PATH_SAMPLE, 0, video_meta_sample().nframes);
    }

    #[ignore]
    #[test]
    fn test_decode_all_real() {
        _decode_all(VIDEO_PATH_REAL, 10, video_meta_real().nframes - 10);
    }

    async fn _decode_frame(video_path: &str) {
        let (_, parameters, packets) = read_video(video_path).unwrap();
        let decode_manager = DecoderManager::new(parameters, 20);

        let mut handles = Vec::new();
        let ok_cnt = Arc::new(AtomicU32::new(0));
        let abort_cnt = Arc::new(AtomicU32::new(0));
        let packets = Arc::new(packets);
        for i in 0..packets.len() {
            let packets = packets.clone();
            let decode_manager = decode_manager.clone();
            let ok_cnt = ok_cnt.clone();
            let abort_cnt = abort_cnt.clone();
            let handle = tokio::spawn(async move {
                match decode_manager.decode_frame_base64(packets, i).await {
                    Ok(_) => ok_cnt.fetch_add(1, Relaxed),
                    Err(_) => abort_cnt.fetch_add(1, Relaxed),
                }
            });
            handles.push(handle);
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        for handle in handles {
            let _ = handle.await;
        }

        dbg!(ok_cnt, abort_cnt);
    }

    fn _decode_all(video_path: &str, start_frame: usize, cal_num: usize) {
        let (_, parameters, packets) = read_video(video_path).unwrap();
        let decode_manager = DecoderManager::new(parameters, 20);
        decode_manager
            .decode_all(Arc::new(packets), start_frame, cal_num, (10, 10, 600, 800))
            .unwrap();
    }
}
