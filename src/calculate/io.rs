use ndarray::prelude::*;
use std::error::Error;
use std::path::Path;

pub fn get_metadata<P: AsRef<Path>>(
    video_path: P,
    daq_path: P,
    start_frame: usize,
    start_row: usize,
) -> Result<(usize, usize, usize, usize), Box<dyn Error>> {
    let (total_frames, frame_rate) = get_frames_of_video(video_path)?;
    let total_rows = get_rows_of_daq(daq_path)?;
    let frame_num = (total_frames - start_frame).min(total_rows - start_row) - 1;
    Ok((frame_num, frame_rate, total_frames, total_rows))
}

use ffmpeg_next as ffmpeg;

use ffmpeg::format::{input, Pixel};
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;

fn get_frames_of_video<P: AsRef<Path>>(video_path: P) -> Result<(usize, usize), Box<dyn Error>> {
    ffmpeg::init()?;

    let ictx = input(&video_path)?;
    let decoder = ictx
        .stream(0)
        .ok_or("wrong video file")?
        .codec()
        .decoder()
        .video()?;
    let rational = decoder.frame_rate().ok_or("")?;
    let frame_rate = (rational.numerator() as f64 / rational.denominator() as f64).round() as usize;
    let total_frame = ictx.duration() as usize * frame_rate / 1_000_000;
    Ok((total_frame, frame_rate))
}

use calamine::{open_workbook, Reader, Xlsx};

fn get_rows_of_daq<P: AsRef<Path>>(daq_path: P) -> Result<usize, Box<dyn Error>> {
    use std::io::{Error, ErrorKind::InvalidData};
    match daq_path.as_ref().extension() {
        Some(ext) if "lvm".eq(ext) => {
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_path(daq_path)?;
            Ok(rdr.records().count())
        }
        Some(ext) if "xlsx".eq(ext) => {
            let mut excel: Xlsx<_> = open_workbook(daq_path)?;
            let sheet = excel.worksheet_range_at(0).ok_or("no sheet exists")??;
            Ok(sheet.height())
        }
        _ => Err(Box::new(Error::new(
            InvalidData,
            "only .lvm or .xlsx supported",
        ))),
    }
}

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
) -> Result<Array2<u8>, Box<dyn Error>> {
    ffmpeg::init()?;
    // .expect("ffmpeg failed to initialize");

    let (start_frame, frame_num, video_path) = video_record;
    let mut ictx = input(&video_path)?;
    let mut decoder = ictx
        .stream(0)
        .ok_or("wrong video file")?
        .codec()
        .decoder()
        .video()?;

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
                if let Some(g) = iter.next() {
                    *g = rgb[j];
                }
            }
        }
    }

    Ok(g2d)
}

/// *read reference temperatures from data acquisition file(.lvm or .xlsx)*
/// ### Argument:
/// temperature record(start line number, total frame number, column numbers that record the temperatures, daq_path)
/// ### Return:
/// 2D matrix of the temperatures from thermocouples
pub fn read_daq<P: AsRef<Path>>(
    temp_record: (usize, usize, &Vec<usize>, P),
) -> Result<Array2<f64>, Box<dyn Error>> {
    use std::io::{Error, ErrorKind::InvalidData};
    match temp_record.3.as_ref().extension() {
        Some(ext) if "lvm".eq(ext) => read_temp_from_lvm(temp_record),
        Some(ext) if "xlsx".eq(ext) => read_temp_from_excel(temp_record),
        _ => Err(Box::new(Error::new(
            InvalidData,
            "only .lvm or .xlsx supported",
        ))),
    }
}

fn read_temp_from_lvm<P: AsRef<Path>>(
    temp_record: (usize, usize, &Vec<usize>, P),
) -> Result<Array2<f64>, Box<dyn Error>> {
    let (start_line, frame_num, columns, temp_path) = temp_record;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(temp_path)?;

    let mut t2d = Array2::zeros((frame_num, columns.len()));
    for (csv_row_result, mut temp_row) in rdr
        .records()
        .skip(start_line)
        .take(frame_num)
        .zip(t2d.axis_iter_mut(Axis(0)))
    {
        let csv_row = csv_row_result?;
        for (&index, t) in columns.iter().zip(temp_row.iter_mut()) {
            *t = csv_row[index].parse::<f64>()?;
        }
    }

    Ok(t2d)
}

fn read_temp_from_excel<P: AsRef<Path>>(
    temp_record: (usize, usize, &Vec<usize>, P),
) -> Result<Array2<f64>, Box<dyn Error>> {
    let (start_line, frame_num, columns, temp_path) = temp_record;
    let mut excel: Xlsx<_> = open_workbook(temp_path)?;
    let sheet = excel.worksheet_range_at(0).ok_or("no sheet exists")??;

    let mut t2d = Array2::zeros((frame_num, columns.len()));
    for (excel_row, mut temp_row) in sheet
        .rows()
        .skip(start_line)
        .take(frame_num)
        .zip(t2d.axis_iter_mut(Axis(0)))
    {
        for (&index, t) in columns.iter().zip(temp_row.iter_mut()) {
            *t = excel_row[index]
                .get_float()
                .ok_or("temperate not in floats")?;
        }
    }

    Ok(t2d)
}

use serde::{Deserialize, Serialize};
use serde_json;

use super::preprocess::{FilterMethod, InterpMethod};
use std::fs::File;
use std::io::BufReader;

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigParas {
    pub video_path: String,
    pub daq_path: String,
    pub start_frame: usize,
    pub start_row: usize,
    pub upper_left_pos: (usize, usize),
    pub region_shape: (usize, usize),
    pub temp_column_num: Vec<usize>,
    pub thermocouple_pos: Vec<(i32, i32)>,
    pub interp_method: InterpMethod,
    pub filter_method: FilterMethod,
    pub peak_temp: f64,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,
    pub h0: f64,
    pub max_iter_num: usize,
}

pub fn read_config<P: AsRef<Path>>(config_path: P) -> Result<ConfigParas, Box<dyn Error>> {
    let file = File::open(config_path)?;
    let reader = BufReader::new(file);
    let cfg = serde_json::from_reader(reader)?;
    Ok(cfg)
}
