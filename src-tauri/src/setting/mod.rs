mod sqlite;

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::{
    daq::{DaqMetadata, InterpolationMethod, Thermocouple},
    solve::{IterationMethod, PhysicalParam},
    video::{FilterMetadata, FilterMethod, Green2Metadata, VideoMetadata},
};
pub use sqlite::SqliteSettingStorage;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait SettingStorage: Send + 'static {
    fn create_setting(&mut self, request: CreateRequest) -> Result<()>;
    fn switch_setting(&mut self, setting_id: i64) -> Result<()>;
    fn delete_setting(&mut self) -> Result<()>;

    fn save_root_dir(&self) -> Result<String>;
    fn set_save_root_dir(&self, save_root_dir: &Path) -> Result<()>;
    fn video_metadata(&self) -> Result<VideoMetadata>;
    fn set_video_metadata(&self, video_metadata: &VideoMetadata) -> Result<()>;
    fn daq_metadata(&self) -> Result<DaqMetadata>;
    fn set_daq_metadata(&self, daq_metadata: &DaqMetadata) -> Result<()>;
    fn start_index(&self) -> Result<StartIndex>;
    fn synchronize_video_and_daq(&self, start_frame: usize, start_row: usize) -> Result<()>;
    fn set_start_frame(&self, start_frame: usize) -> Result<()>;
    fn set_start_row(&self, start_row: usize) -> Result<()>;
    fn area(&self) -> Result<(usize, usize, usize, usize)>;
    fn set_area(&self, area: (usize, usize, usize, usize)) -> Result<()>;
    fn green2_metadata(&self) -> Result<Green2Metadata>;
    fn thermocouples(&self) -> Result<Vec<Thermocouple>>;
    fn interpolation_method(&self) -> Result<InterpolationMethod>;
    fn set_interpolation_method(&self, interpolation_method: InterpolationMethod) -> Result<()>;
    fn filter_metadata(&self) -> Result<FilterMetadata>;
    fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()>;
    fn iteration_method(&self) -> Result<IterationMethod>;
    fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()>;
    fn physical_param(&self) -> Result<PhysicalParam>;
}

struct Setting {
    /// Unique id of this experiment setting, opaque to users.
    id: i64,
    /// User defined unique name of this experiment setting.
    name: String,

    /// Directory in which you save your data(parameters and results) of this experiment.
    /// * setting_path: {root_dir}/{expertiment_name}/setting.toml
    /// * nu_matrix_path: {root_dir}/{expertiment_name}/nu_matrix.csv
    /// * plot_matrix_path: {root_dir}/{expertiment_name}/nu_plot.png
    save_root_dir: String,

    /// Path and some attributes of video.
    video_metadata: String,

    /// Path and some attributes of data acquisition file.
    daq_metadata: String,

    /// Start frame of video involved in the calculation.
    /// Should be updated simultaneously with start_row.
    start_frame: Option<usize>,
    /// Start row of DAQ data involved in the calculation.
    /// Should be updated simultaneously with start_frame.
    start_row: Option<usize>,

    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    area: Option<String>,

    /// Storage info and positions of thermocouples.
    thermocouples: Option<String>,

    /// Filter method of green matrix along the time axis.
    filter_method: Option<String>,

    /// Interpolation method of thermocouple temperature distribution.
    iteration_method: String,

    /// All physical parameters used when solving heat transfer equation.
    gmax_temperature: f64,
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,

    /// If process of current experiment has been completed.
    completed: bool,

    created_at: Instant,
    updated_at: Instant,
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
    video_metadata: VideoMetadata,
    daq_metadata: DaqMetadata,
    start_frame: usize,
    start_row: usize,
    area: (usize, usize, usize, usize),
    thermocouples: Vec<Thermocouple>,
    filter_method: FilterMethod,
    interpolation_method: InterpolationMethod,
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
