use std::{path::Path, sync::Arc};

use anyhow::anyhow;
pub use ffmpeg::codec::{packet::Packet, Parameters};
use tracing::instrument;

#[instrument(fields(video_path), err)]
pub(crate) fn read_video<P: AsRef<Path>>(
    video_path: P,
) -> anyhow::Result<(Parameters, usize, Arc<[Packet]>)> {
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
    let packets: Arc<[_]> = input
        .packets()
        .filter_map(|(stream, packet)| (stream.index() == video_stream_index).then_some(packet))
        .collect();
    assert_eq!(nframes, packets.len());
    Ok((parameters, frame_rate, packets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::{
        tests::{video_meta_real, video_meta_sample, VIDEO_PATH_REAL, VIDEO_PATH_SAMPLE},
        VideoMeta,
    };

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
        let (_, frame_rate, packets) = read_video(video_path).unwrap();
        assert_eq!(frame_rate, expected_video_meta.frame_rate);
        let mut cnt = 0;
        for packet in &*packets {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, expected_video_meta.nframes);
    }
}
