use std::{
    assert_matches::debug_assert_matches,
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use crossbeam::{
    channel::{bounded, Receiver, Sender},
    queue::ArrayQueue,
};
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
    format::Pixel::RGB24,
    software::{scaling, scaling::flag::Flags},
    util::frame::video::Video,
};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::prelude::*;
use rayon::prelude::*;
use thread_local::ThreadLocal;
use tokio::sync::oneshot;
use tracing::instrument;

use crate::{
    util::{
        impl_eq_always_false,
        progress_bar::{Progress, ProgressBar},
    },
    video::VideoId,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Green2Id {
    pub video_id: VideoId,
    pub start_frame: usize,
    pub cal_num: usize,
    pub area: (u32, u32, u32, u32),
}

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
    pub(crate) fn new(
        parameters: Parameters,
        frame_backlog_capacity: usize,
        num_decode_frame_workers: usize,
    ) -> DecoderManager {
        assert!(num_decode_frame_workers > 0);

        let ring_buffer = ArrayQueue::new(frame_backlog_capacity);
        let (task_dispatcher, task_notifier) = bounded(frame_backlog_capacity);

        let decode_manager = DecoderManager {
            inner: Arc::new(DecoderManagerInner {
                parameters: Mutex::new(parameters),
                decoders: ThreadLocal::new(),
                ring_buffer,
                task_dispatcher,
            }),
        };

        for _ in 0..num_decode_frame_workers {
            let decode_manager = decode_manager.clone();
            let task_notifier = task_notifier.clone();
            std::thread::spawn(move || decode_manager.decode_frame_worker(task_notifier));
        }

        decode_manager
    }

    pub fn decode_frame_base64(&self, packet: Arc<Packet>) -> Result<String> {
        let (tx, rx) = oneshot::channel();

        // When the returned value which contains a `Sender` is dropped, its corresponding
        // `Receiver` which is waiting on another thread will be disconnected.
        self.inner.ring_buffer.force_push((packet, tx));
        let _ = self.inner.task_dispatcher.try_send(());

        rx.blocking_recv()?
    }

    pub fn decode_all(
        &self,
        packets: Vec<Arc<Packet>>,
        start_frame: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
        progress_bar: ProgressBar,
    ) -> Result<Array2<u8>> {
        self.inner
            .decode_all(packets, start_frame, cal_num, area, progress_bar)
    }

    fn decode_frame_worker(&self, task_notifier: Receiver<()>) {
        for _ in task_notifier {
            // `ring_buffer` and `task_dispatcher` are given the same number of elements,
            // so at this point ring_buffer should not be empty.
            let (packet, tx) = self
                .inner
                .ring_buffer
                .pop()
                .expect("ring_buffer should not be empty here");
            tx.send(self.inner.decode_frame_base64(&packet)).unwrap();
        }
    }
}

/// `DecoderManager` maintains a thread-local style decoder pool to avoid frequent
/// initialization of decoder. It should be used by a small number of threads so
/// that the thread-local decoders can be reused efficiently.
struct DecoderManagerInner {
    parameters: Mutex<Parameters>,
    decoders: ThreadLocal<RefCell<Decoder>>,

    /// When user drags the progress bar quickly, the decoding can not keep up
    /// and there will be a significant lag. Actually, we do not have to decode
    /// every frames, and the key is how to give up decoding some frames properly.
    /// The naive solution to avoid too much backlog is maintaining the number of
    /// pending tasks and directly abort current decoding if it already exceeds the
    /// limit. But FIFO is not perfect for this use case because it's better to give
    /// priority to newer frames, e.g. we should at least guarantee decoding the frame
    /// where the progress bar **stops**.
    /// `ring_buffer` is used to automatically eliminate the oldest frame to limit the
    /// number of backlog frames.
    /// `task_dispatcher` is a spmc used to trigger multiple workers.
    ring_buffer: ArrayQueue<(Arc<Packet>, oneshot::Sender<Result<String>>)>,
    task_dispatcher: Sender<()>,
}

impl DecoderManagerInner {
    fn decoder(&self) -> Result<RefMut<Decoder>> {
        let decoder = self.decoders.get_or_try(|| -> Result<RefCell<Decoder>> {
            let decoder = Decoder::new(self.parameters.lock().unwrap().clone())?;
            Ok(RefCell::new(decoder))
        })?;

        Ok(decoder.borrow_mut())
    }

    #[instrument(skip(self, packet))]
    fn decode_frame_base64(&self, packet: &Packet) -> Result<String> {
        let mut decoder = self.decoder()?;
        let (w, h) = decoder.shape();
        let mut buf = Vec::new();
        let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
        jpeg_encoder.encode(decoder.decode(packet)?.data(0), w, h, Rgb8)?;

        Ok(base64::encode(buf))
    }

    #[instrument(skip(self, packets, progress_bar), err)]
    fn decode_all(
        &self,
        packets: Vec<Arc<Packet>>,
        start_frame: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
        progress_bar: ProgressBar,
    ) -> Result<Array2<u8>> {
        let byte_w = self.decoder()?.shape().1 as usize * 3;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        let (tl_y, tl_x, cal_h, cal_w) =
            (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
        progress_bar.start(cal_num as u32)?;
        let mut green2 = Array2::zeros((cal_num, cal_h * cal_w));
        packets
            .par_iter()
            .skip(start_frame)
            .zip(green2.axis_iter_mut(Axis(0)))
            .try_for_each(|(packet, mut row)| -> Result<()> {
                // Cancel point.
                // This does not add noticeable overhead.
                progress_bar.add(1)?;
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

                Ok(())
            })?;

        debug_assert_matches!(
            progress_bar.get(),
            Progress::Finished { total } if total == cal_num as u32
        );

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

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicU32, Ordering::Relaxed},
            Arc,
        },
        thread::{sleep, spawn},
        time::Duration,
    };

    use crate::{
        util::progress_bar::ProgressBar,
        video::{
            read_video,
            test_util::{video_meta_real, video_meta_sample, VIDEO_PATH_REAL, VIDEO_PATH_SAMPLE},
            DecoderManager,
        },
    };

    #[test]
    fn test_decode_frame_sample() {
        _decode_frame(VIDEO_PATH_SAMPLE);
    }

    #[ignore]
    #[test]
    fn test_decode_frame_read() {
        _decode_frame(VIDEO_PATH_REAL);
    }

    #[test]
    fn test_decode_all_sample() {
        _decode_all(VIDEO_PATH_SAMPLE, 0, video_meta_sample().nframes);
    }

    #[test]
    fn test_decode_all_real() {
        _decode_all(VIDEO_PATH_REAL, 10, video_meta_real().nframes - 10);
    }

    fn _decode_frame(video_path: &str) {
        let progress_bar = ProgressBar::default();
        let (_, parameters, packet_rx) = read_video(video_path, progress_bar).unwrap();
        let decode_manager = DecoderManager::new(parameters, 10, 20);

        let mut handles = Vec::new();
        let ok_cnt = Arc::new(AtomicU32::new(0));
        let abort_cnt = Arc::new(AtomicU32::new(0));
        for packet in packet_rx {
            let decode_manager = decode_manager.clone();
            let ok_cnt = ok_cnt.clone();
            let abort_cnt = abort_cnt.clone();
            let handle =
                spawn(
                    move || match decode_manager.decode_frame_base64(Arc::new(packet)) {
                        Ok(_) => ok_cnt.fetch_add(1, Relaxed),
                        Err(_) => abort_cnt.fetch_add(1, Relaxed),
                    },
                );
            handles.push(handle);
            sleep(Duration::from_millis(1));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        dbg!(ok_cnt, abort_cnt);
    }

    fn _decode_all(video_path: &str, start_frame: usize, cal_num: usize) {
        let progress_bar = ProgressBar::default();
        let (_, parameters, packet_rx) = read_video(video_path, progress_bar).unwrap();
        let decode_manager = DecoderManager::new(parameters, 10, 20);

        let packets = packet_rx.into_iter().map(Arc::new).collect();
        let progress_bar = ProgressBar::default();

        decode_manager
            .decode_all(
                packets,
                start_frame,
                cal_num,
                (10, 10, 600, 800),
                progress_bar,
            )
            .unwrap();
    }
}
