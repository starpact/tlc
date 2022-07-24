use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
};

use anyhow::{anyhow, bail, Result};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use rayon::{ThreadPool, ThreadPoolBuilder};
use tokio::sync::{oneshot, Semaphore};
use tracing::{instrument, trace_span};

use super::VideoData;

pub struct FrameReader {
    thread_pool: ThreadPool,
    semaphore: Semaphore,
    last_pending: AtomicUsize,
}

impl FrameReader {
    pub(super) fn new() -> Self {
        // As thread-local decoders are designed to be kept in just a few threads,
        // so a standalone `rayon` thread pool is used.
        // `spawn` form `rayon`'s global thread pool will block when something like
        // `par_iter` is working as `rayon` uses `depth-first` strategy for highest
        // efficiency. So another dedicated thread pool is used.
        const NUM_THREADS: usize = 4;
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(NUM_THREADS)
            .build()
            .expect("Failed to init rayon thread pool");

        Self {
            thread_pool,
            semaphore: Semaphore::new(NUM_THREADS),
            last_pending: AtomicUsize::new(0),
        }
    }

    pub(super) async fn read_single_frame_base64(
        &self,
        video_data: Arc<RwLock<VideoData>>,
        frame_index: usize,
    ) -> Result<String> {
        // When user drags the progress bar quickly, the decoding can not keep up
        // and there will be a significant lag. Actually, we do not have to decode
        // every frames, and the key is how to give up decoding some frames properly.
        // The naive solution to avoid too much backlog is maintaining the number of
        // pending tasks and directly abort current decoding if it already exceeds the
        // limit. But it's not perfect for this use case because it can not guarantee
        // decoding the frame where the progress bar **stops**.

        // An asynchronous semaphore is used to ensure it will be not synchronously
        // blocked at `spawn` when there is no idle worker threads. The number of
        // permits must be the same as the number of threads of the pool.
        if let Ok(_permit) = self.semaphore.try_acquire() {
            return self
                .read_single_frame_base64_core(video_data, frame_index)
                .await;
        }

        self.last_pending.store(frame_index, Ordering::Relaxed);

        let _permit = self.semaphore.acquire().await;

        // While awaiting here, the `last_pending` may be modified by subsequent requests.
        // So we need to check if this is still the last one, otherwise we should abort it
        // to make sure the last one is processed.
        if self.last_pending.load(Ordering::Relaxed) != frame_index {
            bail!("no idle worker thread");
        }

        self.read_single_frame_base64_core(video_data, frame_index)
            .await
    }

    async fn read_single_frame_base64_core(
        &self,
        video_data: Arc<RwLock<VideoData>>,
        frame_index: usize,
    ) -> Result<String> {
        let (tx, rx) = oneshot::channel();

        // It will never be synchronously blocked here because we have used semaphore to
        // ensure that there is idel thread in the pool when we reach here.
        self.thread_pool.spawn(move || {
            let ret = read_single_frame_base64(video_data, frame_index);
            let _ = tx.send(ret);
        });

        rx.await?
    }
}

#[instrument(level = "trace", skip(video_data))]
fn read_single_frame_base64(
    video_data: Arc<RwLock<VideoData>>,
    frame_index: usize,
) -> Result<String> {
    loop {
        let video_data = video_data.read().unwrap();
        let video_cache = video_data
            .video_cache
            .as_ref()
            .ok_or_else(|| anyhow!("uninitialized"))?;

        let nframes = video_cache.video_metadata.nframes;
        if frame_index >= nframes {
            // This is an invalid `frame_index` from frontend and will never get the frame.
            // So directly abort it.
            bail!("frame_index({}) out of range({})", frame_index, nframes);
        }

        if let Some(packet) = video_cache.packets.get(frame_index) {
            let _span = trace_span!("decode_single_frame").entered();

            let mut decoder = video_cache.decoder_manager.get()?;
            let (h, w) = video_cache.video_metadata.shape;
            let mut buf = Vec::new();
            let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
            jpeg_encoder.encode(decoder.decode(packet)?.data(0), w as u32, h as u32, Rgb8)?;

            break Ok(base64::encode(buf));
        }
    }
}
