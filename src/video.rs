mod detect_peak;

use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use anyhow::anyhow;
use crossbeam::{
    channel::{Receiver, Sender},
    queue::ArrayQueue,
};
pub use ffmpeg::codec::{packet::Packet, Parameters};
use ffmpeg::{codec, format::Pixel::RGB24, software::scaling, util::frame::video::Video};
use ndarray::ArcArray2;
use serde::Serialize;
use tracing::{info_span, instrument};

pub use detect_peak::{filter_detect_peak, filter_point, FilterMethod};

pub fn init() {
    ffmpeg::init().expect("failed to init ffmpeg");
}

#[derive(Debug, Serialize, Clone, Copy)]
pub struct VideoMeta {
    pub frame_rate: usize,
    pub nframes: usize,
    /// (video_height, video_width)
    pub shape: (u32, u32),
}

#[derive(Debug, Clone)]
pub struct VideoData {
    inner: Arc<Inner>,
}

#[instrument(fields(video_path=?video_path.as_ref()), err)]
pub fn read_video<P: AsRef<Path>>(video_path: P) -> anyhow::Result<VideoData> {
    let video_path = video_path.as_ref().to_owned();
    let mut input = ffmpeg::format::input(&video_path)?;
    let video_stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("video stream not found"))?;
    let video_stream_index = video_stream.index();
    let nframes = video_stream.frames() as usize;
    let parameters = video_stream.parameters();
    let frame_rate = {
        let rational = video_stream.avg_frame_rate();
        (rational.0 as f64 / rational.1 as f64).round() as usize
    };
    let packets: Box<[_]> = input
        .packets()
        .filter_map(|(stream, packet)| (stream.index() == video_stream_index).then_some(packet))
        .collect();
    assert_eq!(nframes, packets.len());
    let video_data = VideoData::new(parameters, frame_rate, packets, 4)?;
    Ok(video_data)
}

struct Inner {
    parameters: Mutex<Parameters>,
    frame_rate: usize,
    shape: (u32, u32),
    packets: Box<[Packet]>,
    /// When user drags the progress bar quickly, the decoding can not keep up and
    /// there will be a significant lag. However, we actually do not have to decode
    /// every frames, and the key is how to give up decoding some frames properly.
    /// The naive solution to avoid too much backlog is maintaining the number of
    /// pending tasks and directly abort current decoding if it already exceeds the
    /// limit. But FIFO is not perfect for this use case because it's better to give
    /// priority to newer frames, e.g. we should at least guarantee decoding the frame
    /// where the progress bar **stops**.
    /// `task_ring_buffer` is a ring buffer that only stores the most recent tasks.
    task_ring_buffer: ArrayQueue<(usize, usize)>,
    task_dispatcher: Sender<()>,
    decoded_frame_slot: Mutex<Option<(Vec<u8>, usize)>>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoData")
            .field("frame_rate", &self.frame_rate)
            .field("shape", &self.shape)
            .field("npackets", &self.packets.len())
            .finish()
    }
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

impl VideoData {
    pub fn new(
        parameters: Parameters,
        frame_rate: usize,
        packets: Box<[Packet]>,
        num_decode_frame_workers: usize,
    ) -> anyhow::Result<VideoData> {
        assert!(num_decode_frame_workers > 0);

        let task_ring_buffer = ArrayQueue::new(num_decode_frame_workers);
        let (task_dispatcher, task_listener) =
            crossbeam::channel::bounded(num_decode_frame_workers);
        let decoded_frame_slot = Mutex::new(None);

        let shape = {
            let decoder = codec::Context::from_parameters(parameters.clone())?
                .decoder()
                .video()?;
            (decoder.height(), decoder.width())
        };

        let video_data = VideoData {
            inner: Arc::new(Inner {
                parameters: Mutex::new(parameters),
                frame_rate,
                shape,
                packets,
                task_ring_buffer,
                task_dispatcher,
                decoded_frame_slot,
            }),
        };
        video_data.spawn_decode_workers(task_listener, num_decode_frame_workers);

        Ok(video_data)
    }

    pub fn frame_rate(&self) -> usize {
        self.inner.frame_rate
    }

    pub fn nframes(&self) -> usize {
        self.inner.packets.len()
    }

    pub fn shape(&self) -> (u32, u32) {
        self.inner.shape
    }

    pub fn decode_one(&self, frame_index: usize, serial_num: usize) {
        self.inner
            .task_ring_buffer
            .force_push((frame_index, serial_num));
        _ = self.inner.task_dispatcher.try_send(());
    }

    pub fn take_decoded_frame(&self) -> Option<(Vec<u8>, usize)> {
        self.inner.decoded_frame_slot.lock().unwrap().take()
    }

    #[instrument(skip(self), err)]
    pub fn decode_range_area(
        &self,
        start_frame: usize,
        cal_num: usize,
        area: (u32, u32, u32, u32),
    ) -> anyhow::Result<ArcArray2<u8>> {
        let (tl_y, tl_x, cal_h, cal_w) = area;
        let (tl_y, tl_x, cal_h, cal_w) =
            (tl_y as usize, tl_x as usize, cal_h as usize, cal_w as usize);
        let green2 = ArcArray2::zeros((cal_num, cal_h * cal_w));
        let cal_index = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..std::thread::available_parallelism().unwrap().get() {
                s.spawn(|| {
                    let parameters = self.inner.parameters.lock().unwrap().clone();
                    let mut decode_converter = DecodeConverter::new(parameters).unwrap();
                    let byte_w = decode_converter.decoder.width() as usize * 3;
                    loop {
                        let cal_index = cal_index.fetch_add(1, Ordering::SeqCst);
                        if cal_index >= cal_num {
                            break;
                        }
                        let dst_frame = decode_converter
                            .decode_convert(&self.inner.packets[start_frame + cal_index])
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

    fn spawn_decode_workers(&self, task_listener: Receiver<()>, num_decode_frame_workers: usize) {
        for _ in 0..num_decode_frame_workers {
            let video_data = self.inner.clone();
            let task_listener = task_listener.clone();
            std::thread::spawn(move || {
                let mut decode_converter =
                    DecodeConverter::new(video_data.parameters.lock().unwrap().clone()).unwrap();
                for _ in task_listener {
                    if let Some((frame_index, serial_num)) = video_data.task_ring_buffer.pop() {
                        let _span = info_span!("decode_one", frame_index, serial_num).entered();
                        if let Ok(decoded_frame) =
                            decode_converter.decode_convert(&video_data.packets[frame_index])
                        {
                            *video_data.decoded_frame_slot.lock().unwrap() =
                                Some((decoded_frame.data(0).to_vec(), serial_num));
                        }
                    }
                }
            });
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_read_video_sample() {
        read_video1(VIDEO_PATH_SAMPLE, video_meta_sample());
    }

    #[ignore]
    #[test]
    fn test_read_video_real() {
        read_video1(VIDEO_PATH_REAL, video_meta_real());
    }

    fn read_video1(video_path: &str, expected_video_meta: VideoMeta) {
        let video_data = super::read_video(video_path).unwrap();
        assert_eq!(video_data.frame_rate(), expected_video_meta.frame_rate);
        let mut cnt = 0;
        for packet in &*video_data.inner.packets {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, expected_video_meta.nframes);
    }

    #[test]
    fn test_decode_range_sample() {
        decode_range1(VIDEO_PATH_SAMPLE, 0, video_meta_sample().nframes);
    }

    #[ignore]
    #[test]
    fn test_decode_range_real() {
        decode_range1(VIDEO_PATH_REAL, 10, video_meta_real().nframes - 10);
    }

    fn decode_range1(video_path: &str, start_frame: usize, cal_num: usize) {
        let video_data = read_video(video_path).unwrap();
        video_data
            .decode_range_area(start_frame, cal_num, (10, 10, 600, 800))
            .unwrap();
    }

    pub const VIDEO_PATH_SAMPLE: &str = "./testdata/almost_empty.avi";
    pub const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";

    fn video_meta_sample() -> VideoMeta {
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
