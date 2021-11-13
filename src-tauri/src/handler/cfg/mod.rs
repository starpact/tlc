use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncReadExt;

use super::data::{DAQMeta, VideoMeta};

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
    video_shape: Option<(u32, u32)>,
    /// [top_left_y, top_left_x, region_height, region_width]
    region: Option<[u32; 4]>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PhysicalParameter {
    peak_temp: Option<f64>,
    solid_thermal_conductivity: Option<f64>,
    solid_thermal_diffusivity: Option<f64>,
    characteristic_length: Option<f64>,
    air_thermal_conductivity: Option<f64>,
}

#[derive(Debug)]
pub struct G2DParameter {
    pub video_shape: (u32, u32),
    pub region: [u32; 4],
    pub start_frame: usize,
    pub frame_num: usize,
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

    pub fn get_save_info(&self) -> Result<SaveInfo> {
        self.path_manager.get_save_info()
    }

    pub fn get_video_path(&self) -> Option<&Path> {
        Some(self.path_manager.video_path.as_ref()?.as_path())
    }

    pub fn get_daq_path(&self) -> Option<&Path> {
        Some(self.path_manager.daq_path.as_ref()?.as_path())
    }

    pub fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P, video_meta: VideoMeta) {
        self.path_manager.video_path = Some(video_path.as_ref().to_owned());
        self.timing_parameter
            .on_video_change(video_meta.frame_rate, video_meta.total_frames);
        self.geometric_parameter.on_video_change(video_meta.shape);
    }

    pub fn set_daq_path<P: AsRef<Path>>(&mut self, daq_path: P, daq_meta: DAQMeta) {
        self.path_manager.daq_path = Some(daq_path.as_ref().to_owned());
        self.timing_parameter.on_daq_change(daq_meta.total_rows);
    }

    pub fn set_region(&mut self, region: [u32; 4]) -> Result<G2DParameter> {
        let region = self.geometric_parameter.set_region(region)?;

        let start_frame = self
            .timing_parameter
            .start_frame
            .ok_or(anyhow!("start frame unset"))?;
        let frame_num = self
            .timing_parameter
            .frame_num
            .ok_or(anyhow!("frame num unset"))?;
        let video_shape = self
            .geometric_parameter
            .video_shape
            .ok_or(anyhow!("video shape unset"))?;

        Ok(G2DParameter {
            video_shape,
            region,
            start_frame,
            frame_num,
        })
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<G2DParameter> {
        let (start_frame, frame_num) = self.timing_parameter.set_start_frame(start_frame)?;

        let region = self
            .geometric_parameter
            .region
            .ok_or(anyhow!("region unset"))?;
        let video_shape = self
            .geometric_parameter
            .video_shape
            .ok_or(anyhow!("video shape unset"))?;

        Ok(G2DParameter {
            video_shape,
            region,
            start_frame,
            frame_num,
        })
    }
}

impl TimingParameter {
    fn on_video_change(&mut self, frame_rate: usize, total_frames: usize) {
        self.frame_rate = Some(frame_rate);
        self.total_frames = Some(total_frames);

        self.start_frame.take();
        self.frame_num.take();
    }

    fn on_daq_change(&mut self, total_rows: usize) {
        self.total_rows = Some(total_rows);

        self.start_frame.take();
        self.frame_num.take();
    }

    fn set_start_frame(&mut self, start_frame: usize) -> Result<(usize, usize)> {
        if self.start_frame == Some(start_frame) {
            bail!("start frame same as before, no need to rebuild g2d");
        }

        self.start_frame = Some(start_frame);
        let total_frames = self
            .total_frames
            .ok_or(anyhow!("total frames of video unset"))?;
        let total_rows = self.total_rows.ok_or(anyhow!("total rows of daq unset"))?;
        let start_row = self.start_row.ok_or(anyhow!("start row of daq unset"))?;

        let frame_num = (total_frames - start_frame).min(total_rows - start_row);
        self.frame_num = Some(frame_num);

        Ok((start_frame, frame_num))
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
        let case_name = video_path
            .file_stem()
            .ok_or(anyhow!("invalid video path: {:?}", video_path))?;

        Ok(case_name)
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
    fn on_video_change(&mut self, video_shape: (u32, u32)) {
        if self.video_shape == Some(video_shape) {
            return;
        }

        // Most of the time we can make use of the former geometric
        // setting rather than directly invalidate it because within
        // a series of experiments the geometric settings should be similar.
        // We can only get this point when working with a brand new config
        // or different camera parameters were used. Then we just put
        // the select box in the center by default.
        let (h, w) = video_shape;
        self.video_shape = Some((h, w));
        self.region = Some([h / 4, w / 4, h / 2, w / 2]);
    }

    fn set_region(&mut self, region: [u32; 4]) -> Result<[u32; 4]> {
        if self.region == Some(region) {
            bail!("calculation region same as before, no need to rebuild g2d");
        }

        self.region = Some(region);

        Ok(region)
    }
}
