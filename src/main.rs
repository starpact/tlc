#![allow(dead_code, unused_imports)]

use ffmpeg::format::{input, Pixel};
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;
use ffmpeg_next as ffmpeg;

use ndarray::prelude::*;

fn main() {
    const PATH: &str = "./resource/test.avi";
    const START_FRAME: usize = 0;
    const FRAME_NUM: usize = 2000;
    const UPPER_LEFT_COORD: (usize, usize) = (100, 200);
    const REGION_SHAPE: (usize, usize) = (800, 1000);

    let frame_record = (START_FRAME, FRAME_NUM, &PATH.to_string());
    let region_record = (UPPER_LEFT_COORD, REGION_SHAPE);

    let t0 = std::time::Instant::now();

    let (green_history, frame_rate) = match read_video(frame_record, region_record) {
        Ok(res) => res,
        Err(ffmpeg::Error::InvalidData) => panic!("please check your frame settings"),
        Err(err) => panic!("{}", err),
    };

    println!("{:?}", std::time::Instant::now().duration_since(t0));
    println!("{}", frame_rate);
    println!("{}", green_history.row(0)[400000]);
    println!("{}", green_history.row(500)[400000]);
    println!("{}", green_history.row(1000)[400000]);
    println!("{}", green_history.row(1500)[400000]);
}

/// ### Argument:
/// frame_record(start frame, frame num, video path)
/// ### Return:
/// (green values 2D matrix, frame rate)
///
/// * pixels in rows, frames in columns, shape: (total_pix_num, frame_num)
/// ### Paincs
/// ffmpeg errors
fn read_video(
    frame_record: (usize, usize, &String),
    region_record: ((usize, usize), (usize, usize)),
) -> Result<(Array2<u8>, usize), ffmpeg::Error> {
    ffmpeg::init().expect("ffmpeg failed to initialize");

    let (start_frame, frame_num, video_path) = frame_record;

    let mut ictx = input(video_path)?;
    let mut decoder = ictx.stream(0).unwrap().codec().decoder().video()?;

    let rational = decoder.frame_rate().unwrap();
    let frame_rate = (rational.numerator() / rational.denominator()) as usize;
    let total_frame = ictx.duration() as usize * frame_rate / 1_000_000;

    if start_frame + frame_num >= total_frame {
        return Err(ffmpeg::Error::InvalidData);
    }

    let (ul_y, ul_x) = region_record.0;
    let (cal_h, cal_w) = region_record.1;
    let pix_num = cal_h * cal_w;

    // Target color space: RGB24, 8 bits respectively for R, G and B
    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        decoder.width(),
        decoder.height(),
        Flags::FAST_BILINEAR,
    )?;

    // g2d stores green values of all pixels at all frame in a 2D array: single row for all pixels at single frame
    let mut g2d = Array2::zeros((frame_num, pix_num));
    let real_w = decoder.width() as usize * 3;

    for (frame_index, (_, packet)) in (0..frame_num).zip(ictx.packets().skip(start_frame)) {
        decoder.send_packet(&packet)?;
        let (mut raw_frame, mut rgb_frame) = (Video::empty(), Video::empty());
        decoder.receive_frame(&mut raw_frame)?;
        scaler.run(&raw_frame, &mut rgb_frame)?;
        let rgb = rgb_frame.data(0);

        let mut row = g2d.row_mut(frame_index);
        let mut iter = row.iter_mut();
        for i in (0..).step_by(real_w).skip(ul_y).take(cal_h) {
            for j in (i..).skip(1).step_by(3).skip(ul_x).take(cal_w) {
                *(iter.next().unwrap()) = rgb[j];
            }
        }
    }

    Ok((g2d, frame_rate))
}

#[test]
fn test() {
    let mut arr = array![[0, 1, 2, 3], [4, 5, 6, 7]];
    for mut row in arr.axis_iter_mut(Axis(0)) {
        row[0] = 100;
    }

    println!("{}", arr);
}
