use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, Error::QueryReturnedNoRows};
use video::{FilterMeta, FilterMethod, VideoMeta};

use super::{CreateRequest, SettingStorage, StartIndex};
use crate::{
    daq::{InterpMethod, Thermocouple},
    solve::{IterationMethod, PhysicalParam},
    util,
};

#[derive(Debug)]
pub struct SqliteSettingStorage {
    conn: Connection,
    /// Setting id of the experiment which is currently being processed.
    /// `setting_id` should be manually updated by the user and will be
    /// used for all single row operations automatically.
    setting_id: Option<i64>,
}

impl SqliteSettingStorage {
    pub fn new<P: AsRef<Path>>(path: P) -> SqliteSettingStorage {
        let conn = Connection::open(path)
            .unwrap_or_else(|e| panic!("Failed to create/open metadata db: {e}"));
        conn.execute(include_str!("../../db/schema.sql"), ())
            .expect("Failed to create db");

        Self {
            conn,
            setting_id: None,
        }
    }

    #[cfg(test)]
    pub fn new_in_memory() -> SqliteSettingStorage {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(include_str!("../../db/schema.sql"), ())
            .expect("Failed to create db");

        SqliteSettingStorage {
            conn,
            setting_id: None,
        }
    }

    fn setting_id(&self) -> Result<i64> {
        self.setting_id
            .ok_or_else(|| anyhow!("no experiment setting is selected"))
    }
}

impl SettingStorage for SqliteSettingStorage {
    fn create_setting(&mut self, request: CreateRequest) -> Result<()> {
        let CreateRequest {
            name,
            save_root_dir,
            filter_method,
            iteration_method,
            physical_param:
                PhysicalParam {
                    gmax_temperature,
                    solid_thermal_conductivity,
                    solid_thermal_diffusivity,
                    characteristic_length,
                    air_thermal_conductivity,
                },
        } = request;
        let filter_method_str = serde_json::to_string(&filter_method)?;
        let iteration_method_str = serde_json::to_string(&iteration_method)?;
        let created_at = util::time::now_as_millis();
        let id = self
            .conn
            .prepare(
                "
                INSERT INTO settings (
                    name,
                    save_root_dir,
                    filter_method,
                    iteration_method,
                    gmax_temperature,
                    solid_thermal_conductivity,
                    solid_thermal_diffusivity,
                    characteristic_length,
                    air_thermal_conductivity,
                    completed_at,
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
                gmax_temperature,
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                characteristic_length,
                air_thermal_conductivity,
                0,
                created_at,
                created_at,
            ])?;
        self.setting_id = Some(id);

        Ok(())
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
                QueryReturnedNoRows => anyhow!("setting dost not exist"),
                _ => e.into(),
            })?;
        self.setting_id = Some(setting_id);

        Ok(())
    }

    fn delete_setting(&mut self) -> Result<()> {
        let id = self.setting_id()?;
        self.conn
            .execute("DELETE FROM settings WHERE id = ?1", [id])?;

        Ok(())
    }

    fn name(&self) -> Result<String> {
        let id = self.setting_id()?;
        let name = self
            .conn
            .query_row("SELECT name FROM settings WHERE id = ?1", [id], |row| {
                row.get(0)
            })?;

        Ok(name)
    }

    fn set_name(&self, name: &str) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, updated_at, id],
        )?;

        Ok(())
    }

    fn save_root_dir(&self) -> Result<PathBuf> {
        let id = self.setting_id()?;
        let save_root_dir_str: String = self.conn.query_row(
            "SELECT save_root_dir FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        let save_root_dir = PathBuf::from(save_root_dir_str);

        Ok(save_root_dir)
    }

    fn set_save_root_dir(&self, save_root_dir: &Path) -> Result<()> {
        let id = self.setting_id()?;
        let save_root_dir = save_root_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid save_root_dir: {save_root_dir:?}"))?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET save_root_dir = ?1, updated_at = ?2 WHERE id = ?3",
            params![save_root_dir, updated_at, id],
        )?;

        Ok(())
    }

    fn video_path(&self) -> Result<PathBuf> {
        todo!()
    }

    fn video_meta_optional(&self) -> Result<Option<VideoMeta>> {
        let id = self.setting_id()?;
        let ret: Option<String> = self.conn.query_row(
            "SELECT video_meta FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    /// Compare the new `video_metadata` with the old one to make minimal updates.
    fn set_video_meta(&self, video_meta: &VideoMeta) -> Result<()> {
        let id = self.setting_id()?;
        match self.video_meta_optional()? {
            Some(old_video_meta) if old_video_meta.path == video_meta.path => Ok(()),
            Some(old_video_meta) if old_video_meta.shape == video_meta.shape => {
                // Most of the time we can make use of the previous position setting rather
                // than directly invalidate it because within a series of experiments the
                // position settings should be similar.
                let video_meta_str = serde_json::to_string(&video_meta)?;
                let updated_at = util::time::now_as_millis();
                // Reset start_frame and start_row but reuse area and thermocouples.
                self.conn.execute(
                    "
                    UPDATE settings SET
                        video_meta = ?1,
                        start_frame = NULL,
                        start_row = NULL,
                        updated_at = ?2
                    WHERE id = ?3
                    ",
                    params![video_meta_str, updated_at, id],
                )?;
                Ok(())
            }
            _ => {
                // We will only get this point when working with a brand new setting
                // or different camera parameters. Then we just put the select box in
                // the center by default.
                let video_meta_str = serde_json::to_string(&video_meta)?;
                let (h, w) = video_meta.shape;
                let area = (h / 4, w / 4, h / 2, w / 2);
                let area_str = serde_json::to_string(&area)?;
                let updated_at = util::time::now_as_millis();
                // Reset start_frame, start_row, area and thermocouples.
                self.conn.execute(
                    "
                    UPDATE settings SET
                        video_meta = ?1,
                        start_frame = NULL,
                        start_row = NULL,
                        area = ?2,
                        thermocouples = NULL,
                        updated_at = ?3
                    WHERE id = ?4
                    ",
                    params![video_meta_str, area_str, updated_at, id],
                )?;
                Ok(())
            }
        }
    }

    fn set_video_path(&self, _video_path: &Path) -> Result<()> {
        todo!()
    }

    // fn set_daq_metadata(&self, daq_metadata: &DaqMetadata) -> Result<()> {
    //     let id = self.setting_id()?;
    //     match self.daq_metadata_optional()? {
    //         Some(old_daq_metadata) if old_daq_metadata.path == daq_metadata.path => Ok(()),
    //         _ => {
    //             let thermocouples = self.thermocouples_optional()?;
    //             let daq_metadata_str = serde_json::to_string(&daq_metadata)?;
    //             let updated_at = util::time::now_as_millis();
    //
    //             if let Some(thermocouples) = thermocouples {
    //                 if thermocouples
    //                     .iter()
    //                     .any(|t| t.column_index >= daq_metadata.ncols)
    //                 {
    //                     self.conn.execute(
    //                         "
    //                         UPDATE settings SET
    //                             daq_metadata = ?1,
    //                             start_frame = NULL,
    //                             start_row = NULL,
    //                             thermocouples = NULL,
    //                             updated_at = ?2
    //                         WHERE id = ?3
    //                         ",
    //                         params![daq_metadata_str, updated_at, id],
    //                     )?;
    //                 }
    //                 return Ok(());
    //             }
    //
    //             self.conn.execute(
    //                 "
    //                 UPDATE settings SET
    //                     daq_metadata = ?1,
    //                     start_frame = NULL,
    //                     start_row = NULL,
    //                     updated_at = ?2
    //                 WHERE id = ?3
    //                 ",
    //                 params![daq_metadata_str, updated_at, id],
    //             )?;
    //             Ok(())
    //         }
    //     }
    // }

    fn daq_path(&self) -> Result<PathBuf> {
        todo!()
    }

    fn set_daq_path(&self, daq_path: &Path) -> Result<()> {
        todo!()
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

    fn set_start_index(&self, start_frame: usize, start_row: usize) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET start_frame = ?1, start_row = ?2 updated_at = ?3 WHERE id = ?4",
            params![start_frame, start_row, updated_at, id],
        )?;

        Ok(())
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
            .video_meta_optional()?
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

        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET area = ?1, updated_at = ?2 WHERE id = ?3",
            params![area_str, updated_at, id],
        )?;

        Ok(())
    }

    fn thermocouples_optional(&self) -> Result<Option<Vec<Thermocouple>>> {
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

    fn interp_method(&self) -> Result<InterpMethod> {
        let id = self.setting_id()?;
        let interpolatioin_method_str: String = self.conn.query_row(
            "SELECT interpolation_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        let interpolation_method = serde_json::from_str(&interpolatioin_method_str)?;

        Ok(interpolation_method)
    }

    fn set_interp_method(&self, interp_method: InterpMethod) -> Result<()> {
        let id = self.setting_id()?;
        let interp_method_str = serde_json::to_string(&interp_method)?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET interp_method = ?1 updated_at = ?2 WHERE id = ?3",
            params![interp_method_str, updated_at, id],
        )?;

        Ok(())
    }

    fn filter_meta(&self) -> Result<FilterMeta> {
        let id = self.setting_id()?;
        let filter_method_str: String = self.conn.query_row(
            "SELECT filter_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        let filter_method = serde_json::from_str(&filter_method_str)?;
        let green2_meta = self.green2_meta()?;

        Ok(FilterMeta {
            filter_method,
            green2_meta,
        })
    }

    fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let id = self.setting_id()?;
        let filter_method_str = serde_json::to_string(&filter_method)?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET filter_method = ?1, updated_at = ?2 WHERE id = ?3",
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
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET iteration_method = ?1, updated_at = ?2 WHERE id = ?3",
            params![iteration_method_str, updated_at, id],
        )?;

        Ok(())
    }

    fn physical_param(&self) -> Result<PhysicalParam> {
        let id = self.setting_id()?;
        let physical_param = self.conn.query_row(
            "
            SELECT (
                gmax_temperature,
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
                    gmax_temperature: row.get(0)?,
                    solid_thermal_conductivity: row.get(1)?,
                    solid_thermal_diffusivity: row.get(2)?,
                    characteristic_length: row.get(3)?,
                    air_thermal_conductivity: row.get(4)?,
                })
            },
        )?;

        Ok(physical_param)
    }

    fn set_gmax_temperature(&self, gmax_temperature: f64) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET gmax_temperature = ?1, updated_at = ?2 WHERE id = ?3",
            params![gmax_temperature, updated_at, id],
        )?;

        Ok(())
    }

    fn set_solid_thermal_conductivity(&self, solid_thermal_conductivity: f64) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET solid_thermal_conductivity = ?1, updated_at = ?2 WHERE id = ?3",
            params![solid_thermal_conductivity, updated_at, id],
        )?;

        Ok(())
    }

    fn set_solid_thermal_diffusivity(&self, solid_thermal_diffusivity: f64) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET solid_thermal_diffusivity = ?1, updated_at = ?2 WHERE id = ?3",
            params![solid_thermal_diffusivity, updated_at, id],
        )?;

        Ok(())
    }

    fn set_characteristic_length(&self, characteristic_length: f64) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET characteristic_length = ?1, updated_at = ?2 WHERE id = ?3",
            params![characteristic_length, updated_at, id],
        )?;

        Ok(())
    }

    fn set_air_thermal_conductivity(&self, air_thermal_conductivity: f64) -> Result<()> {
        let id = self.setting_id()?;
        let updated_at = util::time::now_as_millis();
        self.conn.execute(
            "UPDATE settings SET air_thermal_conductivity = ?1, updated_at = ?2 WHERE id = ?3",
            params![air_thermal_conductivity, updated_at, id],
        )?;

        Ok(())
    }
}
