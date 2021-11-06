use std::path::{Path, PathBuf};

use ffmpeg_next as ffmpeg;

use anyhow::{anyhow, Result};
use ffmpeg::format::Pixel;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{Context, Flags};
use ffmpeg::util::frame;
use serde::Serialize;
use tracing::debug;

#[derive(Debug, Default)]
pub struct FrameCache {
    /// Identifies the current video.
    pub path: Option<PathBuf>,
    /// Total frame number of the current video.
    /// This is used to validate the `frame_index` parameter of `get_frame`.
    pub total_frames: usize,
    /// Frame data.
    pub frames: Vec<usize>,
}

impl FrameCache {
    pub fn reset<P: AsRef<Path>>(&mut self, path: P, total_frames: usize) {
        self.path = Some(path.as_ref().to_owned());
        self.total_frames = total_frames;
        // Actual frame data will be dropped.
        // Allocated capacity of `frames` won't change.
        self.frames.clear();
    }

    pub fn path_changed<P: AsRef<Path>>(&self, path: P) -> bool {
        let old = match self.path {
            Some(ref path) => path,
            None => return true,
        };
        let new = path.as_ref();

        old != new
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct VideoInfo {
    pub frame_rate: usize,
    pub total_frames: usize,
    pub shape: (usize, usize),
}

pub fn get_video_info<P: AsRef<Path>>(_path: P) -> Result<VideoInfo> {
    let video_info = VideoInfo {
        frame_rate: 25,
        total_frames: 1000,
        shape: (500, 500),
    };

    debug!("{:#?}", video_info);

    Ok(video_info)
}

pub fn print_video_frame_info<P: AsRef<Path>>(path: P) -> Result<()> {
    ffmpeg::init().unwrap();

    let mut input = ffmpeg::format::input(&path)?;
    let video_stream = input
        .streams()
        .best(Type::Video)
        .ok_or(anyhow!("video stream not found"))?;
    let video_stream_index = video_stream.index();

    let mut decoder = video_stream.codec().decoder().video()?;

    let mut scale_ctx = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::BILINEAR,
    )?;

    let mut frame_index = 0;

    for (stream, packet) in input.packets() {
        if stream.index() == video_stream_index {
            decoder.send_packet(&packet)?;
            handle_frame(&mut decoder, &mut scale_ctx, &mut frame_index)?;
        }
    }

    decoder.send_eof()?;
    handle_frame(&mut decoder, &mut scale_ctx, &mut frame_index)?;

    Ok(())
}

fn handle_frame(
    decoder: &mut ffmpeg::decoder::Video,
    scale_ctx: &mut Context,
    frame_index: &mut usize,
) -> Result<()> {
    let mut decoded = frame::video::Video::empty();
    while decoder.receive_frame(&mut decoded).is_ok() {
        let mut rgb_frame = frame::video::Video::empty();
        scale_ctx.run(&decoded, &mut rgb_frame)?;
        debug!("pts: {}", rgb_frame.pts().unwrap());
        *frame_index += 1;
    }

    Ok(())
}
