use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, Result};
use crossbeam::channel::Sender;
use ffmpeg::codec;
pub use ffmpeg::codec::{packet::Packet, Parameters};
use tokio::sync::oneshot;
use tracing::{info_span, instrument};

use crate::{ProgressBar, VideoMeta};

pub(crate) enum Packets {
    /// Used when packets are being loaded gradually.
    InProgress(Vec<Packet>),
    /// After finished loading all packets, `Packets` becomes immutable and can be shared
    /// with other thread cheaply.
    Finished(Arc<Vec<Packet>>),
}

/// `progress_bar` can be used to observe the progress of `read_video` and cancel it.
/// `meta_tx` will be sent video metadata very quickly.
/// `packet_tx` will be sent all video packets in order.
#[instrument(
    skip(progress_bar, meta_tx, packet_tx),
    fields(video_path = video_path.as_ref().to_str().unwrap()),
    err
)]
pub fn read_video<P: AsRef<Path>>(
    video_path: P,
    progress_bar: ProgressBar,
    meta_tx: oneshot::Sender<(VideoMeta, Parameters)>,
    packet_tx: Sender<(Arc<PathBuf>, Packet)>,
) -> Result<()> {
    let video_path = video_path.as_ref();

    let _span1 = info_span!("read_video_meta").entered();
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
    let shape = (decoder.height() as usize, decoder.width() as usize);

    let video_meta = VideoMeta {
        path: video_path.to_owned(),
        frame_rate,
        nframes,
        shape,
    };
    meta_tx
        .send((video_meta, parameters))
        .map_err(|_| ())
        .unwrap();
    drop(_span1);

    progress_bar.start(nframes as u32)?;

    let video_path = Arc::new(video_path.to_owned());
    let _span2 = info_span!("load_packets", frame_rate, nframes).entered();
    input
        .packets()
        .filter_map(|(stream, packet)| (stream.index() == video_stream_index).then_some(packet))
        .try_for_each(|packet| -> Result<()> {
            progress_bar.add(1)?;
            packet_tx.send((video_path.clone(), packet))?;
            Ok(())
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::thread::spawn;

    use crossbeam::channel::bounded;

    use crate::init_trace;

    use super::*;

    const VIDEO_PATH_SAMPLE: &str = "../tests/almost_empty.avi";
    const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";
    fn _video_meta_sample() -> VideoMeta {
        VideoMeta {
            path: PathBuf::from(VIDEO_PATH_SAMPLE),
            frame_rate: 25,
            nframes: 3,
            shape: (1024, 1280),
        }
    }
    fn _video_meta_real() -> VideoMeta {
        VideoMeta {
            path: PathBuf::from(VIDEO_PATH_REAL),
            frame_rate: 25,
            nframes: 2444,
            shape: (1024, 1280),
        }
    }

    #[test]
    fn test_read_video_sample() {
        init_trace();

        let progress_bar = ProgressBar::default();
        let (meta_tx, meta_rx) = oneshot::channel();
        let (packet_tx, packet_rx) = bounded(3);
        spawn(move || read_video(VIDEO_PATH_SAMPLE, progress_bar, meta_tx, packet_tx).unwrap());

        let (video_meta, _) = meta_rx.blocking_recv().unwrap();
        let video_meta_sample = _video_meta_sample();
        assert_eq!(video_meta, video_meta_sample,);
        let mut cnt = 0;
        for (_, packet) in packet_rx {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, video_meta_sample.nframes);
    }

    #[ignore]
    #[test]
    fn test_read_video_real() {
        init_trace();

        let progress_bar = ProgressBar::default();
        let (meta_tx, meta_rx) = oneshot::channel();
        let (packet_tx, packet_rx) = bounded(3);
        spawn(move || read_video(VIDEO_PATH_REAL, progress_bar, meta_tx, packet_tx).unwrap());

        let (video_meta, _) = meta_rx.blocking_recv().unwrap();
        let video_meta_real = _video_meta_real();
        assert_eq!(video_meta, video_meta_real);
        let mut cnt = 0;
        for (_, packet) in packet_rx {
            assert_eq!(packet.dts(), Some(cnt as i64));
            cnt += 1;
        }
        assert_eq!(cnt, video_meta_real.nframes);
    }
}
