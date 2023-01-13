use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, Error::QueryReturnedNoRows};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{CreateRequest, StartIndex};
use crate::{
    daq::{InterpMethod, Thermocouple},
    solve::{IterationMethod, PhysicalParam},
    video::FilterMethod,
};

#[derive(Debug, Default)]
pub struct Setting {
    /// Setting id of the experiment which is currently being processed
    /// as well as the primary key of SQLite `settings` table.
    /// `id` should be manually updated by the user and will be used for
    /// all single row operations automatically.
    id: Option<i64>,
}

impl Setting {
    pub fn create_setting(&mut self, conn: &Connection, request: CreateRequest) -> Result<()> {
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
            video_path,
            daq_path,
            start_frame,
            start_row,
            area,
            thermocouples,
            interp_method,
        } = request;
        let save_root_dir = save_root_dir.to_str().unwrap_or_default();
        let video_path = video_path.as_ref().and_then(|x| x.to_str());
        let daq_path = daq_path.as_ref().and_then(|x| x.to_str());
        let area = area.and_then(|x| serde_json::to_string(&x).ok());
        let thermocouples = thermocouples.and_then(|x| serde_json::to_string(&x).ok());
        let interp_method = interp_method.and_then(|x| serde_json::to_string(&x).ok());
        let filter_method_str = serde_json::to_string(&filter_method)?;
        let iteration_method_str = serde_json::to_string(&iteration_method)?;
        let created_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        let id = conn
            .prepare(
                "
                INSERT INTO settings (
                    name,
                    save_root_dir,
                    video_path,
                    daq_path,
                    start_frame,
                    start_row,
                    area,
                    thermocouples,
                    interp_method,
                    filter_method,
                    iteration_method,
                    gmax_temperature,
                    solid_thermal_conductivity,
                    solid_thermal_diffusivity,
                    characteristic_length,
                    air_thermal_conductivity,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                ",
            )?
            .insert(params![
                name,
                save_root_dir,
                video_path,
                daq_path,
                start_frame,
                start_row,
                area,
                thermocouples,
                interp_method,
                filter_method_str,
                iteration_method_str,
                gmax_temperature,
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                characteristic_length,
                air_thermal_conductivity,
                created_at,
                created_at,
            ])?;
        self.id = Some(id);

        Ok(())
    }

    pub fn switch_setting(&mut self, conn: &Connection, setting_id: i64) -> Result<()> {
        if Some(setting_id) == self.id {
            // The caller will reload everything even if the setting id has not changed.
            return Ok(());
        }
        let _: i32 = conn
            .query_row(
                "SELECT 1 FROM settings WHERE id = ?1",
                [setting_id],
                |row| row.get(0),
            )
            .map_err::<anyhow::Error, _>(|e| match e {
                QueryReturnedNoRows => anyhow!("setting dost not exist"),
                _ => e.into(),
            })?;
        self.id = Some(setting_id);

        Ok(())
    }

    pub fn delete_setting(&mut self, conn: &Connection, setting_id: i64) -> Result<()> {
        if self.id == Some(setting_id) {
            self.id = None;
        }
        conn.execute("DELETE FROM settings WHERE id = ?1", [setting_id])?;
        Ok(())
    }

    pub fn name(&self, conn: &Connection) -> Result<String> {
        let id = self.id()?;
        let name = conn.query_row("SELECT name FROM settings WHERE id = ?1", [id], |row| {
            row.get(0)
        })?;

        Ok(name)
    }

    pub fn set_name(&self, conn: &Connection, name: &str) -> Result<()> {
        let id = self.id()?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, updated_at, id],
        )?;

        Ok(())
    }

    pub fn save_root_dir(&self, conn: &Connection) -> Result<PathBuf> {
        let id = self.id()?;
        let save_root_dir_str: String = conn.query_row(
            "SELECT save_root_dir FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        let save_root_dir = PathBuf::from(save_root_dir_str);

        Ok(save_root_dir)
    }

    pub fn set_save_root_dir(&self, conn: &Connection, save_root_dir: &Path) -> Result<()> {
        let id = self.id()?;
        let save_root_dir = save_root_dir
            .to_str()
            .ok_or_else(|| anyhow!("invalid save_root_dir: {save_root_dir:?}"))?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET save_root_dir = ?1, updated_at = ?2 WHERE id = ?3",
            params![save_root_dir, updated_at, id],
        )?;

        Ok(())
    }

    pub fn video_path(&self, conn: &Connection) -> Result<PathBuf> {
        let id = self.id()?;
        let ret: Option<String> = conn.query_row(
            "SELECT video_path FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(PathBuf::from(s)),
            None => bail!("video path unset"),
        }
    }

    pub fn set_video_path(&self, conn: &Connection, video_path: &Path) -> Result<()> {
        let id = self.id()?;
        let video_path = video_path.to_str().ok_or_else(|| anyhow!("invliad path"))?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET video_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![video_path, updated_at, id],
        )?;

        Ok(())
    }

    pub fn daq_path(&self, conn: &Connection) -> Result<PathBuf> {
        let id = self.id()?;
        let ret: Option<String> =
            conn.query_row("SELECT daq_path FROM settings WHERE id = ?1", [id], |row| {
                row.get(0)
            })?;

        match ret {
            Some(s) => Ok(PathBuf::from(s)),
            None => bail!("daq path unset"),
        }
    }

    pub fn set_daq_path(&self, conn: &Connection, daq_path: &Path) -> Result<()> {
        let id = self.id()?;
        let daq_path = daq_path.to_str().ok_or_else(|| anyhow!("invliad path"))?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET daq_path = ?1, updated_at = ?2 WHERE id = ?3",
            params![daq_path, updated_at, id],
        )?;

        Ok(())
    }

    pub fn start_index(&self, conn: &Connection) -> Result<Option<StartIndex>> {
        let id = self.id()?;
        let ret: (Option<usize>, Option<usize>) = conn.query_row(
            "SELECT start_frame, start_row FROM settings WHERE id = ?1",
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

    pub fn set_start_index(
        &self,
        conn: &Connection,
        start_index: Option<StartIndex>,
    ) -> Result<()> {
        let id = self.id()?;
        let (start_frame, start_row) = match start_index {
            Some(StartIndex {
                start_frame,
                start_row,
            }) => (Some(start_frame), Some(start_row)),
            None => (None, None),
        };

        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET start_frame = ?1, start_row = ?2, updated_at = ?3 WHERE id = ?4",
            params![start_frame, start_row, updated_at, id],
        )?;

        Ok(())
    }

    pub fn area(&self, conn: &Connection) -> Result<Option<(u32, u32, u32, u32)>> {
        let id = self.id()?;
        let ret: Option<String> =
            conn.query_row("SELECT area FROM settings WHERE id = ?1", [id], |row| {
                row.get(0)
            })?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_area(&self, conn: &Connection, area: Option<(u32, u32, u32, u32)>) -> Result<()> {
        let id = self.id()?;
        let area_str = serde_json::to_string(&area)?;

        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET area = ?1, updated_at = ?2 WHERE id = ?3",
            params![area_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn thermocouples(&self, conn: &Connection) -> Result<Option<Vec<Thermocouple>>> {
        let id = self.id()?;
        let ret: Option<String> = conn.query_row(
            "SELECT thermocouples FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_thermocouples(
        &self,
        conn: &Connection,
        thermocouples: Option<&[Thermocouple]>,
    ) -> Result<()> {
        let id = self.id()?;
        let thermocouples_str = match thermocouples {
            Some(thermocouples) => Some(serde_json::to_string(thermocouples)?),
            None => None,
        };
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET thermocouples = ?1, updated_at = ?2 WHERE id = ?3",
            params![thermocouples_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn interp_method(&self, conn: &Connection) -> Result<Option<InterpMethod>> {
        let id = self.id()?;
        let ret: Option<String> = conn.query_row(
            "SELECT interp_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        match ret {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn set_interp_method(&self, conn: &Connection, interp_method: InterpMethod) -> Result<()> {
        let id = self.id()?;
        let interp_method_str = serde_json::to_string(&interp_method)?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET interp_method = ?1, updated_at = ?2 WHERE id = ?3",
            params![interp_method_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn filter_method(&self, conn: &Connection) -> Result<FilterMethod> {
        let id = self.id()?;
        let filter_method_str: String = conn.query_row(
            "SELECT filter_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(serde_json::from_str(&filter_method_str)?)
    }

    pub fn set_filter_method(&self, conn: &Connection, filter_method: FilterMethod) -> Result<()> {
        let id = self.id()?;
        let filter_method_str = serde_json::to_string(&filter_method)?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET filter_method = ?1, updated_at = ?2 WHERE id = ?3",
            params![filter_method_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn iteration_method(&self, conn: &Connection) -> Result<IterationMethod> {
        let id = self.id()?;
        let s: String = conn.query_row(
            "SELECT iteration_method FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(serde_json::from_str(&s)?)
    }

    pub fn set_iteration_method(
        &self,
        conn: &Connection,
        iteration_method: IterationMethod,
    ) -> Result<()> {
        let id = self.id()?;
        let iteration_method_str = serde_json::to_string(&iteration_method)?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET iteration_method = ?1, updated_at = ?2 WHERE id = ?3",
            params![iteration_method_str, updated_at, id],
        )?;

        Ok(())
    }

    pub fn physical_param(&self, conn: &Connection) -> Result<PhysicalParam> {
        let id = self.id()?;
        let physical_param = conn.query_row(
            "
            SELECT
                gmax_temperature,
                solid_thermal_conductivity,
                solid_thermal_diffusivity,
                characteristic_length,
                air_thermal_conductivity
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

    pub fn set_gmax_temperature(&self, conn: &Connection, gmax_temperature: f64) -> Result<()> {
        let id = self.id()?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET gmax_temperature = ?1, updated_at = ?2 WHERE id = ?3",
            params![gmax_temperature, updated_at, id],
        )?;

        Ok(())
    }

    pub fn set_solid_thermal_conductivity(
        &self,
        conn: &Connection,
        solid_thermal_conductivity: f64,
    ) -> Result<()> {
        let id = self.id()?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET solid_thermal_conductivity = ?1, updated_at = ?2 WHERE id = ?3",
            params![solid_thermal_conductivity, updated_at, id],
        )?;

        Ok(())
    }

    pub fn set_solid_thermal_diffusivity(
        &self,
        conn: &Connection,
        solid_thermal_diffusivity: f64,
    ) -> Result<()> {
        let id = self.id()?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET solid_thermal_diffusivity = ?1, updated_at = ?2 WHERE id = ?3",
            params![solid_thermal_diffusivity, updated_at, id],
        )?;

        Ok(())
    }

    pub fn set_characteristic_length(
        &self,
        conn: &Connection,
        characteristic_length: f64,
    ) -> Result<()> {
        let id = self.id()?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET characteristic_length = ?1, updated_at = ?2 WHERE id = ?3",
            params![characteristic_length, updated_at, id],
        )?;

        Ok(())
    }

    pub fn set_air_thermal_conductivity(
        &self,
        conn: &Connection,
        air_thermal_conductivity: f64,
    ) -> Result<()> {
        let id = self.id()?;
        let updated_at = OffsetDateTime::now_local()?.format(&Rfc3339)?;
        conn.execute(
            "UPDATE settings SET air_thermal_conductivity = ?1, updated_at = ?2 WHERE id = ?3",
            params![air_thermal_conductivity, updated_at, id],
        )?;

        Ok(())
    }

    fn id(&self) -> Result<i64> {
        self.id
            .ok_or_else(|| anyhow!("no experiment setting is selected"))
    }

    #[cfg(test)]
    fn updated_at(&self, conn: &Connection) -> Result<String> {
        let id = self.id()?;
        let updated_at = conn.query_row(
            "SELECT updated_at FROM settings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;

        Ok(updated_at)
    }
}

#[cfg(test)]
mod tests {
    use crate::setting::new_db_in_memory;

    use super::*;

    const NAME: &str = "test_setting";
    const SAVE_ROOT_DIR: &str = "save_root_dir";
    const PHYSICAL_PARAM: PhysicalParam = PhysicalParam {
        gmax_temperature: 35.48,
        solid_thermal_conductivity: 0.19,
        solid_thermal_diffusivity: 1.091e-7,
        characteristic_length: 0.015,
        air_thermal_conductivity: 0.0276,
    };

    #[test]
    fn test_brand_new_setting() {
        let (setting, db) = _new_db();
        assert_eq!(setting.name(&db).unwrap(), NAME);
        assert_eq!(
            setting.save_root_dir(&db).unwrap(),
            PathBuf::from(SAVE_ROOT_DIR)
        );
        setting.video_path(&db).unwrap_err();
        setting.daq_path(&db).unwrap_err();
        assert!(setting.start_index(&db).unwrap().is_none());
        assert!(setting.area(&db).unwrap().is_none());
        assert!(setting.thermocouples(&db).unwrap().is_none());
        assert!(setting.interp_method(&db).unwrap().is_none());
        assert_eq!(setting.filter_method(&db).unwrap(), FilterMethod::No);
        assert_eq!(
            setting.iteration_method(&db).unwrap(),
            IterationMethod::default()
        );
        assert_eq!(setting.physical_param(&db).unwrap(), PHYSICAL_PARAM);
    }

    #[test]
    fn test_rw_name() {
        let (setting, db) = _new_db();
        setting.set_name(&db, "aaa").unwrap();
        assert_eq!(setting.name(&db).unwrap(), "aaa");
        println!("{}", setting.updated_at(&db).unwrap());
    }

    #[test]
    fn test_rw_save_root_dir() {
        let (setting, db) = _new_db();
        setting
            .set_save_root_dir(&db, &PathBuf::from("aaa"))
            .unwrap();
        assert_eq!(setting.save_root_dir(&db).unwrap(), PathBuf::from("aaa"));
    }

    #[test]
    fn test_rw_video_path() {
        let (setting, db) = _new_db();
        setting.set_video_path(&db, &PathBuf::from("aaa")).unwrap();
        assert_eq!(setting.video_path(&db).unwrap(), PathBuf::from("aaa"));
    }

    #[test]
    fn test_rw_daq_path() {
        let (setting, db) = _new_db();
        setting.set_daq_path(&db, &PathBuf::from("aaa")).unwrap();
        assert_eq!(setting.daq_path(&db).unwrap(), PathBuf::from("aaa"));
    }

    #[test]
    fn test_rw_start_index() {
        let (setting, db) = _new_db();
        let start_index = StartIndex {
            start_frame: 10,
            start_row: 20,
        };
        setting.set_start_index(&db, Some(start_index)).unwrap();
        assert_eq!(setting.start_index(&db).unwrap().unwrap(), start_index);
    }

    #[test]
    fn test_rw_area() {
        let (setting, db) = _new_db();
        let area = (1, 2, 9, 18);
        setting.set_area(&db, Some(area)).unwrap();
        assert_eq!(setting.area(&db).unwrap().unwrap(), area);
    }

    #[test]
    fn test_rw_thermocouples() {
        let (setting, db) = _new_db();
        let thermocouples = vec![
            Thermocouple {
                column_index: 1,
                position: (-10, 20),
            },
            Thermocouple {
                column_index: 2,
                position: (0, 50),
            },
        ];
        setting
            .set_thermocouples(&db, Some(&thermocouples))
            .unwrap();
        assert_eq!(setting.thermocouples(&db).unwrap().unwrap(), thermocouples);
    }

    #[test]
    fn test_rw_interp_method() {
        let (setting, db) = _new_db();
        let interp_method = InterpMethod::BilinearExtra(2, 4);
        setting.set_interp_method(&db, interp_method).unwrap();
        assert_eq!(setting.interp_method(&db).unwrap().unwrap(), interp_method);
    }

    #[test]
    fn test_rw_filter_method() {
        let (setting, db) = _new_db();
        let filter_method = FilterMethod::Wavelet {
            threshold_ratio: 0.8,
        };
        setting.set_filter_method(&db, filter_method).unwrap();
        assert_eq!(setting.filter_method(&db).unwrap(), filter_method);
    }

    #[test]
    fn test_rw_iteration_method() {
        let (setting, db) = _new_db();
        let iteration_method = IterationMethod::NewtonTangent {
            h0: 60.0,
            max_iter_num: 20,
        };
        setting.set_iteration_method(&db, iteration_method).unwrap();
        assert_eq!(setting.iteration_method(&db).unwrap(), iteration_method);
    }

    #[test]
    fn test_rw_physical_param() {
        let (setting, db) = _new_db();
        setting.set_gmax_temperature(&db, 0.1).unwrap();
        setting.set_solid_thermal_conductivity(&db, 0.2).unwrap();
        setting.set_solid_thermal_diffusivity(&db, 0.3).unwrap();
        setting.set_characteristic_length(&db, 0.4).unwrap();
        setting.set_air_thermal_conductivity(&db, 0.5).unwrap();
        assert_eq!(
            setting.physical_param(&db).unwrap(),
            PhysicalParam {
                gmax_temperature: 0.1,
                solid_thermal_conductivity: 0.2,
                solid_thermal_diffusivity: 0.3,
                characteristic_length: 0.4,
                air_thermal_conductivity: 0.5,
            }
        );
    }

    fn _new_db() -> (Setting, Connection) {
        let db = new_db_in_memory();
        let mut setting = Setting { id: None };
        setting
            .create_setting(
                &db,
                CreateRequest {
                    name: NAME.to_owned(),
                    save_root_dir: PathBuf::from(SAVE_ROOT_DIR),
                    video_path: None,
                    daq_path: None,
                    start_frame: None,
                    start_row: None,
                    area: None,
                    thermocouples: None,
                    interp_method: None,
                    filter_method: FilterMethod::No,
                    iteration_method: IterationMethod::default(),
                    physical_param: PHYSICAL_PARAM,
                },
            )
            .unwrap();
        (setting, db)
    }
}
