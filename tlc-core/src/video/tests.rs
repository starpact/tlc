use std::{path::PathBuf, thread, time::Duration};

use salsa::ParallelDatabase;

use super::*;
use crate::util::log;

#[tokio::test]
async fn test_decode_frame() {
    log::init();

    let mut db = crate::Database::default();
    db.get_video_path().unwrap_err();
    let video_path = PathBuf::from("./testdata/almost_empty.avi");
    db.set_video_path(video_path.clone()).unwrap();
    assert_eq!(db.get_video_path().unwrap(), &video_path);

    println!("first");
    assert_eq!(db.get_video_nframes().unwrap(), 3);
    assert_eq!(db.get_video_frame_rate().unwrap(), 25);
    assert_eq!(db.get_video_shape().unwrap(), (1024, 1280));

    println!("no update");
    db.get_video_nframes().unwrap();

    println!("set");
    db.set_video_path(PathBuf::from("./testdata/almost_empty.avi"))
        .unwrap();
    db.get_video_nframes().unwrap();
}

#[test]
fn test_read_video_cancel() {
    log::init();

    let mut db = crate::Database::default();
    let video_path = PathBuf::from("./testdata/almost_empty.avi");
    db.set_video_path(video_path.clone()).unwrap();

    {
        let db = db.snapshot();
        thread::spawn(move || {
            let _span = tracing::debug_span!("will_be_canceled").entered();
            thread::sleep(Duration::from_millis(10));
            db.get_video_nframes().unwrap(); // panic here by salsa
            unreachable!();
        });
    }

    tracing::debug_span!("set_input")
        .in_scope(|| db.set_video_path(video_path))
        .unwrap();
    assert_eq!(db.get_video_nframes().unwrap(), 3);
}

pub const VIDEO_PATH_SAMPLE: &str = "./testdata/almost_empty.avi";
pub const VIDEO_PATH_REAL: &str = "/home/yhj/Downloads/EXP/imp/videos/imp_20000_1_up.avi";

pub(crate) fn video_meta_sample() -> VideoMeta {
    VideoMeta {
        frame_rate: 25,
        nframes: 3,
        shape: (1024, 1280),
    }
}

pub(crate) fn video_meta_real() -> VideoMeta {
    VideoMeta {
        frame_rate: 25,
        nframes: 2444,
        shape: (1024, 1280),
    }
}
