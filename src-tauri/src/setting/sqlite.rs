use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, Error::QueryReturnedNoRows};

use crate::{
    daq::{DaqMetadata, Thermocouple},
    error::Error,
    solve::{IterationMethod, PhysicalParam},
    util,
    video::{FilterMetadata, FilterMethod, Green2Metadata, VideoMetadata},
};

use super::{CreateRequest, SettingStorage, StartIndex};

#[derive(Debug)]
pub struct SqliteSettingStorage {
    conn: Connection,
    /// Setting id of the experiment which is currently being processed.
    /// `setting_id` should be manually updated by the user and will be
    /// used for all single row operations automatically.
    setting_id: Option<i64>,
}

impl SqliteSettingStorage {
    pub fn new() -> Self {
        const DB_FILEPATH: &str = "./var/db.sqlite3";
        let conn = Connection::open(DB_FILEPATH)
            .unwrap_or_else(|e| panic!("Failed to create/open metadata db at {DB_FILEPATH}: {e}"));
        conn.execute(include_str!("../../db/schema.sql"), ())
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

    fn video_metadata_inner(&self) -> Result<Option<VideoMetadata>> {
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

    fn daq_metadata_inner(&self) -> Result<Option<DaqMetadata>> {
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

    fn set_start_index(&self, start_frame: usize, start_row: usize) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET start_frame = ?1, start_row = ?2 updated_at = ?3 WHERE id = ?4",
            params![start_frame, start_row, updated_at, id],
        )?;

        Ok(())
    }

    fn thermocouples_inner(&self) -> Result<Option<Vec<Thermocouple>>> {
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
}

impl SettingStorage for SqliteSettingStorage {
    fn create_setting(&mut self, request: CreateRequest) -> Result<i64> {
        let CreateRequest {
            name,
            save_root_dir,
            filter_method,
            iteration_method,
            physical_param:
                PhysicalParam {
                    peak_temperature,
                    solid_thermal_conductivity,
                    solid_thermal_diffusivity,
                    characteristic_length,
                    air_thermal_conductivity,
                },
        } = request;
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
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ",
            )?
            .insert(params![
                name,
                save_root_dir,
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

        Ok(id)
    }

    fn switch_setting(&mut self, setting_id: i64) -> Result<()> {
        if Some(setting_id) == self.setting_id {
            // The caller will reload everything even if the setting id has not changed.
            return Ok(());
        }
        let _: i32 = self
            .conn
            .query_row(
                "SELECT 1 FROM settings WHERE id = ?1",
                [setting_id],
                |row| row.get(0),
            )
            .map_err::<anyhow::Error, _>(|e| match e {
                QueryReturnedNoRows => Error::SettingIdNotFound(setting_id).into(),
                _ => e.into(),
            })?;
        self.setting_id = Some(setting_id);

        Ok(())
    }

    fn delete_setting(&mut self, setting_id: i64) -> Result<()> {
        unimplemented!()
    }

    fn save_root_dir(&self) -> Result<String> {
        let id = self.setting_id()?;
        let save_root_dir = self.conn.query_row(
            "SELECT save_root_dir FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(save_root_dir)
    }

    fn set_save_root_dir(&self, save_root_dir: PathBuf) -> Result<()> {
        let id = self.setting_id()?;
        let save_root_dir = save_root_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid save_root_dir: {save_root_dir:?}"))?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET save_root_dir = ?1, updated_at = ?2 WHERE id = ?3",
            params![save_root_dir, updated_at, id],
        )?;

        Ok(())
    }

    fn video_metadata(&self) -> Result<VideoMetadata> {
        self.video_metadata_inner()?
            .ok_or_else(|| anyhow!("video metadata not loaded yet"))
    }

    /// Compare the new `video_metadata` with the old one to make minimal updates.
    fn set_video_metadata(&self, video_metadata: VideoMetadata) -> Result<()> {
        let id = self.setting_id()?;
        match self.video_metadata_inner()? {
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
                // Most of the time we can make use of the previous position setting rather
                // than directly invalidate it because within a series of experiments the
                // position settings should be similar.
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
                    WHERE id = ?3
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
                    WHERE id = ?4
                    ",
                    params![video_metadata_str, area_str, updated_at, id],
                )?;
                Ok(())
            }
        }
    }

    fn daq_metadata(&self) -> Result<DaqMetadata> {
        self.daq_metadata_inner()?
            .ok_or_else(|| anyhow!("daq metadata not loaded yet"))
    }

    fn set_daq_metadata(&self, daq_metadata: DaqMetadata) -> Result<()> {
        let id = self.setting_id()?;
        match self.daq_metadata_inner()? {
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
                let thermocouples = self.thermocouples_inner()?;
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
                            WHERE id = ?3
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
                    WHERE id = ?3
                    ",
                    params![daq_metadata_str, updated_at, id],
                )?;
                Ok(())
            }
        }
    }

    fn start_index(&self) -> Result<StartIndex> {
        let id = self.setting_id()?;
        let ret: (Option<usize>, Option<usize>) = self.conn.query_row(
            "SELECT (start_frame, start_row) FROM settings WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        match ret {
            (None, None) => bail!("video and daq not synchronized yet"),
            (Some(start_frame), Some(start_row)) => Ok(StartIndex {
                start_frame,
                start_row,
            }),
            _ => unreachable!("start_frame and start_row are not consistent"),
        }
    }

    fn synchronize_video_and_daq(&self, start_frame: usize, start_row: usize) -> Result<()> {
        let nframes = self
            .video_metadata_inner()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }
        let nrows = self
            .daq_metadata_inner()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.set_start_index(start_frame, start_row)
    }

    fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        let nframes = self
            .video_metadata_inner()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.start_index()?;

        let nrows = self
            .daq_metadata_inner()?
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

    fn set_start_row(&self, start_row: usize) -> Result<()> {
        let nrows = self
            .daq_metadata_inner()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.start_index()?;

        let nframes = self
            .video_metadata_inner()?
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

    fn area(&self) -> Result<(usize, usize, usize, usize)> {
        let id = self.setting_id()?;
        let ret: Option<String> =
            self.conn
                .query_row("SELECT area FROM settings WHERE id = ?1", [id], |row| {
                    row.get(0)
                })?;

        match ret {
            Some(s) => Ok(serde_json::from_str(&s)?),
            None => bail!("area not selected yet"),
        }
    }

    fn set_area(&self, area: (usize, usize, usize, usize)) -> Result<()> {
        let id = self.setting_id()?;
        let (h, w) = self
            .video_metadata_inner()?
            .ok_or_else(|| anyhow!("video path unset"))?
            .shape;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_x + cal_w > w {
            bail!("area X out of range: top_left_x({tl_x}) + width({cal_w}) > video_width({w})");
        }
        if tl_y + cal_h > h {
            bail!("area Y out of range: top_left_y({tl_y}) + height({cal_h}) > video_height({h})");
        }
        let area_str = serde_json::to_string(&area)?;

        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET area = ?1, updated_at = ?2 WHERE id = ?3",
            params![area_str, updated_at, id],
        )?;

        Ok(())
    }

    fn green2_metadata(&self) -> Result<Green2Metadata> {
        let video_metadata = self
            .video_metadata_inner()?
            .ok_or_else(|| anyhow!("video path unset"))?;
        let nrows = self
            .daq_metadata_inner()?
            .ok_or_else(|| anyhow!("daq  path unset"))?
            .nrows;
        let StartIndex {
            start_frame,
            start_row,
        } = self.start_index()?;
        let area = self.area()?;

        let nframes = video_metadata.nframes;
        let cal_num = (nframes - start_frame).min(nrows - start_row);

        Ok(Green2Metadata {
            start_frame,
            cal_num,
            area,
            video_fingerprint: video_metadata.fingerprint,
        })
    }

    fn thermocouples(&self) -> Result<Vec<Thermocouple>> {
        self.thermocouples_inner()?
            .ok_or_else(|| anyhow!("thermocouples not selected yet"))
    }

    fn filter_metadata(&self) -> Result<FilterMetadata> {
        let id = self.setting_id()?;
        let (video_metadata_str, filter_method_str): (String, String) = self.conn.query_row(
            "SELECT video_metadata, filter_method FROM settings WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let video_metadata: VideoMetadata = serde_json::from_str(&video_metadata_str)?;
        let filter_method = serde_json::from_str(&filter_method_str)?;

        Ok(FilterMetadata {
            filter_method,
            video_fingerprint: video_metadata.fingerprint,
        })
    }

    fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let id = self.setting_id()?;
        let filter_method_str = serde_json::to_string(&filter_method)?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET filter_method = ?1 updated_at = ?2 WHERE id = ?3",
            params![filter_method_str, updated_at, id],
        )?;

        Ok(())
    }

    fn iteration_method(&self) -> Result<IterationMethod> {
        let id = self.setting_id()?;
        let s: String = self.conn.query_row(
            "SELECT iteration_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(serde_json::from_str(&s)?)
    }

    fn set_iteration_method(&self, iteration_method: IterationMethod) -> Result<()> {
        let id = self.setting_id()?;
        let iteration_method_str = serde_json::to_string(&iteration_method)?;
        let updated_at = util::time::now_as_secs();
        self.conn.execute(
            "UPDATE settings SET  = ?1 iteration_method updated_at = ?2 WHERE id = ?3",
            params![iteration_method_str, updated_at, id],
        )?;

        Ok(())
    }

    fn physical_param(&self) -> Result<PhysicalParam> {
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
            FROM settings 
            WHERE id = ?1
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
}
