use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::{
    daq::{DaqMetadata, Thermocouple},
    filter::FilterMethod,
    interpolation::InterpMethod,
    solve::{IterationMethod, PhysicalParam},
    video::{Green2Param, VideoMetadata},
};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
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

    /// Start index of video and DAQ after synchronization.
    start_index: Option<StartIndex>,

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

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
struct StartIndex {
    /// Start frame of video involved in the calculation.
    start_frame: usize,

    /// Start row of DAQ data involved in the calculation.
    start_row: usize,
}

impl Config {
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

    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        self.video_metadata.as_ref()
    }

    pub fn set_video_metadata(&mut self, video_metadata: Option<VideoMetadata>) {
        let video_metadata = match video_metadata {
            Some(video_metadata) => video_metadata,
            None => {
                self.video_metadata = None;
                self.start_index = None;
                return;
            }
        };

        if !matches!(&self.video_metadata, Some(old) if old.path == video_metadata.path) {
            self.start_index = None;
        }

        if !matches!(&self.video_metadata, Some(old) if old.shape == video_metadata.shape) {
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

        self.video_metadata = Some(video_metadata);
    }

    pub fn daq_metadata(&self) -> Option<&DaqMetadata> {
        self.daq_metadata.as_ref()
    }

    pub fn set_daq_metadata(&mut self, daq_metadata: Option<DaqMetadata>) {
        let daq_metadata = match daq_metadata {
            Some(daq_metadata) => daq_metadata,
            None => {
                self.daq_metadata = None;
                self.start_index = None;
                return;
            }
        };

        self.daq_metadata = Some(daq_metadata);
    }

    fn nframes(&self) -> Result<usize> {
        Ok(self
            .video_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes)
    }

    fn nrows(&self) -> Result<usize> {
        Ok(self
            .daq_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("daq path unset"))?
            .nrows)
    }

    pub fn start_frame(&self) -> Result<usize> {
        Ok(self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?
            .start_frame)
    }

    pub fn start_row(&self) -> Result<usize> {
        Ok(self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?
            .start_row)
    }

    pub fn synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
    ) -> Result<()> {
        let nframes = self.nframes()?;
        if start_frame >= nframes {
            bail!("frame_index({}) out of range({})", start_frame, nframes);
        }
        let nrows = self.nrows()?;
        if start_row >= nrows {
            bail!("row_index({}) out of range({})", start_row, nrows);
        }

        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });

        Ok(())
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        let nframes = self.nframes()?;
        if start_frame >= nframes {
            bail!("frame_index({}) out of range({})", start_frame, nframes);
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
        let nrows = self.nrows()?;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }
        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({}) out of range({})", start_row, nrows);
        }

        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });

        Ok(())
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        let nrows = self.nrows()?;
        if start_row >= nrows {
            bail!("row_index({}) out of range({})", start_row, nrows);
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
        let nframes = self.nframes()?;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_row");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({}) out of range({})", start_frame, nframes);
        }

        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });

        Ok(())
    }

    pub fn green2_param(&self) -> Result<Green2Param> {
        let nframes = self.nframes()?;
        let nrows = self.nrows()?;
        let StartIndex {
            start_frame,
            start_row,
        } = self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
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
        let cfg = Config::from_path("config/default.toml").unwrap();
        println!("{cfg:#?}");
    }
}