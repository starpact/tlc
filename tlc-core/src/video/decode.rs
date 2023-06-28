use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

use base64::{engine::general_purpose, Engine};
use crossbeam::{
    channel::{Receiver, Sender},
    queue::ArrayQueue,
};
use ffmpeg::{
    codec,
    codec::{packet::Packet, Parameters},
    format::Pixel::RGB24,
    software::scaling,
    util::frame::video::Video,
};
use image::{codecs::jpeg::JpegEncoder, ColorType::Rgb8};
use ndarray::prelude::*;
use tokio::sync::oneshot;
use tracing::instrument;

use crate::util::impl_eq_always_false;

pub struct VideoData {
    parameters: Mutex<Parameters>,
    frame_rate: usize,
    shape: (u32, u32),
    packets: Arc<[Packet]>,
    /// When user drags the progress bar quickly, the decoding can not keep up and
    /// there will be a significant lag. However, we actually do not have to decode
    /// every frames, and the key is how to give up decoding some frames properly.
    /// The naive solution to avoid too much backlog is maintaining the number of
    /// pending tasks and directly abort current decoding if it already exceeds the
    /// limit. But FIFO is not perfect for this use case because it's better to give
    /// priority to newer frames, e.g. we should at least guarantee decoding the frame
    /// where the progress bar **stops**.
    /// `task_ring_buffer` is a ring buffer that only stores the most recent tasks.
    task_ring_buffer: Arc<ArrayQueue<DecodeFrameTask>>,
    task_waker: Sender<()>,
    worker_handles: Box<[JoinHandle<()>]>,
}

struct DecodeFrameTask {
    frame_index: usize,
    tx: oneshot::Sender<anyhow::Result<String>>,
}

impl std::fmt::Debug for VideoData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder").finish()
    }
}

impl_eq_always_false!(VideoData);

impl VideoData {
    pub fn new(
        parameters: Parameters,
        frame_rate: usize,
        packets: Arc<[Packet]>,
        num_decode_frame_workers: usize,
    ) -> anyhow::Result<VideoData> {
        assert!(num_decode_frame_workers > 0);

        let task_ring_buffer = Arc::new(ArrayQueue::new(num_decode_frame_workers));
        let (task_waker, task_listener) = crossbeam::channel::unbounded();
        let worker_handles: Box<[_]> = (0..num_decode_frame_workers)
            .map(|_| {
                let parameters = parameters.clone();
                let packets = packets.clone();
                let listener = task_listener.clone();
                let backlog = task_ring_buffer.clone();
                std::thread::spawn(move || {
                    decode_frame_base64_worker(parameters, &packets, listener, &backlog);
                })
            })
            .collect();

        let shape = {
            let decoder = codec::Context::from_parameters(parameters.clone())?
                .decoder()
                .video()?;
            (decoder.height(), decoder.width())
        };

        Ok(VideoData {
            parameters: Mutex::new(parameters),
            frame_rate,
            shape,
            packets,
            task_ring_buffer,
            task_waker,
            worker_handles,
        })
    }

    pub fn frame_rate(&self) -> usize {
        self.frame_rate
    }

    pub fn nframes(&self) -> usize {
        self.packets.len()
    }

    pub fn shape(&self) -> (u32, u32) {
        self.shape
    }

    /// `decode_frame_base64` is excluded from salsa database.
    /// `decode_frame_base64` is nondeterministic as whether decoding can succeed depends whether
    /// there is enough idle worker at the moment.
    /// Meanwhile, `decode_frame_base64` is not part of the overall computation but just for display.
    /// It can already yield the final output, there is no benefit to put it into salsa database.
    pub async fn decode_frame_base64(&self, frame_index: usize) -> anyhow::Result<String> {
        assert!(!self.worker_handles.iter().any(|h| h.is_finished()));
        let (tx, rx) = oneshot::channel();
        // When the old value which contains a tx is dropped, its corresponding
        // rx(which is awaiting) will be disconnected.
        self.task_ring_buffer
            .force_push(DecodeFrameTask { frame_index, tx });
        _ = self.task_waker.send(());
        rx.await?
    }

    pub fn decode_all(
        &self,
        start_frame: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
    ) -> anyhow::Result<Array2<u8>> {
        decode_all(&self.parameters, &self.packets, start_frame, cal_num, area)
    }
}

fn decode_frame_base64_worker(
    parameters: Parameters,
    packets: &[Packet],
    listener: Receiver<()>,
    backlog: &ArrayQueue<DecodeFrameTask>,
) {
    let mut decode_converter = DecodeConverter::new(parameters).unwrap();
    let mut buf = Vec::new();
    for _ in listener {
        if let Some(DecodeFrameTask { frame_index, tx }) = backlog.pop() {
            _ = tx.send(decode_frame_base64(
                &mut decode_converter,
                &mut buf,
                &packets[frame_index],
            ));
            buf.clear();
        }
    }
}

#[instrument(skip_all, err)]
fn decode_frame_base64(
    decode_converter: &mut DecodeConverter,
    mut buf: &mut Vec<u8>,
    packet: &Packet,
) -> anyhow::Result<String> {
    let (w, h) = (
        decode_converter.decoder.width(),
        decode_converter.decoder.height(),
    );
    let mut jpeg_encoder = JpegEncoder::new_with_quality(&mut buf, 100);
    let img = decode_converter.decode_convert(packet)?.data(0);
    jpeg_encoder.encode(img, w, h, Rgb8)?; // slowest
    Ok(general_purpose::STANDARD.encode(buf))
}

#[instrument(skip(parameters, packets), err)]
fn decode_all(
    parameters: &Mutex<Parameters>,
    packets: &[Packet],
    start_frame: usize,
    cal_num: usize,
    area: (u32, u32, u32, u32),
) -> anyhow::Result<Array2<u8>> {
    let (tl_y, tl_x, cal_h, cal_w) = area;
    let (tl_y, tl_x, cal_h, cal_w) = (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
    let green2 = Array2::zeros((cal_num, cal_h * cal_w));
    let cal_index = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..std::thread::available_parallelism().unwrap().get() {
            s.spawn(|| {
                let parameters = parameters.lock().unwrap().clone();
                let mut decode_converter = DecodeConverter::new(parameters).unwrap();
                let byte_w = decode_converter.decoder.width() as usize * 3;
                loop {
                    let cal_index = cal_index.fetch_add(1, Ordering::SeqCst);
                    if cal_index >= cal_num {
                        break;
                    }
                    let dst_frame = decode_converter
                        .decode_convert(&packets[start_frame + cal_index])
                        .unwrap();
                    // Each frame is stored in a u8 array:
                    // |r g b r g b...r g b|r g b r g b...r g b|......|r g b r g b...r g b|
                    // |.......row_0.......|.......row_1.......|......|.......row_n.......|
                    let rgb = dst_frame.data(0);
                    let mut ptr = green2.row(cal_index).as_ptr() as *mut u8;
                    for i in (0..).step_by(byte_w).skip(tl_y).take(cal_h) {
                        for j in (i..).skip(1).step_by(3).skip(tl_x).take(cal_w) {
                            unsafe {
                                *ptr = *rgb.get_unchecked(j);
                                ptr = ptr.add(1);
                            };
                        }
                    }
                }
            });
        }
    });
    Ok(green2)
}

/// DecodeConverter is bound to a specific video and can decode any packet of this video
/// and convert it into RGB24.
struct DecodeConverter {
    decoder: ffmpeg::decoder::Video,
    converter: scaling::Context,
    decoded_frame: Video,
    rgb_frame: Video,
}

impl DecodeConverter {
    fn new(parameters: Parameters) -> anyhow::Result<Self> {
        let decoder = codec::Context::from_parameters(parameters)?
            .decoder()
            .video()?;
        let (h, w) = (decoder.height(), decoder.width());
        let converter = ffmpeg::software::converter((w, h), decoder.format(), RGB24)?;
        Ok(Self {
            decoder,
            converter,
            decoded_frame: Video::empty(),
            rgb_frame: Video::empty(),
        })
    }

    fn decode_convert(&mut self, packet: &Packet) -> anyhow::Result<&Video> {
        self.decoder.send_packet(packet)?;
        self.decoder.receive_frame(&mut self.decoded_frame)?;
        self.converter
            .run(&self.decoded_frame, &mut self.rgb_frame)?;
        assert!(
            self.decoder.receive_frame(&mut self.decoded_frame).is_err(),
            "one packet should be decoded to one frame",
        );
        Ok(&self.rgb_frame)
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
        io::read_video,
        tests::{video_meta_real, video_meta_sample, VIDEO_PATH_REAL, VIDEO_PATH_SAMPLE},
        VideoData,
    };

    #[tokio::test]
    async fn test_decode_frame_sample() {
        _decode_frame(VIDEO_PATH_SAMPLE).await;
    }

    #[ignore]
    #[tokio::test]
    async fn test_decode_frame_real() {
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
        let (parameters, frame_rate, packets) = read_video(video_path).unwrap();
        let video_data =
            Arc::new(VideoData::new(parameters, frame_rate, packets.clone(), 20).unwrap());

        let mut handles = Vec::new();
        let ok_cnt = Arc::new(AtomicU32::new(0));
        let abort_cnt = Arc::new(AtomicU32::new(0));
        for i in 0..packets.len() {
            let decode_manager = video_data.clone();
            let ok_cnt = ok_cnt.clone();
            let abort_cnt = abort_cnt.clone();
            let handle = tokio::spawn(async move {
                match decode_manager.decode_frame_base64(i).await {
                    Ok(_) => ok_cnt.fetch_add(1, Relaxed),
                    Err(_) => abort_cnt.fetch_add(1, Relaxed),
                }
            });
            handles.push(handle);
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        for handle in handles {
            _ = handle.await;
        }

        dbg!(ok_cnt, abort_cnt);
    }

    fn _decode_all(video_path: &str, start_frame: usize, cal_num: usize) {
        let (parameters, frame_rate, packets) = read_video(video_path).unwrap();
        let video_data = VideoData::new(parameters, frame_rate, packets, 20).unwrap();
        video_data
            .decode_all(start_frame, cal_num, (10, 10, 600, 800))
            .unwrap();
    }
}
