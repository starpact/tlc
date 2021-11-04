use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::debug;

const DEFAULT_CONFIG_PATH: &'static str = "./config/default.toml";

#[derive(Debug, Serialize, Deserialize)]
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
    /// frame rate of video as well as sampling rate of DAQ
    frame_rate: Option<usize>,
    /// total frames of video
    total_frames: Option<usize>,
    /// total raws of DAQ data
    total_rows: Option<usize>,
    /// start frame of video involved in the calculation
    start_frame: Option<usize>,
    /// start row of DAQ data involved in the calculation
    start_row: Option<usize>,
    /// the actual frame number involved in the calculation
    frame_num: Option<usize>,
}

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
    pub async fn from_default_path() -> Result<Self> {
        Self::from_path(DEFAULT_CONFIG_PATH).await
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
        let cfg = toml::from_slice::<TLCConfig>(&buf)?;
        debug!("{:#?}", cfg);

        Ok(cfg)
    }

    pub fn get_save_info(&self) -> Result<SaveInfo> {
        self.path_manager.get_save_info()
    }

    pub fn get_video_path(&self) -> Option<PathBuf> {
        self.path_manager.video_path.clone()
    }

    pub fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P) -> Result<()> {
        self.path_manager.video_path = Some(video_path.as_ref().to_owned());

        // Invalidate all timing parameters.
        self.timing_parameter = TimingParameter::default();
        // Here we choose not to invalidate the geometric parameter because this
        // varies little from case to case, we can make use of former settings.

        Ok(())
    }

    async fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        debug!("{:?}", path.as_ref());

        Ok(())
    }

    async fn save(&self) -> Result<()> {
        self.save_to_path(self.path_manager.get_save_path(SaveCategory::Config)?)
            .await
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

    /// case_name is always extracted from the current video_path so we do not need
    /// to take care of invalidation.
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

        Ok(self
            .save_root_dir
            .as_ref()
            .ok_or(anyhow!("save root dir unset"))?
            .join(dir)
            .join(self.get_case_name()?)
            .with_extension(ext))
    }
}
