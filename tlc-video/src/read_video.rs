use std::path::Path;

use anyhow::{anyhow, Result};
use crossbeam::channel::{bounded, Receiver};
use ffmpeg::codec;
pub use ffmpeg::codec::{packet::Packet, Parameters};
use tracing::{error, info_span, instrument};

use crate::{ProgressBar, VideoMeta};

/// `read_video` will return after finished reading video metadata, which just takes tens of
/// milliseconds. Then packets can be received from the returned channel asynchronously.
/// `progress_bar` can be used to observe the progress of `read_video` and cancel it.
#[instrument(skip(progress_bar), fields(video_path), err)]
pub fn read_video<P: AsRef<Path>>(
    video_path: P,
    progress_bar: ProgressBar,
) -> Result<(VideoMeta, Parameters, Receiver<Packet>)> {
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

    progress_bar.start(nframes as u32)?;
    let (tx, rx) = bounded(3); // cap doesn't really matter
    let video_meta = VideoMeta {
        frame_rate,
        nframes,
        shape,
    };

    std::thread::spawn(move || {
        let _span = info_span!("load_packets", frame_rate, nframes).entered();
        if let Err(e) = input
            .packets()
            .filter_map(|(stream, packet)| (stream.index() == video_stream_index).then_some(packet))
            .try_for_each(|packet| -> Result<()> {
                progress_bar.add(1)?;
                tx.send(packet)?;
                Ok(())
            })
        {
            error!(%e, "failed to load packets");
        }
    });

    Ok((video_meta, parameters, rx))
}

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use crate::{
        test_util::{video_meta_real, video_meta_sample, VIDEO_PATH_REAL, VIDEO_PATH_SAMPLE},
        VideoController,
    };

    use super::*;

    #[test]
    fn test_read_video_sample() {
        tlc_util::log::init();
        _read_video(VIDEO_PATH_SAMPLE, video_meta_sample());
    }

    #[ignore]
    #[test]
    fn test_read_video_real() {
        tlc_util::log::init();
        _read_video(VIDEO_PATH_REAL, video_meta_real());
    }

    #[test]
    fn test_cancel_before_start_sample() {
        tlc_util::log::init();
        let mut video_controller = VideoController::default();
        let progress_bar = video_controller.prepare_read_video();
        // Cancel the previous one.
        video_controller.prepare_read_video();
        assert!(read_video(VIDEO_PATH_SAMPLE, progress_bar).is_err());
    }

    #[ignore]
    #[test]
    fn test_cancel_while_reading_real() {
        tlc_util::log::init();
        let mut video_controller = VideoController::default();
        let progress_bar = video_controller.prepare_read_video();
        let (_, _, packet_rx) = read_video(VIDEO_PATH_REAL, progress_bar).unwrap();
        std::thread::spawn(move || {
            sleep(Duration::from_millis(20));
            video_controller.prepare_read_video();
        });
        let cnt = packet_rx.into_iter().count();
        dbg!(cnt);
    }

    fn _read_video(video_path: &str, expected_video_meta: VideoMeta) {
        let progress_bar = ProgressBar::default();
        let (video_meta, _, packet_rx) = read_video(video_path, progress_bar).unwrap();
        assert_eq!(video_meta.frame_rate, expected_video_meta.frame_rate);
        assert_eq!(video_meta.shape, expected_video_meta.shape);
        assert_eq!(video_meta.nframes, expected_video_meta.nframes);
        let mut cnt = 0;
        for packet in packet_rx {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, expected_video_meta.nframes);
    }
}
