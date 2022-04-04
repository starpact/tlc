use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    daq::{DaqMetadata, Thermocouple},
    filter::FilterMethod,
    interpolation::InterpMethod,
    solve::{IterationMethod, PhysicalParam},
    video::VideoMetadata,
};

const DEFAULT_CONFIG_PATH: &str = "./config/default.toml";

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TlcConfig {
    /// Directory in which you save your data.
    /// As the `video_path` is unique, we can use its file stem as `case_name`.
    /// * config_path: {root_dir}/config/{case_name}.toml
    /// * nu_matrix_path: {root_dir}/nu_matrix/{case_name}.csv
    /// * plot_matrix_path: {root_dir}/nu_plot/{case_name}.png
    save_root_dir: Option<PathBuf>,
    /// Path and some attributes of video.
    video_metadata: Option<VideoMetadata>,
    /// Path and some attributes of DAQ data.
    daq_metadata: Option<DaqMetadata>,
    /// Start frame of video involved in the calculation.
    start_frame: Option<usize>,
    /// Start row of DAQ data involved in the calculation.
    start_row: Option<usize>,
    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    area: Option<(u32, u32, u32, u32)>,
    /// Storage info and positions of thermocouples.
    #[serde(default)]
    thermocouples: Vec<Thermocouple>,
    /// Filter method of green matrix along the time axis.
    #[serde(default)]
    filter_method: FilterMethod,
    /// Interpolation method of thermocouple temperature distribution.
    interp_method: Option<InterpMethod>,
    /// Iteration method used when solving heat transfer equation.
    #[serde(default)]
    iteration_method: IterationMethod,
    /// All physical parameters used when solving heat transfer equation.
    physical_param: Option<PhysicalParam>,
}

#[derive(Debug, Serialize)]
pub struct Green2Meta {
    start_frame: usize,
    frame_num: usize,
    area: (u32, u32, u32, u32),
}

impl TlcConfig {
    /// Automatically called on startup.
    pub fn from_default_path() -> Self {
        Self::from_path(DEFAULT_CONFIG_PATH).unwrap_or_default()
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let cfg = toml::from_slice(&buf)?;

        Ok(cfg)
    }

    pub fn video_metadata(&self) -> &Option<VideoMetadata> {
        &self.video_metadata
    }

    pub fn set_video_metadata(&mut self, video_metadata: VideoMetadata) {
        self.video_metadata = Some(video_metadata);
        self.start_frame = None;
        self.start_row = None;
        self.thermocouples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_from_file() {
        let cfg = TlcConfig::from_path("config/default.toml").unwrap();
        println!("{cfg:#?}");
    }
}
