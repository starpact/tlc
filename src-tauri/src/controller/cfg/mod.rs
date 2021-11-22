use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncReadExt;

use super::data::{FilterMethod, InterpMethod, IterationMethod};

const DEFAULT_CONFIG_PATH: &str = "./config/default.toml";

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TLCConfig {
    /// Directory in which you save your data.
    /// As the `video_path` varies from case to case, we can use file stem of it as `case_name`.
    /// * config_path: {root_dir}/config/{case_name}.toml
    /// * nu_matrix_path: {root_dir}/nu_matrix/{case_name}.csv
    /// * plot_matrix_path: {root_dir}/nu_plot/{case_name}.png
    pub save_root_dir: Option<PathBuf>,

    /// Video metadata: attributes of the video. Once video path is determined, so are
    /// other attributes. So these can be regarded as a cache.
    video_meta: Option<VideoMeta>,
    ///
    daq_meta: Option<DAQMeta>,

    /// Start frame of video involved in the calculation.
    start_frame: Option<usize>,
    /// Start row of DAQ data involved in the calculation.
    start_row: Option<usize>,

    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    area: Option<(u32, u32, u32, u32)>,
    /// Storage and positions of thermocouples.
    thermocouples: Option<Vec<Thermocouple>>,

    /// Filter method of green matrix along the time axis.
    pub filter_method: FilterMethod,
    /// Interpolation method of thermocouple temperature distribution.
    interp_method: InterpMethod,
    /// Iteration method used when solving heat transfer equation.
    iteration_method: IterationMethod,

    physical_param: PhysicalParam,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PhysicalParam {
    peak_temperature: Option<f64>,
    solid_thermal_conductivity: Option<f64>,
    solid_thermal_diffusivity: Option<f64>,
    characteristic_length: Option<f64>,
    air_thermal_conductivity: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VideoMeta {
    /// Path of TLC video file.
    pub path: PathBuf,
    /// Frame rate of video.
    #[serde(default)]
    pub frame_rate: usize,
    /// Total frames of video.
    #[serde(default)]
    pub total_frames: usize,
    /// (video_height, video_width)
    #[serde(default)]
    pub shape: (u32, u32),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DAQMeta {
    /// Path of TLC data acquisition file.
    pub path: PathBuf,
    /// Total raws of DAQ data.
    #[serde(default)]
    pub total_rows: usize,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Thermocouple {
    /// Column numbers of this thermocouple in the DAQ file.
    pub column_num: usize,
    /// Position of this thermocouple(y, x). Thermocouples
    /// may not be in the video area, so coordinate can be negative.
    pub pos: (i32, i32),
}

#[derive(Debug)]
pub struct G2Param {
    pub start_frame: usize,
    pub frame_num: usize,
    pub area: (u32, u32, u32, u32),
}

enum SaveCategory {
    Config,
    NuMatrix,
    NuPlot,
}

impl TLCConfig {
    pub async fn from_default_path() -> Self {
        Self::from_path(DEFAULT_CONFIG_PATH)
            .await
            .unwrap_or_default()
    }

    pub async fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .await?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await?;
        let cfg = toml::from_slice(&buf)?;

        Ok(cfg)
    }

    pub fn get_video_path(&self) -> Option<&Path> {
        Some(self.video_meta.as_ref()?.path.as_path())
    }

    pub fn get_daq_path(&self) -> Option<&Path> {
        Some(self.daq_meta.as_ref()?.path.as_path())
    }

    pub fn get_save_root_dir(&self) -> Result<&Path> {
        Ok(self
            .save_root_dir
            .as_ref()
            .ok_or_else(|| anyhow!("save root dir unset"))?
            .as_path())
    }

    #[allow(dead_code)]
    pub fn get_config_save_path(&self) -> Result<PathBuf> {
        self.get_save_path(SaveCategory::Config)
    }

    #[allow(dead_code)]
    pub fn get_nu_matrix_save_path(&self) -> Result<PathBuf> {
        self.get_save_path(SaveCategory::NuMatrix)
    }

    #[allow(dead_code)]
    pub fn get_nu_plot_save_path(&self) -> Result<PathBuf> {
        self.get_save_path(SaveCategory::NuPlot)
    }

    pub fn on_video_load(&mut self, video_meta: VideoMeta) -> Result<()> {
        let new_path = video_meta.path.clone();
        let new_shape = video_meta.shape;
        let old_video_meta = self.video_meta.replace(video_meta);

        if let Some(ref old_video_meta) = old_video_meta {
            if old_video_meta.path == new_path {
                bail!("video path same as before")
            }
            if old_video_meta.shape != new_shape {
                // Most of the time we can make use of the former position
                // setting rather than directly invalidate it because within
                // a series of experiments the position settings should be similar.
                // We can only get this point when working with a brand new config
                // or different camera parameters were used. Then we just put
                // the select box in the center by default.
                let (h, w) = new_shape;
                self.area = Some((h / 4, w / 4, h / 2, w / 2));
            }
        }

        self.start_frame.take();

        Ok(())
    }

    pub fn on_daq_load(&mut self, daq_meta: DAQMeta) -> Result<()> {
        let new_path = daq_meta.path.clone();
        let old_daq_meta = self.daq_meta.replace(daq_meta);

        if let Some(old_daq_meta) = old_daq_meta {
            if old_daq_meta.path == new_path {
                bail!("daq path same as before")
            }
        }

        self.start_row.take();

        Ok(())
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<G2Param> {
        if self.start_frame == Some(start_frame) {
            bail!("start frame same as before");
        }

        self.start_frame = Some(start_frame);
        let cal_num = self.get_cal_num()?;
        let area = self.area.ok_or_else(|| anyhow!("calculation area unset"))?;

        Ok(G2Param {
            start_frame,
            frame_num: cal_num,
            area,
        })
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<G2Param> {
        if self.start_row == Some(start_row) {
            bail!("start row same as before");
        }

        self.start_row = Some(start_row);
        let cal_num = self.get_cal_num()?;
        let start_frame = self
            .start_frame
            .ok_or_else(|| anyhow!("start frame unset"))?;
        let area = self.area.ok_or_else(|| anyhow!("calculation area unset"))?;

        Ok(G2Param {
            start_frame,
            frame_num: cal_num,
            area,
        })
    }

    pub fn set_area(&mut self, area: (u32, u32, u32, u32)) -> Result<G2Param> {
        if self.area == Some(area) {
            bail!("calculation area same as before, no need to rebuild g2");
        }

        self.area = Some(area);
        let start_frame = self
            .start_frame
            .ok_or_else(|| anyhow!("start frame unset"))?;
        let cal_num = self.get_cal_num()?;

        Ok(G2Param {
            start_frame,
            frame_num: cal_num,
            area,
        })
    }
}

impl TLCConfig {
    fn get_save_path(&self, save: SaveCategory) -> Result<PathBuf> {
        let (dir, ext) = match save {
            SaveCategory::Config => ("config", "toml"),
            SaveCategory::NuMatrix => ("nu_matrix", "csv"),
            SaveCategory::NuPlot => ("nu_plot", "png"),
        };

        let save_path = self
            .save_root_dir
            .as_ref()
            .ok_or_else(|| anyhow!("save root dir unset"))?
            .join(dir)
            .join(self.get_case_name()?)
            .with_extension(ext);

        Ok(save_path)
    }

    fn get_case_name(&self) -> Result<&OsStr> {
        let video_path = &self
            .video_meta
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?
            .path;
        let case_name = video_path
            .file_stem()
            .ok_or_else(|| anyhow!("invalid video path: {:?}", video_path))?;

        Ok(case_name)
    }

    fn get_cal_num(&mut self) -> Result<usize> {
        let total_frames = self
            .video_meta
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?
            .total_frames;
        let total_rows = self
            .daq_meta
            .as_ref()
            .ok_or_else(|| anyhow!("daq path unset"))?
            .total_rows;
        let start_frame = self
            .start_frame
            .ok_or_else(|| anyhow!("start frame unset"))?;
        let start_row = self.start_row.ok_or_else(|| anyhow!("start row unset"))?;

        Ok((total_frames - start_frame).min(total_rows - start_row))
    }
}
