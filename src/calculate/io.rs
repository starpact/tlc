use ndarray::prelude::*;
use std::path::Path;

use ffmpeg_next as ffmpeg;

use ffmpeg::format::{input, Pixel};
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;

/// *read the video and collect all green values spatially and temporally*
/// ### Argument:
/// video record(start frame, frame num, video path)
///
/// region record((upper left y, upper left x), (calculate region height, calculate region width))
/// ### Return:
/// (green values 2D matrix, frame rate)
///
/// * pixels in rows, frames in columns, shape: (total_pix_num, frame_num)
/// ### Panics
/// ffmpeg errors
pub fn read_video<P: AsRef<Path>>(
    video_record: (usize, usize, P),
    region_record: ((usize, usize), (usize, usize)),
) -> Result<(Array2<u8>, usize), ffmpeg::Error> {
    ffmpeg::init().expect("ffmpeg failed to initialize");

    let (start_frame, frame_num, video_path) = video_record;
    let mut ictx = input(&video_path)?;
    let mut decoder = ictx.stream(0).unwrap().codec().decoder().video()?;

    let rational = decoder.frame_rate().unwrap();
    let frame_rate = (rational.numerator() as f64 / rational.denominator() as f64).round() as usize;
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
        // ||r g b r g b...r g b|......|r g b r g b...r g b||
        // ||.......row_0.......|......|.......row_n.......||
        let rgb = rgb_frame.data(0);

        let mut iter = row.iter_mut();
        for i in (0..).step_by(real_w).skip(ul_y).take(cal_h) {
            for j in (i..).skip(1).step_by(3).skip(ul_x).take(cal_w) {
                *iter.next().unwrap() = rgb[j];
            }
        }
    }

    Ok((g2d, frame_rate))
}

use calamine::{open_workbook, DataType, Reader, Xlsx};

/// *read temperature data from excel*
/// ### Argument:
/// temperature record(start line number, total frame number, column numbers that record the temperatures, excel_path)
/// ### Return:
/// 2D matrix of the temperatures from thermocouples
pub fn read_temp_excel<P: AsRef<Path>>(
    temp_record: (usize, usize, Vec<usize>, P),
) -> Result<Array2<f64>, calamine::Error> {
    let (start_line, frame_num, columns, temp_path) = temp_record;
    let mut excel: Xlsx<_> = open_workbook(temp_path).unwrap();
    let sheet = excel.worksheet_range_at(0).expect("no sheet exists")?;

    let mut t2d = Array2::zeros((frame_num, columns.len()));

    for (excel_row, mut temp_row) in sheet
        .rows()
        .skip(start_line)
        .take(frame_num)
        .zip(t2d.axis_iter_mut(Axis(0)))
    {
        for (&index, t) in columns.iter().zip(temp_row.iter_mut()) {
            match excel_row[index] {
                DataType::Float(t0) => *t = t0,
                _ => {
                    return Err(calamine::Error::Msg("temperatures not as floats"));
                }
            }
        }
    }

    Ok(t2d)
}

use serde::{Serialize, Deserialize};
use serde_json;

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use super::preprocess::{InterpMethod, FilterMethod};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigParas {
    pub video_path: String,
    pub excel_path: String,
    pub start_frame: usize,
    pub start_line: usize,
    pub frame_num: usize,
    pub upper_left_pos: (usize, usize),
    pub region_shape: (usize, usize),
    pub temp_column_num: Vec<usize>,
    pub thermocouple_pos: Vec<(i32, i32)>,
    pub interp_method: InterpMethod,
    pub filter_method: FilterMethod,
    pub peak_temp: f64,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub h0: f64,
    pub max_iter_num: usize,
}

pub fn read_config<P: AsRef<Path>>(config_path: P) -> Result<ConfigParas, Box<dyn Error>> {
    let file = File::open(config_path)?;
    let reader = BufReader::new(file);
    let c = serde_json::from_reader(reader)?;
    Ok(c)
}
