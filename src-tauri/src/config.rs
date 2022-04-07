use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{
    daq::{DaqMetadata, Thermocouple},
    filter::FilterMethod,
    interpolation::InterpMethod,
    solve::{IterationMethod, PhysicalParam},
    video::{Green2Param, VideoMetadata},
};

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

impl TlcConfig {
    const DEFAULT_CONFIG_PATH: &'static str = "./config/default.toml";

    /// Automatically called on startup.
    pub fn from_default_path() -> Result<Self> {
        Self::from_path(Self::DEFAULT_CONFIG_PATH)
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

    pub fn start_frame(&self) -> Option<usize> {
        self.start_frame
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        let nframes = self
            .video_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if start_frame >= nframes {
            bail!("frame_index({}) out of range({})", start_frame, nframes);
        }
        let nrows = self
            .daq_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("daq path unset"))?
            .nrows;
        if let Some(old_start_frame) = self.start_frame.replace(start_frame) {
            if let Some(old_start_row) = self.start_row {
                if old_start_row + start_frame < old_start_frame {
                    bail!("invalid start_frame");
                }
                let start_row = old_start_row + start_frame - old_start_frame;
                if start_row >= nrows {
                    bail!("row_index({}) out of range({})", start_row, nrows);
                }
                self.start_row = Some(start_row);
            }
        }

        Ok(())
    }

    pub fn start_row(&self) -> Option<usize> {
        self.start_row
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        let nrows = self
            .daq_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("daq path unset"))?
            .nrows;
        debug!("{}", nrows);
        if start_row >= nrows {
            bail!("row_index({}) out of range({})", start_row, nrows);
        }
        let nframes = self
            .video_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if let Some(old_start_row) = self.start_row.replace(start_row) {
            if let Some(old_start_frame) = self.start_frame {
                if old_start_frame + start_row < old_start_row {
                    bail!("invalid start_row");
                }
                let start_frame = old_start_frame + start_row - old_start_row;
                if start_frame >= nframes {
                    bail!("frame_index({}) out of range({})", start_frame, nframes);
                }
                self.start_frame = Some(start_frame);
            }
        }

        Ok(())
    }

    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        self.video_metadata.as_ref()
    }

    pub fn set_video_metadata(&mut self, video_metadata: VideoMetadata) {
        match self.video_metadata {
            Some(ref old) if old.shape == video_metadata.shape => {}
            _ => {
                // Most of the time we can make use of the former position
                // setting rather than directly invalidate it because within
                // a series of experiments the position settings should be similar.
                // We can only get this point when working with a brand new config
                // or different camera parameters were used. Then we just put
                // the select box in the center by default.
                let (h, w) = video_metadata.shape;
                self.area = Some((h / 4, w / 4, h / 2, w / 2));
                self.thermocouples.clear();
            }
        }

        self.video_metadata = Some(video_metadata);
        self.start_frame = None;
        self.start_row = None;
    }

    pub fn green2_param(&self) -> Result<Green2Param> {
        let nframes = self
            .video_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        let nrows = self
            .daq_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("daq path unset"))?
            .nrows;
        let start_frame = self
            .start_frame
            .ok_or_else(|| anyhow!("start frame unset"))?;
        let start_row = self.start_row.ok_or_else(|| anyhow!("start row unset"))?;
        let area = self.area.ok_or_else(|| anyhow!("area unset"))?;

        let cal_num = (nframes - start_frame).min(nrows - start_row);

        Ok(Green2Param {
            start_frame,
            cal_num,
            area,
        })
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
