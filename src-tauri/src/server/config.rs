use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::{TLCConfig, TLCHandler, TLCStorage};
use tracing::debug;

impl TLCHandler {
    pub fn reload_config<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        *self = Self::from_path(path)?;
        Ok(())
    }
}

enum Save {
    Config,
    Nu,
    Plot,
}

impl TLCConfig {
    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        debug!("{:?}", path.as_ref());

        Ok(())
    }

    fn _save(&self) -> Result<()> {
        self.save_to_path(self.storage.get_save_path(Save::Config)?)
    }
}

#[derive(Serialize)]
pub struct SaveInfo {
    save_root_dir: PathBuf,
    config_path: PathBuf,
    nu_path: PathBuf,
    plot_path: PathBuf,
}

impl TLCStorage {
    pub fn get_save_info(&self) -> Result<SaveInfo> {
        match self.save_root_dir {
            Some(ref save_root_dir) => Ok(SaveInfo {
                save_root_dir: save_root_dir.to_owned(),
                config_path: self.get_config_path()?,
                nu_path: self.get_nu_path()?,
                plot_path: self.get_plot_path()?,
            }),
            None => bail!("save root dir unset"),
        }
    }

    pub fn get_config_path(&self) -> Result<PathBuf> {
        self.get_save_path(Save::Config)
    }

    pub fn get_nu_path(&self) -> Result<PathBuf> {
        self.get_save_path(Save::Nu)
    }

    pub fn get_plot_path(&self) -> Result<PathBuf> {
        self.get_save_path(Save::Plot)
    }

    /// case_name is always extracted from the current video_path so we do not need
    /// to take care of invalidation.
    fn get_case_name(&self) -> Result<&OsStr> {
        let video_path = self
            .video_path
            .as_ref()
            .ok_or(anyhow!("video path unset"))?;

        Ok(video_path
            .file_stem()
            .ok_or(anyhow!("invalid video path: {:?}", video_path))?)
    }

    fn get_save_path(&self, save: Save) -> Result<PathBuf> {
        let (dir, ext) = match save {
            Save::Config => ("config", "toml"),
            Save::Nu => ("nu", "csv"),
            Save::Plot => ("plot", "png"),
        };

        Ok(self
            .save_root_dir
            .as_ref()
            .ok_or(anyhow!("save root dir unset"))?
            .join(dir)
            .join(self.get_case_name()?)
            .with_extension(ext))
    }
}
