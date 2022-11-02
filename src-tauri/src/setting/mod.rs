mod sqlite;

use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
pub use sqlite::Setting;
use tlc_video::{FilterMethod, VideoMeta};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::{
    daq::{DaqMeta, InterpMethod, Thermocouple},
    solve::{IterationMethod, PhysicalParam},
};

pub fn new_db<P: AsRef<Path>>(path: P) -> Connection {
    let db =
        Connection::open(path).unwrap_or_else(|e| panic!("Failed to create/open metadata db: {e}"));
    init_db(&db);
    db
}

#[cfg(test)]
pub fn new_db_in_memory() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    init_db(&db);
    db
}

fn init_db(db: &Connection) {
    db.execute(include_str!("../../db/schema.sql"), ())
        .expect("Failed to create db");
}

#[derive(Debug)]
pub struct CreateRequest {
    pub name: String,
    pub save_root_dir: PathBuf,
    pub video_path: Option<PathBuf>,
    pub daq_path: Option<PathBuf>,
    pub start_frame: Option<usize>,
    pub start_row: Option<usize>,
    pub area: Option<(u32, u32, u32, u32)>,
    pub thermocouples: Option<Vec<Thermocouple>>,
    pub interp_method: Option<InterpMethod>,
    pub filter_method: FilterMethod,
    pub iteration_method: IterationMethod,
    pub physical_param: PhysicalParam,
}

/// `SettingSnapshot` will be saved together with the results for later check.
#[derive(Debug, Serialize)]
pub struct SettingSnapshot {
    /// User defined unique name of this experiment setting.
    pub name: String,
    /// Directory in which you save your data(parameters and results) of this experiment.
    /// * setting_path: {root_dir}/{expertiment_name}/setting.toml
    /// * nu_matrix_path: {root_dir}/{expertiment_name}/nu_matrix.csv
    /// * plot_matrix_path: {root_dir}/{expertiment_name}/nu_plot.png
    pub save_root_dir: PathBuf,
    pub video_path: PathBuf,
    pub video_meta: VideoMeta,
    pub daq_path: PathBuf,
    pub daq_meta: DaqMeta,
    /// Start frame of video involved in the calculation.
    /// Updated simultaneously with start_row.
    pub start_frame: usize,
    /// Start row of DAQ data involved in the calculation.
    /// Updated simultaneously with start_frame.
    pub start_row: usize,
    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    pub area: (u32, u32, u32, u32),
    /// Storage info and positions of thermocouples.
    pub thermocouples: Vec<Thermocouple>,
    /// Filter method of green matrix along the time axis.
    pub filter_method: FilterMethod,
    /// Interpolation method of thermocouple temperature distribution.
    pub interp_method: InterpMethod,
    pub iteration_method: IterationMethod,
    /// All physical parameters used when solving heat transfer equation.
    pub physical_param: PhysicalParam,
    /// Timestamp in milliseconds.
    pub completed_at: u64,
}

impl SettingSnapshot {
    #[instrument(fields(setting_path), err)]
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
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct StartIndex {
    /// Start frame of video involved in the calculation.
    pub start_frame: usize,
    /// Start row of DAQ data involved in the calculation.
    pub start_row: usize,
}
