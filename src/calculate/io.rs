use ffmpeg_next as ffmpeg;

use ffmpeg::format::{input, Pixel};
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;

use calamine::{open_workbook, DataType, Reader, Xlsx};

use ndarray::prelude::*;

/// *read the video and collect all green values spatially and temporally*
/// ### Argument:
/// video record(start frame, frame num, video path)
///
/// region record((upper left y, upper left x), (calculate region height, calculate region width))
/// ### Return:
/// (green values 2D matrix, frame rate)
///
/// * pixels in rows, frames in columns, shape: (total_pix_num, frame_num)
/// ### Paincs
/// ffmpeg errors
pub fn read_video(
    video_record: (usize, usize, &String),
    region_record: ((usize, usize), (usize, usize)),
) -> Result<(Array2<u8>, usize), ffmpeg::Error> {
    ffmpeg::init().expect("ffmpeg failed to initialize");

    let (start_frame, frame_num, video_path) = video_record;
    let mut ictx = input(video_path)?;
    let mut decoder = ictx.stream(0).unwrap().codec().decoder().video()?;

    let rational = decoder.frame_rate().unwrap();
    let frame_rate = (rational.numerator() / rational.denominator()) as usize;
    let total_frame = ictx.duration() as usize * frame_rate / 1_000_000;

    if start_frame + frame_num >= total_frame {
        return Err(ffmpeg::Error::InvalidData);
    }

    // upper_left_coordinate
    let (ul_y, ul_x) = region_record.0;
    // height and width of calculation region
    let (cal_h, cal_w) = region_record.1;
    // total number of pixels in the calculation region
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

    // g2d stores green values of all pixels at all frames in a 2D array: single row for all pixels at single frame
    let mut g2d = Array2::zeros((frame_num, pix_num));
    let real_w = decoder.width() as usize * 3;

    for ((_, packet), mut row) in ictx
        .packets()
        .skip(start_frame)
        .zip(g2d.axis_iter_mut(Axis(0)))
    {
        decoder.send_packet(&packet)?;
        let (mut raw_frame, mut rgb_frame) = (Video::empty(), Video::empty());
        decoder.receive_frame(&mut raw_frame)?;
        scaler.run(&raw_frame, &mut rgb_frame)?;
        // the data of each frame stores in one 1D array:
        // ||rgbrgbrgb...rgb|rgbrgbrgb...rgb|......|rgbrgbrgb...rgb||
        // ||.....row_0.....|.....row_1.....|......|.....row_n.....||
        let rgb = rgb_frame.data(0);

        let mut iter = row.iter_mut();
        for i in (0..).step_by(real_w).skip(ul_y).take(cal_h) {
            for j in (i..).skip(1).step_by(3).skip(ul_x).take(cal_w) {
                *(iter.next().unwrap()) = rgb[j];
            }
        }
    }

    Ok((g2d, frame_rate))
}

/// *read temperature data from excel*
/// ### Argument:
/// temperature record(start line number, total frame number, column numbers that record the temperatures, excel_path)
/// ### Return:
/// 2D matrix of the delta temperatures between adjacent frames
pub fn read_temp_excel(
    temp_record: (usize, usize, &[usize], &String),
) -> Result<Array2<f64>, calamine::Error> {
    let (start_line, frame_num, columns, temp_path) = temp_record;
    let mut excel: Xlsx<_> = open_workbook(temp_path).unwrap();
    let sheet = excel.worksheet_range_at(0).expect("no sheet exsits")?;

    let mut t2d = Array2::zeros((frame_num, columns.len()));
    let mut fst = true;

    for ((excel_row0, excel_row1), mut temp_row) in sheet
        .rows()
        .skip(start_line)
        .take(frame_num)
        .zip(sheet.rows().skip(start_line + 1).take(frame_num))
        .zip(t2d.axis_iter_mut(Axis(0)))
    {
        for (&index, t) in columns.iter().zip(temp_row.iter_mut()) {
            match (&excel_row0[index], &excel_row1[index]) {
                (&DataType::Float(t0), &DataType::Float(t1)) => *t = if fst { t0 } else { t1 - t0 },
                _ => {
                    return Err(calamine::Error::Msg("temperatures not as floats"));
                }
            }
        }
        fst = false;
    }

    Ok(t2d)
}
