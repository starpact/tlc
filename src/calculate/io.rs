use std::cell::RefCell;
use std::fs::{DirBuilder, File};
use std::io::BufReader;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json;

use ndarray::prelude::*;

use rayon::{scope, ThreadPoolBuilder};

use thread_local::ThreadLocal;

use ffmpeg_next as ffmpeg;

use ffmpeg::format::{input, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{flag::Flags, Context};
use ffmpeg::util::frame::video::Video;

use calamine::{open_workbook, Reader, Xlsx};

use csv::{ReaderBuilder, StringRecord, WriterBuilder};

use super::error::{TLCError, TLCResult};
use super::preprocess::{FilterMethod, InterpMethod};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigParas {
    pub video_path: String,
    pub daq_path: String,
    pub save_dir: String,
    pub start_frame: usize,
    pub start_row: usize,
    pub top_left_pos: (usize, usize),
    pub region_shape: (usize, usize),
    pub temp_column_num: Vec<usize>,
    pub thermocouple_pos: Vec<(i32, i32)>,
    pub interp_method: InterpMethod,
    pub filter_method: FilterMethod,
    pub peak_temp: f32,
    pub solid_thermal_conductivity: f32,
    pub solid_thermal_diffusivity: f32,
    pub characteristic_length: f32,
    pub air_thermal_conductivity: f32,
    pub h0: f32,
    pub max_iter_num: usize,
}

pub fn read_config<P: AsRef<Path>>(config_path: P) -> TLCResult<ConfigParas> {
    let file = File::open(config_path.as_ref())
        .map_err(|err| TLCError::config_io_error(err, config_path.as_ref()))?;
    let reader = BufReader::new(file);
    let cfg = serde_json::from_reader(reader)?;

    Ok(cfg)
}

pub fn get_metadata<P: AsRef<Path>>(
    video_path: P,
    daq_path: P,
    start_frame: usize,
    start_row: usize,
) -> TLCResult<(usize, usize, usize, usize)> {
    let (total_frames, frame_rate) = get_frames_of_video(video_path)?;
    let total_rows = get_rows_of_daq(daq_path)?;
    let frame_num = (total_frames - start_frame).min(total_rows - start_row) - 1;

    Ok((frame_num, frame_rate, total_frames, total_rows))
}

fn get_frames_of_video<P: AsRef<Path>>(video_path: P) -> TLCResult<(usize, usize)> {
    ffmpeg::init().map_err(|err| TLCError::video_error(err, "ffmpeg初始化错误，建议重装"))?;

    let input =
        input(&video_path).map_err(|err| TLCError::video_error(err, video_path.as_ref()))?;
    let video_stream = input
        .streams()
        .best(Type::Video)
        .ok_or(TLCError::video_error("找不到视频流", video_path.as_ref()))?;
    let rational = video_stream.avg_frame_rate();
    let frame_rate = (rational.numerator() as f32 / rational.denominator() as f32).round() as usize;
    let total_frame = input.duration() as usize * frame_rate / 1_000_000;

    Ok((total_frame, frame_rate))
}

fn get_rows_of_daq<P: AsRef<Path>>(daq_path: P) -> TLCResult<usize> {
    match daq_path
        .as_ref()
        .extension()
        .ok_or(TLCError::daq_io_error("路径有误", daq_path.as_ref()))?
        .to_str()
        .ok_or(TLCError::daq_io_error("路径有误", daq_path.as_ref()))?
    {
        "lvm" => Ok(ReaderBuilder::new()
            .has_headers(false)
            .from_path(daq_path.as_ref())
            .map_err(|err| TLCError::daq_io_error(err, daq_path.as_ref()))?
            .records()
            .count()),
        "xlsx" => {
            let mut excel: Xlsx<_> = open_workbook(daq_path.as_ref())
                .map_err(|err| TLCError::daq_io_error(err, daq_path.as_ref()))?;
            let sheet = excel
                .worksheet_range_at(0)
                .ok_or(TLCError::daq_error("找不到worksheet", daq_path.as_ref()))?
                .map_err(|err| TLCError::daq_error(err, daq_path.as_ref()))?;
            Ok(sheet.height())
        }
        _ => Err(TLCError::daq_io_error(
            "只支持.lvm或.xlsx格式",
            daq_path.as_ref(),
        ))?,
    }
}

/// wrap `Context` to pass between threads(because of raw pointer)
struct SendableContext(Context);

unsafe impl Send for SendableContext {}

impl Deref for SendableContext {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendableContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// read the video and collect all green values spatially and temporally
///
/// use thread pool
pub fn read_video<P: AsRef<Path>>(
    video_record: (usize, usize, P),
    region_record: ((usize, usize), (usize, usize)),
) -> TLCResult<Array2<u8>> {
    let (start_frame, frame_num, video_path) = video_record;

    // top left coordinate
    let (tl_y, tl_x) = region_record.0;
    // height and width of calculation region
    let (cal_h, cal_w) = region_record.1;
    // total number of pixels in the calculation region
    let pix_num = cal_h * cal_w;

    ffmpeg::init().map_err(|err| TLCError::video_error(err, "ffmpeg初始化错误，建议重装"))?;

    let mut input =
        input(&video_path).map_err(|err| TLCError::video_error(err, video_path.as_ref()))?;
    let video_stream = input
        .streams()
        .best(Type::Video)
        .ok_or(TLCError::video_error("找不到视频流", video_path.as_ref()))?;

    let video_stream_index = video_stream.index();
    let ctx_mutex = &Mutex::new(video_stream.codec());

    let g2d = Array2::zeros((frame_num, pix_num));
    let g2d_view = g2d.view();

    let tls = Arc::new(ThreadLocal::new());

    ThreadPoolBuilder::new()
        .build()
        .map_err(|err| TLCError::UnKnown(err.to_string()))?
        .install(|| {
            scope(|scp| {
                for (frame_index, (_, packet)) in input
                    .packets()
                    .filter(|(stream, _)| stream.index() == video_stream_index)
                    .skip(start_frame)
                    .take(frame_num)
                    .enumerate()
                {
                    let tls_arc = tls.clone();
                    scp.spawn(move |_| {
                        let tls_paras = tls_arc.get_or(|| {
                            let decoder =
                                ctx_mutex.lock().unwrap().clone().decoder().video().unwrap();
                            let sws_ctx = Context::get(
                                decoder.format(),
                                decoder.width(),
                                decoder.height(),
                                Pixel::RGB24,
                                decoder.width(),
                                decoder.height(),
                                Flags::FAST_BILINEAR,
                            )
                            .unwrap();
                            (
                                RefCell::new(decoder),
                                RefCell::new(SendableContext(sws_ctx)),
                                RefCell::new(Video::empty()),
                                RefCell::new(Video::empty()),
                            )
                        });

                        let mut decoder = tls_paras.0.borrow_mut();
                        let mut ctx = tls_paras.1.borrow_mut();
                        let mut src_frame = tls_paras.2.borrow_mut();
                        let mut dst_frame = tls_paras.3.borrow_mut();

                        decoder.send_packet(&packet).unwrap(); // most time-consuming function
                        decoder.receive_frame(&mut src_frame).unwrap();
                        ctx.run(&src_frame, &mut dst_frame).unwrap();

                        // the data of each frame stores in one u8 array:
                        // ||r g b r g b...r g b|......|r g b r g b...r g b||
                        // ||.......row_0.......|......|.......row_n.......||
                        let rgb = dst_frame.data(0);
                        let real_w = (decoder.width() * 3) as usize;

                        let ptr = g2d_view.view().as_ptr() as *mut u8;
                        let mut index = (pix_num * frame_index) as isize;
                        for i in (0..).step_by(real_w).skip(tl_y).take(cal_h) {
                            for j in (i..).skip(1).step_by(3).skip(tl_x).take(cal_w) {
                                unsafe { *ptr.offset(index) = *rgb.get_unchecked(j) };
                                index += 1;
                            }
                        }
                    });
                }
            })
        });

    Ok(g2d)
}

/// read reference temperatures from data acquisition file(.lvm or .xlsx)
pub fn read_daq<P: AsRef<Path>>(
    temp_record: (usize, usize, &Vec<usize>, P),
) -> TLCResult<Array2<f32>> {
    let daq_path = &temp_record.3;
    match daq_path
        .as_ref()
        .extension()
        .ok_or(TLCError::daq_io_error("路径有误", daq_path.as_ref()))?
        .to_str()
        .ok_or(TLCError::daq_io_error("路径有误", daq_path.as_ref()))?
    {
        "lvm" => read_temp_from_lvm(temp_record),
        "xlsx" => read_temp_from_excel(temp_record),
        _ => Err(TLCError::daq_io_error(
            "只支持.lvm或.xlsx格式",
            daq_path.as_ref(),
        ))?,
    }
}

fn read_temp_from_lvm<P: AsRef<Path>>(
    temp_record: (usize, usize, &Vec<usize>, P),
) -> TLCResult<Array2<f32>> {
    let (start_line, frame_num, columns, daq_path) = temp_record;
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(daq_path.as_ref())
        .map_err(|err| TLCError::daq_io_error(err, daq_path.as_ref()))?;

    let mut t2d = Array2::zeros((frame_num, columns.len()));
    for (csv_row_result, mut temp_row) in rdr
        .records()
        .skip(start_line)
        .take(frame_num)
        .zip(t2d.axis_iter_mut(Axis(0)))
    {
        let csv_row =
            csv_row_result.map_err(|err| TLCError::daq_io_error(err, daq_path.as_ref()))?;
        for (&index, t) in columns.iter().zip(temp_row.iter_mut()) {
            *t = csv_row[index].parse::<f32>().map_err(|_| {
                TLCError::daq_error("数据采集文件中不应当有数字以外的格式", daq_path.as_ref())
            })?;
        }
    }

    Ok(t2d)
}

fn read_temp_from_excel<P: AsRef<Path>>(
    temp_record: (usize, usize, &Vec<usize>, P),
) -> TLCResult<Array2<f32>> {
    let (start_line, frame_num, columns, daq_path) = temp_record;
    let mut excel: Xlsx<_> = open_workbook(daq_path.as_ref())
        .map_err(|err| TLCError::daq_io_error(err, daq_path.as_ref()))?;
    let sheet = excel
        .worksheet_range_at(0)
        .ok_or(TLCError::daq_error("找不到worksheet", daq_path.as_ref()))?
        .map_err(|err| TLCError::daq_io_error(err, daq_path.as_ref()))?;

    let mut t2d = Array2::zeros((frame_num, columns.len()));
    for (excel_row, mut temp_row) in sheet
        .rows()
        .skip(start_line)
        .take(frame_num)
        .zip(t2d.axis_iter_mut(Axis(0)))
    {
        for (&index, t) in columns.iter().zip(temp_row.iter_mut()) {
            *t = excel_row[index].get_float().ok_or(TLCError::daq_error(
                "数据采集文件中不应当有数字以外的格式",
                daq_path.as_ref(),
            ))? as f32;
        }
    }

    Ok(t2d)
}

pub fn get_save_path<P: AsRef<Path>>(video_path: P, save_dir: P) -> TLCResult<(PathBuf, PathBuf)> {
    let nu_dir = save_dir.as_ref().join("Nu");
    let plot_dir = save_dir.as_ref().join("plots");
    DirBuilder::new()
        .recursive(true)
        .create(&nu_dir)
        .map_err(|err| {
            TLCError::create_dir_error(format!("创建/Nu失败。{}", err), save_dir.as_ref())
        })?;
    DirBuilder::new()
        .recursive(true)
        .create(&plot_dir)
        .map_err(|err| {
            TLCError::create_dir_error(format!("创建/plots失败。{}", err), save_dir.as_ref())
        })?;
    let file_name = video_path
        .as_ref()
        .file_stem()
        .ok_or(TLCError::video_io_error(video_path.as_ref()))?;
    let nu_path = nu_dir.join(file_name).with_extension("csv");
    let plot_path = plot_dir.join(file_name).with_extension("png");

    Ok((nu_path, plot_path))
}

pub fn save_nu<P: AsRef<Path>>(nu2d: ArrayView2<f32>, nu_path: P) -> TLCResult<()> {
    let mut wtr = WriterBuilder::new()
        .has_headers(false)
        .from_path(nu_path.as_ref())
        .map_err(|err| TLCError::nu_save_error(err, nu_path.as_ref()))?;

    for row in nu2d.axis_iter(Axis(0)) {
        let v: Vec<_> = row.iter().map(|x| x.to_string()).collect();
        wtr.write_record(&StringRecord::from(v))
            .map_err(|err| TLCError::nu_save_error(err, nu_path.as_ref()))?;
    }

    Ok(())
}

pub fn read_nu<P: AsRef<Path>>(nu_path: P) -> TLCResult<Array2<f32>> {
    // avoid adding the shape into arguments, though ugly
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(nu_path.as_ref())
        .map_err(|err| TLCError::nu_read_error(err, nu_path.as_ref()))?;
    let width = rdr
        .records()
        .next()
        .ok_or(TLCError::nu_read_error("Nu矩阵为空", nu_path.as_ref()))?
        .map_err(|err| TLCError::nu_read_error(err, nu_path.as_ref()))?
        .len();
    let height = rdr.records().count() + 1;

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(nu_path.as_ref())
        .map_err(|err| TLCError::nu_read_error(err, nu_path.as_ref()))?;
    let mut nu2d = Array2::zeros((height, width));

    for (csv_row_result, mut nu_row) in rdr.records().zip(nu2d.axis_iter_mut(Axis(0))) {
        let csv_row =
            csv_row_result.map_err(|err| TLCError::nu_read_error(err, nu_path.as_ref()))?;

        for (csv_val, nu) in csv_row.iter().zip(nu_row.iter_mut()) {
            *nu = csv_val
                .parse::<f32>()
                .map_err(|err| TLCError::nu_read_error(err, nu_path.as_ref()))?;
        }
    }

    Ok(nu2d)
}
