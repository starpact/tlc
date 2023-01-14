use std::{path::PathBuf, thread, time::Duration};

use salsa::ParallelDatabase;

use crate::log;

#[test]
fn test_decode_frame() {
    log::init();

    let mut db = crate::Database::default();
    assert!(db.get_video_path().is_none());
    let video_path = PathBuf::from("./testdata/almost_empty.avi");
    db.set_video_path(video_path.clone());
    assert_eq!(db.get_video_path().unwrap(), video_path);

    println!("first");
    assert_eq!(db.get_video_nframes().unwrap(), 3);
    assert_eq!(db.get_video_frame_rate().unwrap(), 25);
    assert_eq!(db.get_video_shape().unwrap(), (1024, 1280));

    println!("no update");
    db.get_video_nframes().unwrap();

    println!("set");
    db.set_video_path(PathBuf::from("./testdata/almost_empty.avi"));
    db.get_video_nframes().unwrap();

    (0..3)
        .map(|i| {
            let db = db.snapshot();
            thread::spawn(move || println!("{i}---------{}", db.decode_frame(i).is_ok()))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|handle| handle.join().unwrap());
}

#[test]
fn test_read_video_cancel() {
    log::init();

    let mut db = crate::Database::default();
    let video_path = PathBuf::from("./testdata/almost_empty.avi");
    db.set_video_path(video_path.clone());

    {
        let db = db.snapshot();
        thread::spawn(move || {
            let _span = tracing::debug_span!("will_be_canceled").entered();
            thread::sleep(Duration::from_millis(10));
            db.get_video_nframes().unwrap(); // panic here by salsa
            unreachable!();
        });
    }

    tracing::debug_span!("set_input").in_scope(|| db.set_video_path(video_path));
    assert_eq!(db.get_video_nframes().unwrap(), 3);
}
