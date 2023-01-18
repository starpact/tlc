use std::path::Path;

use anyhow::{anyhow, Result};
use ffmpeg::codec;
pub use ffmpeg::codec::{packet::Packet, Parameters};
use tracing::instrument;

use crate::VideoMeta;

/// `read_video` will return after finished reading video metadata, which just takes
/// several milliseconds. Then packets can be received from the returned channel
/// asynchronously.
/// `progress_bar` can be used to observe the progress and cancel it.
#[instrument(fields(video_path), err)]
pub fn read_video<P: AsRef<Path>>(video_path: P) -> Result<(VideoMeta, Parameters, Vec<Packet>)> {
    let video_path = video_path.as_ref().to_owned();
    let mut input = ffmpeg::format::input(&video_path)?;
    let video_stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("video stream not found"))?;
    let video_stream_index = video_stream.index();
    let parameters = video_stream.parameters();
    let codec_ctx = codec::Context::from_parameters(parameters.clone())?;
    let rational = video_stream.avg_frame_rate();
    let frame_rate = (rational.0 as f64 / rational.1 as f64).round() as usize;
    let nframes = video_stream.frames() as usize;
    let decoder = codec_ctx.decoder().video()?;
    let shape = (decoder.height(), decoder.width());
    let video_meta = VideoMeta {
        frame_rate,
        nframes,
        shape,
    };
    let packets: Vec<_> = input
        .packets()
        .filter_map(|(stream, packet)| (stream.index() == video_stream_index).then_some(packet))
        .collect();
    debug_assert_eq!(nframes, packets.len());
    Ok((video_meta, parameters, packets))
}

#[cfg(test)]
mod tests {
    use crate::video::tests::{
        video_meta_real, video_meta_sample, VIDEO_PATH_REAL, VIDEO_PATH_SAMPLE,
    };

    use super::*;

    #[test]
    fn test_read_video_sample() {
        _read_video(VIDEO_PATH_SAMPLE, video_meta_sample());
    }

    #[ignore]
    #[test]
    fn test_read_video_real() {
        _read_video(VIDEO_PATH_REAL, video_meta_real());
    }

    fn _read_video(video_path: &str, expected_video_meta: VideoMeta) {
        let (video_meta, _, packets) = read_video(video_path).unwrap();
        assert_eq!(video_meta.frame_rate, expected_video_meta.frame_rate);
        assert_eq!(video_meta.shape, expected_video_meta.shape);
        assert_eq!(video_meta.nframes, expected_video_meta.nframes);
        let mut cnt = 0;
        for packet in packets {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, expected_video_meta.nframes);
    }
}
