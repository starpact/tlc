mod sqlite;

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
pub use sqlite::SqliteSettingStorage;
use tlc_video::{FilterMethod, VideoMeta};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::{
    daq::{DaqMeta, InterpMethod, Thermocouple},
    solve::{IterationMethod, PhysicalParam},
};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait SettingStorage: Send + 'static {
    fn create_setting(&mut self, request: CreateRequest) -> Result<()>;
    fn switch_setting(&mut self, setting_id: i64) -> Result<()>;
    fn delete_setting(&mut self) -> Result<()>;

    fn name(&self) -> Result<String>;
    fn set_name(&self, name: &str) -> Result<()>;
    fn save_root_dir(&self) -> Result<PathBuf>;
    fn set_save_root_dir(&self, save_root_dir: &Path) -> Result<()>;
    fn video_path(&self) -> Result<PathBuf>;
    fn set_video_path(&self, video_path: &Path) -> Result<()>;
    fn daq_path(&self) -> Result<PathBuf>;
    fn set_daq_path(&self, daq_path: &Path) -> Result<()>;
    fn start_index(&self) -> Result<StartIndex>;
    fn set_start_index(&self, start_frame: usize, start_row: usize) -> Result<()>;
    fn area(&self) -> Result<(u32, u32, u32, u32)>;
    fn set_area(&self, area: (u32, u32, u32, u32)) -> Result<()>;
    fn thermocouples_optional(&self) -> Result<Option<Vec<Thermocouple>>>;
    fn interp_method(&self) -> Result<InterpMethod>;
    fn set_interp_method(&self, interpolation_method: InterpMethod) -> Result<()>;
    fn filter_method(&self) -> Result<FilterMethod>;
    fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()>;
    fn iteration_method(&self) -> Result<IterationMethod>;
    fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()>;
    fn physical_param(&self) -> Result<PhysicalParam>;
    fn set_gmax_temperature(&self, gmax_temperature: f64) -> Result<()>;
    fn set_solid_thermal_conductivity(&self, solid_thermal_conductivity: f64) -> Result<()>;
    fn set_solid_thermal_diffusivity(&self, solid_thermal_diffusivity: f64) -> Result<()>;
    fn set_characteristic_length(&self, characteristic_length: f64) -> Result<()>;
    fn set_air_thermal_conductivity(&self, air_thermal_conductivity: f64) -> Result<()>;

    fn output_file_stem(&self) -> Result<PathBuf> {
        let save_root_dir = self.save_root_dir()?;
        let name = self.name()?;
        Ok(save_root_dir.join(name))
    }

    fn nu_path(&self) -> Result<PathBuf> {
        Ok(self.output_file_stem()?.with_extension("csv"))
    }

    fn plot_path(&self) -> Result<PathBuf> {
        Ok(self.output_file_stem()?.with_extension("png"))
    }

    fn setting_snapshot_path(&self) -> Result<PathBuf> {
        Ok(self.output_file_stem()?.with_extension("toml"))
    }

    fn thermocouples(&self) -> Result<Vec<Thermocouple>> {
        self.thermocouples_optional()?
            .ok_or_else(|| anyhow!("thermocouples not selected yet"))
    }
}

pub struct Setting {
    /// Unique id of this experiment setting, opaque to users.
    pub id: i64,
    /// User defined unique name of this experiment setting.
    pub name: String,

    /// Directory in which you save your data(parameters and results) of this experiment.
    /// * setting_path: {root_dir}/{expertiment_name}/setting.toml
    /// * nu_matrix_path: {root_dir}/{expertiment_name}/nu_matrix.csv
    /// * plot_matrix_path: {root_dir}/{expertiment_name}/nu_plot.png
    pub save_root_dir: String,

    /// Path and some attributes of video.
    pub video_meta: String,

    pub daq_path: Option<PathBuf>,
    /// Path and some attributes of data acquisition file.
    pub daq_meta: String,

    /// Start frame of video involved in the calculation.
    /// Should be updated simultaneously with start_row.
    pub start_frame: Option<usize>,
    /// Start row of DAQ data involved in the calculation.
    /// Should be updated simultaneously with start_frame.
    pub start_row: Option<usize>,

    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    pub area: Option<String>,

    /// Storage info and positions of thermocouples.
    pub thermocouples: Option<String>,

    /// Filter method of green matrix along the time axis.
    pub filter_method: Option<String>,

    /// Interpolation method of thermocouple temperature distribution.
    pub iteration_method: String,

    /// All physical parameters used when solving heat transfer equation.
    pub gmax_temperature: f64,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,

    pub completed_at: bool,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[derive(Debug)]
pub struct CreateRequest {
    pub name: String,
    pub save_root_dir: String,
    pub filter_method: FilterMethod,
    pub iteration_method: IterationMethod,
    pub physical_param: PhysicalParam,
}

/// `SettingSnapshot` will be saved together with the results for later check.
#[derive(Debug, Serialize)]
struct SettingSnapshot {
    save_root_dir: PathBuf,
    video_meta: VideoMeta,
    daq_meta: DaqMeta,
    start_frame: usize,
    start_row: usize,
    area: (usize, usize, usize, usize),
    thermocouples: Vec<Thermocouple>,
    filter_method: FilterMethod,
    interp_method: InterpMethod,
    iteration_method: IterationMethod,
    physical_param: PhysicalParam,
}

impl SettingSnapshot {
    #[instrument(fields(setting_path))]
    pub async fn save<P: AsRef<Path>>(&self, setting_path: P) -> Result<()> {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(setting_path)
            .await?;
        let buf = toml::to_string_pretty(&self)?;
        file.write_all(buf.as_bytes()).await?;

        Ok(())
    }
}

/// StartIndex combines `start_frame` and `start_row` together because
/// they are only meaningful after synchronization and should be updated
/// simultaneously.
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct StartIndex {
    /// Start frame of video involved in the calculation.
    pub start_frame: usize,
    /// Start row of DAQ data involved in the calculation.
    pub start_row: usize,
}
