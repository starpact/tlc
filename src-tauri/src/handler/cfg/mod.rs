use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::debug;

use super::data::video::VideoInfo;

const DEFAULT_CONFIG_PATH: &'static str = "./config/default.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TLCConfig {
    #[serde(default)]
    path_manager: PathManager,
    #[serde(default)]
    timing_parameter: TimingParameter,
    #[serde(default)]
    geometric_parameter: GeometricParameter,
    #[serde(default)]
    physical_parameter: PhysicalParameter,
}

/// PathManager manages path information that is needed when work with the file system.
/// 1. Where to read data(video + daq)
/// 2. Where to save data(config + nu_matrix + nu_plot)
#[derive(Debug, Default, Serialize, Deserialize)]
struct PathManager {
    /// Path of TLC video file.
    video_path: Option<PathBuf>,
    /// Path of TLC data acquisition file.
    daq_path: Option<PathBuf>,

    /// Directory in which you save your data.
    /// As the `video_path` varies from case to case, we can use file stem of it as `case_name`.
    /// * config_path: {root_dir}/config/{case_name}.toml
    /// * nu_matrix_path: {root_dir}/nu_matrix/{case_name}.csv
    /// * plot_matrix_path: {root_dir}/nu_plot/{case_name}.png
    save_root_dir: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TimingParameter {
    /// Frame rate of video as well as sampling rate of DAQ.
    frame_rate: Option<usize>,
    /// Total frames of video.
    total_frames: Option<usize>,
    /// Total raws of DAQ data.
    total_rows: Option<usize>,
    /// Start frame of video involved in the calculation.
    start_frame: Option<usize>,
    /// Start row of DAQ data involved in the calculation.
    start_row: Option<usize>,
    /// The actual frame number involved in the calculation.
    frame_num: Option<usize>,
}

/// All tuples representing shapes or positions are `(height, width)`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct GeometricParameter {
    video_shape: Option<(usize, usize)>,
    top_left_pos: Option<(usize, usize)>,
    region_shape: Option<(usize, usize)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PhysicalParameter {
    peak_temp: Option<f64>,
    solid_thermal_conductivity: Option<f64>,
    solid_thermal_diffusivity: Option<f64>,
    characteristic_length: Option<f64>,
    air_thermal_conductivity: Option<f64>,
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
        debug!("{:#?}", cfg);

        Ok(cfg)
    }

    pub fn get_video_path(&self) -> Option<&PathBuf> {
        self.path_manager.video_path.as_ref()
    }

    pub fn get_save_info(&self) -> Result<SaveInfo> {
        self.path_manager.get_save_info()
    }

    pub fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P) -> Result<()> {
        let new = video_path.as_ref();
        if let Some(ref old) = self.path_manager.video_path {
            if old == new {
                // If the user re-select the same video, we do not need to do anything.
                bail!("video path same as before");
            }
        }

        self.path_manager.video_path = Some(new.to_owned());

        Ok(())
    }

    pub fn update_with_video_info(&mut self, video_info: VideoInfo) {
        self.timing_parameter.update_with_video_info(video_info);
        self.geometric_parameter.update_with_video_info(video_info);
    }
}

impl TimingParameter {
    fn update_with_video_info(&mut self, video_info: VideoInfo) {
        if self.frame_rate == Some(video_info.frame_rate)
            && self.total_frames == Some(video_info.total_frames)
        {
            return;
        }

        self.frame_rate = Some(video_info.frame_rate);
        self.total_frames = Some(video_info.total_frames);

        self.start_frame.take();
        self.start_row.take();
        self.frame_num.take();
    }
}

#[derive(Serialize)]
pub struct SaveInfo {
    save_root_dir: PathBuf,
    config_path: PathBuf,
    nu_path: PathBuf,
    plot_path: PathBuf,
}

impl PathManager {
    fn get_save_info(&self) -> Result<SaveInfo> {
        match self.save_root_dir {
            Some(ref save_root_dir) => Ok(SaveInfo {
                save_root_dir: save_root_dir.to_owned(),
                config_path: self.get_save_path(SaveCategory::Config)?,
                nu_path: self.get_save_path(SaveCategory::NuMatrix)?,
                plot_path: self.get_save_path(SaveCategory::NuPlot)?,
            }),
            None => bail!("save root dir unset"),
        }
    }

    fn get_case_name(&self) -> Result<&OsStr> {
        let video_path = self
            .video_path
            .as_ref()
            .ok_or(anyhow!("video path unset"))?;

        Ok(video_path
            .file_stem()
            .ok_or(anyhow!("invalid video path: {:?}", video_path))?)
    }

    fn get_save_path(&self, save: SaveCategory) -> Result<PathBuf> {
        let (dir, ext) = match save {
            SaveCategory::Config => ("config", "toml"),
            SaveCategory::NuMatrix => ("nu_matrix", "csv"),
            SaveCategory::NuPlot => ("nu_plot", "png"),
        };

        let save_path = self
            .save_root_dir
            .as_ref()
            .ok_or(anyhow!("save root dir unset"))?
            .join(dir)
            .join(self.get_case_name()?)
            .with_extension(ext);

        Ok(save_path)
    }
}

impl GeometricParameter {
    fn update_with_video_info(&mut self, video_info: VideoInfo) {
        if self.video_shape == Some(video_info.shape) {
            return;
        }

        // Put the select box in the center by default.
        let (h, w) = video_info.shape;
        self.video_shape = Some((h, w));
        self.top_left_pos = Some((h / 4, w / 4));
        self.region_shape = Some((h / 2, w / 2));
    }
}
