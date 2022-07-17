use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tauri::async_runtime;
use tokio::io::AsyncReadExt;

use crate::{
    daq::{DaqMetadata, Thermocouple},
    interpolation::InterpMethod,
    solve::{IterationMethod, PhysicalParam},
    video::{FilterMethod, Green2Param, VideoMetadata},
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
    area: Option<(usize, usize, usize, usize)>,

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

/// StartIndex combines `start_frame` and `start_row` together because
/// they are only meaningful after synchronization and should be updated
/// simultaneously.
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
        async_runtime::block_on(async { Self::from_path(Self::DEFAULT_CONFIG_PATH).await })
    }

    pub async fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = tokio::fs::OpenOptions::new()
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

    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        self.video_metadata.as_ref()
    }

    pub fn set_video_metadata(&mut self, video_metadata: Option<VideoMetadata>) {
        let video_metadata = match video_metadata {
            Some(video_metadata) => video_metadata,
            None => {
                // Clear current video related config, probably because some error has
                // occurred and the config should be invalid.
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

    fn nframes(&self) -> Option<usize> {
        Some(self.video_metadata.as_ref()?.nframes)
    }

    fn nrows(&self) -> Option<usize> {
        Some(self.daq_metadata.as_ref()?.nrows)
    }

    pub fn start_frame(&self) -> Option<usize> {
        Some(self.start_index?.start_frame)
    }

    pub fn start_row(&self) -> Option<usize> {
        Some(self.start_index?.start_row)
    }

    pub fn synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
    ) -> Result<()> {
        let nframes = self.nframes().ok_or_else(|| anyhow!("video path unset"))?;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self.nrows().ok_or_else(|| anyhow!("daq path unset"))?;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });

        Ok(())
    }

    pub fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        let nframes = self.nframes().ok_or_else(|| anyhow!("video path unset"))?;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
        let nrows = self.nrows().ok_or_else(|| anyhow!("daq path unset"))?;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }
        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });

        Ok(())
    }

    pub fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        let nrows = self.nrows().ok_or_else(|| anyhow!("daq path unset"))?;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
        let nframes = self.nframes().ok_or_else(|| anyhow!("video path unset"))?;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_row");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({start_frame}) out of range({nframes})");
        }

        self.start_index = Some(StartIndex {
            start_frame,
            start_row,
        });

        Ok(())
    }

    pub fn green2_param(&self) -> Result<Green2Param> {
        let video_metadata = self
            .video_metadata
            .as_ref()
            .ok_or_else(|| anyhow!("video path unset"))?;
        let nrows = self.nrows().ok_or_else(|| anyhow!("daq path unset"))?;
        let StartIndex {
            start_frame,
            start_row,
        } = self
            .start_index
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
        let area = self.area.ok_or_else(|| anyhow!("area unset"))?;

        let path = video_metadata.path.clone();
        let nframes = video_metadata.nframes;
        let cal_num = (nframes - start_frame).min(nrows - start_row);

        Ok(Green2Param {
            path,
            start_frame,
            cal_num,
            area,
        })
    }

    pub fn filter_method(&self) -> FilterMethod {
        self.filter_method
    }

    pub fn set_filter_method(&mut self, filter_method: FilterMethod) {
        self.filter_method = filter_method;
    }

    pub fn iteration_method(&self) -> IterationMethod {
        self.iteration_method
    }

    pub fn physical_param(&self) -> Option<PhysicalParam> {
        self.physical_param
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_from_file() {
        let cfg = Config::from_default_path().unwrap();
        println!("{cfg:#?}");
    }
}
