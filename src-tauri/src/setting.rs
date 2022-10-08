use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, Error::QueryReturnedNoRows};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::instrument;

use crate::{
    daq::{DaqMetadata, InterpolationMethod, Thermocouple},
    error::Error,
    solve::{IterationMethod, PhysicalParam},
    util,
    video::{FilterMethod, Green2Param, VideoMetadata},
};

#[derive(Debug)]
pub struct SettingStorage {
    conn: Connection,
    /// Setting id of the experiment which is currently being processed.
    /// `setting_id` should be manually updated by the user and will be
    /// used for all single row operations automatically.
    setting_id: Option<i64>,
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
    peak_temperature: f64,
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    characteristic_length: f64,
    air_thermal_conductivity: f64,

    /// If process of current experiment has been completed.
    completed: bool,

    created_at: Instant,
    updated_at: Instant,
}

#[derive(Debug, Deserialize)]
pub struct CreateSettingRequest {
    pub name: String,
    pub save_root_dir: String,
    pub video_metadata: VideoMetadata,
    pub filter_method: FilterMethod,
    pub iteration_method: IterationMethod,
    pub daq_metadata: DaqMetadata,
    pub physical_param: PhysicalParam,
}

#[derive(Debug, Deserialize)]
pub enum SwitchSettingOption {
    Create(Box<CreateSettingRequest>),
    Update(i64),
}

impl SettingStorage {
    pub fn new() -> Self {
        const DB_FILEPATH: &str = "./var/db.sqlite3";
        let conn = Connection::open(DB_FILEPATH)
            .unwrap_or_else(|e| panic!("Failed to create/open metadata db at {DB_FILEPATH}: {e}"));
        conn.execute(include_str!("../db/schema.sql"), ())
            .expect("Failed to create db");

        Self {
            conn,
            setting_id: None,
        }
    }

    fn setting_id(&self) -> Result<i64> {
        self.setting_id
            .ok_or_else(|| anyhow!("no experiment setting is selected"))
    }

    fn switch_setting(&mut self, switch_setting_option: SwitchSettingOption) -> Result<()> {
        match switch_setting_option {
            SwitchSettingOption::Create(create_setting_request) => {
                let CreateSettingRequest {
                    name,
                    save_root_dir,
                    video_metadata,
                    filter_method,
                    iteration_method,
                    daq_metadata,
                    physical_param:
                        PhysicalParam {
                            peak_temperature,
                            solid_thermal_conductivity,
                            solid_thermal_diffusivity,
                            characteristic_length,
                            air_thermal_conductivity,
                        },
                } = *create_setting_request;
                let video_metadata_str = serde_json::to_string(&video_metadata)?;
                let daq_metadata_str = serde_json::to_string(&daq_metadata)?;
                let filter_method_str = serde_json::to_string(&filter_method)?;
                let iteration_method_str = serde_json::to_string(&iteration_method)?;
                let created_at = util::time::now_as_secs();
                let id = self
                    .conn
                    .prepare(
                        "
                        INSERT INTO settings (
                            name,
                            save_root_dir,
                            video_metadata,
                            daq_metadata,
                            filter_method,
                            iteration_method,
                            peak_temperature,
                            solid_thermal_conductivity,
                            solid_thermal_diffusivity,
                            characteristic_length,
                            air_thermal_conductivity,
                            completed,
                            created_at,
                            updated_at
                        )
                        VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
                        )
                        ",
                    )?
                    .insert(params![
                        name,
                        save_root_dir,
                        video_metadata_str,
                        daq_metadata_str,
                        filter_method_str,
                        iteration_method_str,
                        peak_temperature,
                        solid_thermal_conductivity,
                        solid_thermal_diffusivity,
                        characteristic_length,
                        air_thermal_conductivity,
                        false,
                        created_at,
                        created_at,
                    ])?;
                self.setting_id = Some(id);
                Ok(())
            }
            SwitchSettingOption::Update(id) => {
                if Some(id) == self.setting_id {
                    // The caller will reload everything even if the setting id has not changed.
                    return Ok(());
                }
                let id = self
                    .conn
                    .query_row("SELECT * FROM settings WHERE id = ?1", [id], |row| {
                        row.get(0)
                    })
                    .map_err::<anyhow::Error, _>(|e| match e {
                        QueryReturnedNoRows => Error::SettingIdNotFound(id).into(),
                        _ => e.into(),
                    })?;
                self.setting_id = Some(id);
                Ok(())
            }
        }
    }

    pub fn get_save_root_dir(&self) -> Result<String> {
        let id = self.setting_id()?;
        let save_root_dir = self.conn.query_row(
            "SELECT save_root_dir FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(save_root_dir)
    }

    pub fn set_save_root_dir(&self, save_root_dir: String) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET save_root_dir = ?1, updated_at = ?2 WHERE id = ?3",
            params![save_root_dir, updated_at, id],
        )?;

        Ok(())
    }

    pub fn get_video_metadata(&self) -> Result<Option<VideoMetadata>> {
        let id = self.setting_id()?;
        let ret: Option<String> = self.conn.query_row(
            "SELECT video_metadata FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_video_metadata(&self, video_metadata: VideoMetadata) -> Result<()> {
        let id = self.setting_id()?;
        match self.get_video_metadata()? {
            Some(old_video_metadata)
                if old_video_metadata.fingerprint == video_metadata.fingerprint =>
            {
                // When two videos have the same finderprint, we can reuse other part of
                // the current setting. The only possible change is the path of video.
                if old_video_metadata.path != video_metadata.path {
                    let video_metadata_str = serde_json::to_string(&video_metadata)?;
                    let updated_at = util::time::now_as_secs();
                    self.conn.execute(
                        "UPDATE settings SET video_metadata = ?1, updated_at = ?2  WHERE id = ?3",
                        params![video_metadata_str, updated_at, id],
                    )?;
                }
                Ok(())
            }
            Some(old_video_metadata) if old_video_metadata.shape == video_metadata.shape => {
                // Most of the time we can make use of the previous position
                // setting rather than directly invalidate it because within
                // a series of experiments the position settings should be similar.
                let video_metadata_str = serde_json::to_string(&video_metadata)?;
                let updated_at = util::time::now_as_secs();
                // Reset start_frame and start_row but reuse area and thermocouples.
                self.conn.execute(
                    "
                    UPDATE settings SET
                        video_metadata = ?1,
                        start_frame = NULL,
                        start_row = NULL,
                        updated_at = ?2
                    WHERE
                        id = ?3
                    ",
                    params![video_metadata_str, updated_at, id],
                )?;
                Ok(())
            }
            _ => {
                // We will only get this point when working with a brand new setting
                // or different camera parameters. Then we just put the select box in
                // the center by default.
                let video_metadata_str = serde_json::to_string(&video_metadata)?;
                let (h, w) = video_metadata.shape;
                let area = (h / 4, w / 4, h / 2, w / 2);
                let area_str = serde_json::to_string(&area)?;
                let updated_at = util::time::now_as_secs();
                // Reset start_frame, start_row, area and thermocouples.
                self.conn.execute(
                    "
                    UPDATE settings SET
                        video_metadata = ?1,
                        start_frame = NULL,
                        start_row = NULL,
                        area = ?2,
                        thermocouples = NULL,
                        updated_at = ?3
                    WHERE
                        id = ?4
                    ",
                    params![video_metadata_str, area_str, updated_at, id],
                )?;
                Ok(())
            }
        }
    }

    pub fn get_daq_metadata(&self) -> Result<Option<DaqMetadata>> {
        let id = self.setting_id()?;
        let ret: Option<String> = self.conn.query_row(
            "SELECT daq_metadata FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_daq_metadata(&self, daq_metadata: DaqMetadata) -> Result<()> {
        let id = self.setting_id()?;
        match self.get_daq_metadata()? {
            Some(old_daq_metadata) if old_daq_metadata.fingerprint == daq_metadata.fingerprint => {
                if old_daq_metadata.path != daq_metadata.path {
                    let daq_metadata_str = serde_json::to_string(&daq_metadata)?;
                    let updated_at = util::time::now_as_secs();
                    self.conn.execute(
                        "UPDATE settings SET daq_metadata = ?1, updated_at = ?2  WHERE id = ?3",
                        params![daq_metadata_str, updated_at, id],
                    )?;
                }
                Ok(())
            }
            _ => {
                let thermocouples = self.get_thermocouples()?;
                let daq_metadata_str = serde_json::to_string(&daq_metadata)?;
                let updated_at = util::time::now_as_secs();

                if let Some(thermocouples) = thermocouples {
                    if thermocouples
                        .iter()
                        .any(|t| t.column_index >= daq_metadata.ncols)
                    {
                        self.conn.execute(
                            "
                    UPDATE settings SET
                        daq_metadata = ?1,
                        start_frame = NULL,
                        start_row = NULL,
                        thermocouples = NULL,
                        updated_at = ?2
                    WHERE
                        id = ?3
                    ",
                            params![daq_metadata_str, updated_at, id],
                        )?;
                    }
                    return Ok(());
                }

                self.conn.execute(
                    "
                    UPDATE settings SET
                        daq_metadata = ?1,
                        start_frame = NULL,
                        start_row = NULL,
                        updated_at = ?2
                    WHERE
                        id = ?3
                    ",
                    params![daq_metadata_str, updated_at, id],
                )?;
                Ok(())
            }
        }
    }

    pub fn get_start_index(&self) -> Result<Option<StartIndex>> {
        let id = self.setting_id()?;
        let ret: (Option<usize>, Option<usize>) = self.conn.query_row(
            "SELECT (start_frame, start_row) FROM settings WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        match ret {
            (None, None) => Ok(None),
            (Some(start_frame), Some(start_row)) => Ok(Some(StartIndex {
                start_frame,
                start_row,
            })),
            _ => unreachable!("start_frame and start_row are not consistent"),
        }
    }

    pub fn synchronize_video_and_daq(&self, start_frame: usize, start_row: usize) -> Result<()> {
        let nframes = self
            .get_video_metadata()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self
            .get_daq_metadata()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.set_start_index(start_frame, start_row)
    }

    pub fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        let nframes = self
            .get_video_metadata()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self
            .get_start_index()?
            .ok_or_else(|| anyhow!("not synchronized yet"))?;

        let nrows = self
            .get_daq_metadata()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }

        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.set_start_index(start_frame, start_row)
    }

    pub fn set_start_row(&self, start_row: usize) -> Result<()> {
        let nrows = self
            .get_daq_metadata()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self
            .get_start_index()?
            .ok_or_else(|| anyhow!("not synchronized yet"))?;

        let nframes = self
            .get_video_metadata()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_row");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({start_frame}) out of range({nframes})");
        }

        self.set_start_index(start_frame, start_row)
    }

    pub fn get_area(&self) -> Result<Option<(usize, usize, usize, usize)>> {
        let id = self.setting_id()?;
        let ret: Option<String> =
            self.conn
                .query_row("SELECT area FROM settings WHERE id = ?1", [id], |row| {
                    row.get(0)
                })?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_area(&self, area: (usize, usize, usize, usize)) -> Result<()> {
        let id = self.setting_id()?;
        let (h, w) = self
            .get_video_metadata()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .shape;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_y + cal_h > h {
            bail!("area Y out of range: top_left_y({tl_y})+area_height({cal_h})>video_height({h})");
        }
        if tl_x + cal_w > w {
            bail!("area X out of range: top_left_x({tl_x})+area_width({cal_w})>video_width({w})");
        }
        let area_str = serde_json::to_string(&area)?;

        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET area = ?1, updated_at = ?2 WHERE id = ?3",
            params![area_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn get_green2_param(&self) -> Result<Green2Param> {
        let video_metadata = self
            .get_video_metadata()?
            .ok_or_else(|| anyhow!("video path unset"))?;
        let nrows = self
            .get_daq_metadata()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        let StartIndex {
            start_frame,
            start_row,
        } = self
            .get_start_index()?
            .ok_or_else(|| anyhow!("not synchronized yet"))?;
        let area = self.get_area()?.ok_or_else(|| anyhow!("area unset"))?;

        let nframes = video_metadata.nframes;
        let cal_num = (nframes - start_frame).min(nrows - start_row);

        Ok(Green2Param {
            path: video_metadata.path,
            start_frame,
            cal_num,
            area,
        })
    }

    pub fn get_thermocouples(&self) -> Result<Option<Vec<Thermocouple>>> {
        let id = self.setting_id()?;
        let ret: Option<String> = self.conn.query_row(
            "SELECT thermocouples FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn get_filter_method(&self) -> Result<FilterMethod> {
        let id = self.setting_id()?;
        let s: String = self.conn.query_row(
            "SELECT filter_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(serde_json::from_str(&s)?)
    }

    pub fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let id = self.setting_id()?;
        let filter_method_str = serde_json::to_string(&filter_method)?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET filter_method = ?1 updated_at = ?2 WHERE id = ?3",
            params![filter_method_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn get_iteration_method(&self) -> Result<IterationMethod> {
        let id = self.setting_id()?;
        let s: String = self.conn.query_row(
            "SELECT iteration_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(serde_json::from_str(&s)?)
    }

    pub fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        let id = self.setting_id()?;
        let iteration_method_str = serde_json::to_string(&iteration_method)?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET  = ?1 iteration_method updated_at = ?2 WHERE id = ?3",
            params![iteration_method_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn get_physical_param(&self) -> Result<PhysicalParam> {
        let id = self.setting_id()?;
        let physical_param = self.conn.query_row(
            "
            SELECT (
                peak_temperature,
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                characteristic_length,
                air_thermal_conductivity,
            ) 
            FROM 
                settings 
            WHERE
                id = ?1
            ",
            [id],
            |row| {
                Ok(PhysicalParam {
                    peak_temperature: row.get(0)?,
                    solid_thermal_conductivity: row.get(1)?,
                    solid_thermal_diffusivity: row.get(2)?,
                    characteristic_length: row.get(3)?,
                    air_thermal_conductivity: row.get(4)?,
                })
            },
        )?;

        Ok(physical_param)
    }

    fn set_start_index(&self, start_frame: usize, start_row: usize) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET start_frame = ?1, start_row = ?2 updated_at = ?3 WHERE id = ?4",
            params![start_frame, start_row, updated_at, id],
        )?;

        Ok(())
    }
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
    // TODO: hash
}

impl SettingSnapshot {
    #[instrument(fields(setting_path = setting_path.as_ref().to_str()))]
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
    start_frame: usize,
    /// Start row of DAQ data involved in the calculation.
    start_row: usize,
}
