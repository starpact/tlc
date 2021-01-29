use std::fs::{create_dir_all, File};
use std::io::BufReader;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{cell::RefCell, io::BufWriter};

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

use crate::err;
use crate::error::TLCResult;
use crate::TLCConfig;

use super::error::TLCError::VideoError;

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

impl TLCConfig {
    pub fn from_path<P: AsRef<Path>>(config_path: P) -> TLCResult<Self> {
        let file = File::open(config_path.as_ref())
            .map_err(|err| err!(ConfigIOError, err, config_path.as_ref()))?;
        let reader = BufReader::new(file);
        let mut cfg: TLCConfig = serde_json::from_reader(reader)?;

        if let Err(err @ VideoError { .. }) = cfg.init_video_metadata() {
            return Err(err);
        }
        let _ = cfg.init_daq_metadata();
        let _ = cfg.init_path();
        cfg.init_frame_num().init_regulator();

        Ok(cfg)
    }

    fn init_video_metadata(&mut self) -> TLCResult<&mut Self> {
        ffmpeg::init().map_err(|err| err!(VideoError, err, "ffmpeg初始化错误，建议重装"))?;

        let input = input(&self.video_path).map_err(|_| err!(VideoIOError, &self.video_path))?;
        let video_stream = input.streams().best(Type::Video).ok_or(err!(
            VideoError,
            "找不到视频流",
            &self.video_path,
        ))?;
        let rational = video_stream.avg_frame_rate();
        self.frame_rate =
            (rational.numerator() as f32 / rational.denominator() as f32).round() as usize;
        self.total_frames = input.duration() as usize * self.frame_rate / 1_000_000;

        Ok(self)
    }

    fn init_daq_metadata(&mut self) -> TLCResult<&mut Self> {
        let daq_path = Path::new(&self.daq_path);
        self.total_rows = match daq_path
            .extension()
            .ok_or(err!(DAQIOError, "路径有误", daq_path))?
            .to_str()
            .ok_or(err!(DAQIOError, "路径有误", daq_path))?
        {
            "lvm" => ReaderBuilder::new()
                .has_headers(false)
                .from_path(daq_path)
                .map_err(|err| err!(DAQIOError, err, daq_path))?
                .records()
                .count(),
            "xlsx" => {
                let mut excel: Xlsx<_> =
                    open_workbook(daq_path).map_err(|err| err!(DAQIOError, err, daq_path))?;
                excel
                    .worksheet_range_at(0)
                    .ok_or(err!(DAQError, "找不到worksheet", daq_path))?
                    .map_err(|err| err!(DAQError, err, daq_path))?
                    .height()
            }
            _ => Err(err!(DAQIOError, "只支持.lvm或.xlsx格式", daq_path))?,
        };

        Ok(self)
    }

    fn init_frame_num(&mut self) -> &mut Self {
        if self.total_frames > 0 && self.total_rows > 0 {
            self.frame_num =
                (self.total_frames - self.start_frame).min(self.total_rows - self.start_row) - 1;
        }

        self
    }

    fn init_path(&mut self) -> TLCResult<&mut Self> {
        self.case_name = Path::new(&self.video_path)
            .file_stem()
            .ok_or(err!(VideoIOError, &self.video_path))?
            .to_str()
            .ok_or(err!(VideoIOError, &self.video_path))?
            .to_owned();
        let save_dir = Path::new(&self.save_dir);
        let config_dir = save_dir.join("config");
        let data_dir = save_dir.join("data");
        let plots_dir = save_dir.join("plots");

        create_dir_all(&config_dir).map_err(|err| err!(CreateDirError, err, config_dir))?;
        create_dir_all(&data_dir).map_err(|err| err!(CreateDirError, err, data_dir))?;
        create_dir_all(&plots_dir).map_err(|err| err!(CreateDirError, err, plots_dir))?;

        let config_path = config_dir.join(&self.case_name).with_extension("json");
        self.config_path = config_path.to_str().ok_or(err!(config_path))?.to_owned();
        let data_path = data_dir.join(&self.case_name).with_extension("csv");
        self.data_path = data_path.to_str().ok_or(err!(data_path))?.to_owned();
        let plots_path = plots_dir.join(&self.case_name).with_extension("png");
        self.plots_path = plots_path.to_str().ok_or(err!(plots_path))?.to_owned();

        Ok(self)
    }

    fn init_regulator(&mut self) {
        if self.regulator.len() == 0 {
            self.regulator = vec![1.; self.temp_column_num.len()];
        }
    }

    pub fn set_save_dir(&mut self, save_dir: String) -> TLCResult<&mut Self> {
        self.save_dir = save_dir;
        self.init_path()?;

        Ok(self)
    }

    pub fn set_video_path(&mut self, video_path: String) -> TLCResult<&mut Self> {
        self.video_path = video_path;
        self.init_video_metadata()?.init_frame_num().init_path()?;

        Ok(self)
    }

    pub fn set_daq_path(&mut self, daq_path: String) -> TLCResult<&mut Self> {
        self.daq_path = daq_path;
        self.init_daq_metadata()?.init_frame_num();

        Ok(self)
    }

    /// 线程池解码视频读取Green值
    pub fn read_video(&self) -> TLCResult<Array2<u8>> {
        // 左上角坐标
        let (tl_y, tl_x) = self.top_left_pos;
        // 区域尺寸
        let (cal_h, cal_w) = self.region_shape;
        // 总像素点数
        let pix_num = cal_h * cal_w;

        ffmpeg::init().map_err(|err| err!(VideoError, err, "ffmpeg初始化错误，建议重装"))?;

        let mut input =
            input(&self.video_path).map_err(|_| err!(VideoIOError, &self.video_path))?;
        let video_stream = input.streams().best(Type::Video).ok_or(err!(
            VideoError,
            "找不到视频流",
            &self.video_path,
        ))?;

        let video_stream_index = video_stream.index();
        let ctx_mutex = &Mutex::new(video_stream.codec());

        let g2d = Array2::zeros((self.frame_num, pix_num));
        let g2d_view = g2d.view();

        let tls = Arc::new(ThreadLocal::new());

        ThreadPoolBuilder::new()
            .build()
            .map_err(|err| err!(err))?
            .install(|| {
                scope(|scp| {
                    for (frame_index, (_, packet)) in input
                        .packets()
                        .filter(|(stream, _)| stream.index() == video_stream_index)
                        .skip(self.start_frame)
                        .take(self.frame_num)
                        .enumerate()
                    {
                        let tls_arc = tls.clone();
                        scp.spawn(move |_| {
                            let tls_paras = if let Ok(tlc_paras) =
                                tls_arc.get_or_try(|| -> Result<_, Box<dyn std::error::Error>> {
                                    let decoder = ctx_mutex.lock()?.clone().decoder().video()?;
                                    let sws_ctx = Context::get(
                                        decoder.format(),
                                        decoder.width(),
                                        decoder.height(),
                                        Pixel::RGB24,
                                        decoder.width(),
                                        decoder.height(),
                                        Flags::FAST_BILINEAR,
                                    )?;
                                    Ok((
                                        RefCell::new(decoder),
                                        RefCell::new(SendableContext(sws_ctx)),
                                        RefCell::new(Video::empty()),
                                        RefCell::new(Video::empty()),
                                    ))
                                }) {
                                tlc_paras
                            } else {
                                return;
                            };

                            let mut decoder = tls_paras.0.borrow_mut();
                            let mut ctx = tls_paras.1.borrow_mut();
                            let mut src_frame = tls_paras.2.borrow_mut();
                            let mut dst_frame = tls_paras.3.borrow_mut();

                            if decoder.send_packet(&packet).is_err()
                                || decoder.receive_frame(&mut src_frame).is_err()
                                || ctx.run(&src_frame, &mut dst_frame).is_err()
                            {
                                return;
                            }

                            // the data of each frame store in one u8 array:
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

    /// 读取参考温度(.lvm or .xlsx)
    pub fn read_daq(&self) -> TLCResult<Array2<f32>> {
        let daq_path = Path::new(&self.daq_path);
        let raw_t2d = match daq_path
            .extension()
            .ok_or(err!(DAQIOError, "路径有误", daq_path))?
            .to_str()
            .ok_or(err!(DAQIOError, "路径有误", daq_path))?
        {
            "lvm" => self.read_temp_from_lvm(),
            "xlsx" => self.read_temp_from_excel(),
            _ => Err(err!(DAQIOError, "只支持.lvm或.xlsx格式", daq_path))?,
        }?;

        let regulator = Array::from_shape_vec((self.regulator.len(), 1), self.regulator.clone())
            .map_err(|err| err!(err))?;

        Ok(raw_t2d * regulator)
    }

    fn read_temp_from_lvm(&self) -> TLCResult<Array2<f32>> {
        let daq_path = Path::new(&self.daq_path);
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b'\t')
            .from_path(daq_path)
            .map_err(|err| err!(DAQIOError, err, daq_path))?;

        let mut t2d = Array2::zeros((self.temp_column_num.len(), self.frame_num));
        for (csv_row_result, mut temp_col) in rdr
            .records()
            .skip(self.start_row)
            .take(self.frame_num)
            .zip(t2d.axis_iter_mut(Axis(1)))
        {
            let csv_row = csv_row_result.map_err(|err| err!(DAQIOError, err, daq_path))?;
            for (&index, t) in self.temp_column_num.iter().zip(temp_col.iter_mut()) {
                *t = csv_row[index].parse::<f32>().map_err(|err| {
                    err!(
                        DAQError,
                        format!("数据采集文件中不应当有数字以外的格式{}", err),
                        daq_path,
                    )
                })?;
            }
        }

        Ok(t2d)
    }

    fn read_temp_from_excel(&self) -> TLCResult<Array2<f32>> {
        let daq_path = Path::new(&self.daq_path);
        let mut excel: Xlsx<_> =
            open_workbook(daq_path).map_err(|err| err!(DAQIOError, err, daq_path))?;
        let sheet = excel
            .worksheet_range_at(0)
            .ok_or(err!(DAQError, "找不到worksheet", daq_path))?
            .map_err(|err| err!(DAQIOError, err, daq_path))?;

        let mut t2d = Array2::zeros((self.temp_column_num.len(), self.frame_num));
        for (excel_row, mut temp_col) in sheet
            .rows()
            .skip(self.start_row)
            .take(self.frame_num)
            .zip(t2d.axis_iter_mut(Axis(1)))
        {
            for (&index, t) in self.temp_column_num.iter().zip(temp_col.iter_mut()) {
                *t = excel_row[index].get_float().ok_or(err!(
                    DAQError,
                    "数据采集文件中不应当有数字以外的格式",
                    daq_path,
                ))? as f32;
            }
        }

        Ok(t2d)
    }

    /// 保存配置
    pub fn save(&self) -> TLCResult<()> {
        let file = File::create(&self.config_path)
            .map_err(|err| err!(ConfigIOError, err, self.config_path))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;

        Ok(())
    }
}

pub fn save_data<P: AsRef<Path>>(data: ArrayView2<f32>, data_path: P) -> TLCResult<()> {
    let mut wtr = WriterBuilder::new()
        .has_headers(false)
        .from_path(data_path.as_ref())
        .map_err(|err| err!(DataSaveError, err, data_path.as_ref()))?;

    for row in data.axis_iter(Axis(0)) {
        let v: Vec<_> = row.iter().map(|x| x.to_string()).collect();
        wtr.write_record(&StringRecord::from(v))
            .map_err(|err| err!(DataSaveError, err, data_path.as_ref()))?;
    }

    Ok(())
}

pub fn read_data<P: AsRef<Path>>(data_path: P) -> TLCResult<Array2<f32>> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(data_path.as_ref())
        .map_err(|err| err!(DataReadError, err, data_path.as_ref()))?;
    let width = rdr
        .records()
        .next()
        .ok_or(err!(DataReadError, "矩阵为空", data_path.as_ref()))?
        .map_err(|err| err!(DataReadError, err, data_path.as_ref()))?
        .len();
    let height = rdr.records().count() + 1;

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(data_path.as_ref())
        .map_err(|err| err!(DataReadError, err, data_path.as_ref()))?;

    let mut data = Array2::zeros((height, width));

    for (csv_row_result, mut nu_row) in rdr.records().zip(data.axis_iter_mut(Axis(0))) {
        let csv_row = csv_row_result.map_err(|err| err!(DataReadError, err, data_path.as_ref()))?;

        for (csv_val, nu) in csv_row.iter().zip(nu_row.iter_mut()) {
            *nu = csv_val
                .parse::<f32>()
                .map_err(|err| err!(DataReadError, err, data_path.as_ref()))?;
        }
    }

    Ok(data)
}
