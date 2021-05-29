mod colormap;
pub mod error;
pub mod io;
pub mod postprocess;
pub mod preprocess;
pub mod solve;

use std::{path::Path, sync::{Arc, Mutex}};

use ffmpeg_next::Packet;
use serde::{Deserialize, Serialize};
use ndarray::prelude::*;

use preprocess::{FilterMethod, Interp, InterpMethod};
use solve::IterationMethod;
use io::{Decoder, VideoCtx};
use error::TLCResult;
use crate::awsl;

/// 默认配置文件路径
const DEFAULT_CONFIG_PATH: &'static str = "./config/default_config.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Thermocouple {
    /// 热电偶在数采文件中的列数
    pub column_num: usize,
    /// 热电偶的位置(y, x)
    pub pos: (i32, i32),
}

/// 所有配置信息，与case一一对应
#[derive(Debug, Serialize, Deserialize)]
pub struct TLCConfig {
    /// 实验组名称（与视频文件名一致）
    #[serde(default = "default_case_name")]
    case_name: String,
    /// 保存配置信息和所有结果的根目录
    #[serde(default)]
    save_dir: String,
    /// 视频文件路径
    #[serde(default = "default_video_path")]
    video_path: String,
    /// 数采文件路径
    #[serde(default = "default_daq_path")]
    daq_path: String,
    /// 配置文件保存路径（仅运行时使用）
    #[serde(skip)]
    config_path: String,
    /// 图片保存路径（仅运行时使用）
    #[serde(skip)]
    plots_path: String,
    /// 数据保存路径（仅运行时使用）
    #[serde(skip)]
    data_path: String,
    /// 视频起始帧数
    #[serde(default)]
    start_frame: usize,
    /// 视频总帧数
    #[serde(default)]
    total_frames: usize,
    /// 视频帧率
    #[serde(default)]
    frame_rate: usize,
    /// 数采文件起始行数
    #[serde(default)]
    start_row: usize,
    /// 数采文件总行数
    #[serde(default)]
    total_rows: usize,
    /// 实际处理总帧数
    #[serde(default)]
    frame_num: usize,
    /// 视频尺寸（高，宽）
    #[serde(default)]
    video_shape: (usize, usize),
    /// 计算区域左上角坐标(y, x)
    #[serde(default)]
    top_left_pos: (usize, usize),
    /// 计算区域尺寸（高，宽）
    #[serde(default = "default_region_shape")]
    region_shape: (usize, usize),
    /// 各热电偶
    #[serde(default)]
    thermocouples: Vec<Thermocouple>,
    /// 插值方法
    #[serde(default)]
    interp_method: InterpMethod,
    /// 滤波方法
    #[serde(default)]
    filter_method: FilterMethod,
    /// 导热方程迭代求解方法（初值，最大迭代步数）
    #[serde(default)]
    iteration_method: IterationMethod,
    /// 峰值温度
    #[serde(default = "default_peak_temp")]
    peak_temp: f32,
    /// 固体导热系数
    #[serde(default = "default_solid_thermal_conductivity")]
    solid_thermal_conductivity: f32,
    /// 固体热扩散系数
    #[serde(default = "default_solid_thermal_diffusivity")]
    solid_thermal_diffusivity: f32,
    /// 特征长度
    #[serde(default = "default_characteristic_length")]
    characteristic_length: f32,
    /// 空气导热系数
    #[serde(default = "default_air_thermal_conductivity")]
    air_thermal_conductivity: f32,
    /// emmmmmm
    #[serde(default)]
    regulator: Vec<f32>,
}

fn default_case_name() -> String {
    "case_name".to_owned()
}

fn default_video_path() -> String {
    "video_path".to_owned()
}

fn default_daq_path() -> String {
    "daq_path".to_owned()
}

fn default_region_shape() -> (usize, usize) {
    (500, 500)
}

fn default_peak_temp() -> f32 {
    35.48
}

fn default_solid_thermal_conductivity() -> f32 { 
    0.19 
}

fn default_solid_thermal_diffusivity() -> f32 {
    1.091e-7
}

fn default_characteristic_length() -> f32 {
    0.015
}

fn default_air_thermal_conductivity() -> f32 {
    0.0276
}

/// 配置信息 + 运行时数据
///
/// 运行时产生的数据会在所依赖配置变化时析构
pub struct TLCData {
    /// 配置信息
    config: TLCConfig,
    /// 每个视频一份
    video_ctx: Option<VideoCtx>,
    /// 每个线程一份
    decoder_tool: Option<Decoder>,
    /// 已加载的视频数据包
    packets: Arc<Mutex<Vec<Packet>>>,
    /// 未滤波的Green值二维矩阵，排列方式如下：
    ///
    /// 第一帧: | X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ... |
    ///
    /// 第二帧: | X1Y1 X2Y1 ... XnY1 X1Y2 X2Y2 ... XnY2 ... |
    ///
    /// ......
    raw_g2d: Option<Array2<u8>>,
    /// 滤波后的Green值二维矩阵
    filtered_g2d: Option<Array2<u8>>,
    /// 所有点峰值对应帧数
    peak_frames: Option<Vec<usize>>,
    /// 数采文件数据
    /// 数据排列方式与.lvm/.xlsx一致
    daq: Option<Array2<f32>>,
    /// 热电偶温度二维矩阵
    /// 从daq中选出各热电偶对应的列然后转置（保证内存连续，插值时能使用SIMD）
    /// 排列方式如下：
    ///
    /// 1号热电偶：| 第一帧 第二帧 ... |
    ///
    /// 2号热电偶：| 第一帧 第二帧 ... |
    ///
    /// ......
    t2d: Option<Array2<f32>>,
    /// 插值所得温度场
    interp: Option<Interp>,
    /// 努塞尔数二维矩阵
    nu2d: Option<Array2<f32>>,
    /// 努赛尔数平均值
    nu_nan_mean: Option<f32>,
}

/// 当某项数据所依赖的配置信息发生变化时，清空数据
macro_rules! delete {
    ($v:ident @ $($member:tt),* $(,)*) => {
        $($v.$member.take();)*
    };
}

impl TLCData {
    pub fn new() -> TLCResult<Self> {
        Self::from_path(DEFAULT_CONFIG_PATH)
    }

    pub fn from_path<P: AsRef<Path>>(config_path: P) -> TLCResult<Self> {
        Ok(Self {
            config: TLCConfig::from_path(config_path)?,
            video_ctx: None,
            decoder_tool: None,
            packets: Arc::new(Mutex::new(Vec::new())),
            raw_g2d: None,
            filtered_g2d: None,
            peak_frames: None,
            daq: None,
            t2d: None,
            interp: None,
            nu2d: None,
            nu_nan_mean: None,
        })
    }

    pub fn get_config(&self) -> &TLCConfig {
        &self.config
    }

    pub fn get_video_ctx(&self) -> TLCResult<&VideoCtx> {
        self.video_ctx.as_ref().ok_or(awsl!())
    }

    pub fn get_decoder(&self) -> TLCResult<&Decoder> {
        self.decoder_tool.as_ref().ok_or(awsl!())
    }

    pub fn get_raw_g2d(&self) -> TLCResult<ArrayView2<u8>> {
        self.raw_g2d.as_ref().map(|v| v.view()).ok_or(awsl!())
    }

    pub fn get_filtered_g2d(&self) -> TLCResult<ArrayView2<u8>> {
        self.filtered_g2d.as_ref().map(|v| v.view()).ok_or(awsl!())
    }

    pub fn get_peak_frames(&self) -> TLCResult<&Vec<usize>> {
        self.peak_frames.as_ref().ok_or(awsl!())
    }

    pub fn get_daq(&self) -> TLCResult<ArrayView2<f32>> {
        self.daq.as_ref().map(|v| v.view()).ok_or(awsl!())
    }

    pub fn get_t2d(&self) -> TLCResult<ArrayView2<f32>> {
        self.t2d.as_ref().map(|v| v.view()).ok_or(awsl!())
    }

    pub fn get_interp(&self) -> TLCResult<&Interp> {
        self.interp.as_ref().ok_or(awsl!())
    }

    pub fn get_nu2d(&self) -> TLCResult<ArrayView2<f32>> {
        self.nu2d.as_ref().map(|v| v.view())
            .ok_or(awsl!(HandleError, "求解设置发生变化，需要重新求解"))
    }

    pub fn get_nu_nan_mean(&self) -> TLCResult<f32> {
        self.nu_nan_mean.ok_or(awsl!())
    }

    pub fn set_save_dir(&mut self, save_dir: String) -> TLCResult<&mut Self> {
        self.config.set_save_dir(save_dir)?;

        Ok(self)
    }

    pub fn set_video_path(&mut self, video_path: String) -> TLCResult<&mut Self> {
        self.config.set_video_path(video_path)?;
        delete!(self @ video_ctx, decoder_tool, raw_g2d, filtered_g2d, 
            peak_frames, t2d, interp, nu2d, nu_nan_mean);

        Ok(self)
    }

    pub fn set_daq_path(&mut self, daq_path: String) -> TLCResult<&mut Self> {
        self.config.set_daq_path(daq_path)?;
        delete!(self @ raw_g2d, filtered_g2d, peak_frames, daq, t2d, interp, nu2d, nu_nan_mean);

        Ok(self)
    }

    pub fn set_filter_method(&mut self, filter_method: FilterMethod) -> &mut Self {
        self.config.filter_method = filter_method;
        delete!(self @ filtered_g2d, peak_frames, nu2d, nu_nan_mean);

        self
    }

    pub fn set_interp_method(&mut self, interp_method: InterpMethod) -> TLCResult<&mut Self> {
        use InterpMethod::*;
        let tcs = &mut self.config.thermocouples;
        match interp_method {
            Horizontal | HorizontalExtra => tcs.sort_unstable_by_key(|tc| tc.pos.1),
            Vertical | VerticalExtra => tcs.sort_unstable_by_key(|tc| tc.pos.0),
            Bilinear((h, w)) | BilinearExtra((h, w)) => {
                if h * w != tcs.len() {
                    return Err(awsl!(HandleError,format!("热电偶行数({})列数({})之积不等于热电偶数量", h, w)));
                }
                // 先按y排序
                tcs.sort_unstable_by_key(|tc| tc.pos.0);
                // 行内按x排序
                for i in (0..).step_by(w).take(h) {
                    tcs[i..i + w].sort_unstable_by_key(|tc| tc.pos.1);
                }
            }
        }
        self.config.interp_method = interp_method;
        delete!(self @ interp, nu2d, nu_nan_mean);

        Ok(self)
    }

    pub fn set_iteration_method(&mut self, iteration_method: IterationMethod) -> &mut Self {
        self.config.iteration_method = iteration_method;
        delete!(self @ nu2d, nu_nan_mean);

        self
    }

    pub fn set_region(
        &mut self,
        top_left_pos: (usize, usize),
        region_shape: (usize, usize),
    ) -> &mut Self {
        if top_left_pos != self.config.top_left_pos || region_shape != self.config.region_shape {
            self.config.top_left_pos = top_left_pos;
            self.config.region_shape = region_shape;
            delete!(self @ raw_g2d, filtered_g2d, peak_frames, interp, nu2d, nu_nan_mean);
        }

        self
    }

    pub fn set_regulator(&mut self, regulator: Vec<f32>) -> &mut Self {
        self.config.regulator = regulator;
        delete!(self @ t2d, interp, nu2d, nu_nan_mean);

        self
    }

    pub fn set_peak_temp(&mut self, peak_temp: f32) -> &mut Self {
        self.config.peak_temp = peak_temp;
        delete!(self @ nu2d, nu_nan_mean);

        self
    }

    pub fn set_solid_thermal_conductivity(&mut self, solid_thermal_conductivity: f32) -> &mut Self {
        self.config.solid_thermal_conductivity = solid_thermal_conductivity;
        delete!(self @ nu2d, nu_nan_mean);

        self
    }

    pub fn set_solid_thermal_diffusivity(&mut self, solid_thermal_diffusivity: f32) -> &mut Self {
        self.config.solid_thermal_diffusivity = solid_thermal_diffusivity;
        delete!(self @ nu2d, nu_nan_mean);

        self
    }

    pub fn set_air_thermal_conductivity(&mut self, air_thermal_conductivity: f32) -> &mut Self {
        self.config.air_thermal_conductivity = air_thermal_conductivity;
        delete!(self @ nu2d, nu_nan_mean);

        self
    }

    pub fn set_characteristic_length(&mut self, characteristic_length: f32) -> &mut Self {
        self.config.characteristic_length = characteristic_length;
        delete!(self @ nu2d, nu_nan_mean);

        self
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> TLCResult<&mut Self> {
        self.config.set_start_frame(start_frame)?;
        delete!(self @ raw_g2d, filtered_g2d, peak_frames, t2d, interp, nu2d, nu_nan_mean);

        Ok(self)
    }

    pub fn set_start_row(&mut self, start_row: usize) -> TLCResult<&mut Self> {
        self.config.set_start_row(start_row)?;
        delete!(self @ raw_g2d, filtered_g2d, peak_frames, t2d, interp, nu2d, nu_nan_mean);

        Ok(self)
    }

    pub fn synchronize(&mut self, frame_index: usize, row_index: usize) -> &mut Self {
        self.config.synchronize(frame_index, row_index);

        self
    }

    pub fn set_thermocouples(&mut self, thermocouples: Vec<Thermocouple>) -> &mut Self {
        self.config.set_thermocouples(thermocouples);
        delete!(self @ t2d, interp, nu2d, nu_nan_mean);

        self
    }

    pub fn save_config(&self) -> TLCResult<&Self> {
        self.config.save()?;

        Ok(self)
    }

    pub fn save_nu(&mut self) -> TLCResult<&mut Self> {
        io::save_data(self.get_nu2d()?, &self.config.data_path)?;

        Ok(self)
    }
}
