mod cfg;
mod data;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use ndarray::ArcArray2;
use parking_lot::RwLock;
use tracing::debug;

pub use self::data::CalProgress;
use cfg::TLCConfig;
pub use cfg::{DAQMeta, VideoMeta};
pub use data::FilterMethod;
use data::TLCData;

pub struct TLCController {
    /// `cfg` can be mapped to a calculation result set and will be saved to disk.
    cfg: RwLock<TLCConfig>,

    /// `data` stores all runtime data and the calculation result set.
    data: TLCData,
}

impl TLCController {
    pub async fn new() -> Self {
        let mut cfg = TLCConfig::from_default_path().await;
        let data = TLCData::default();

        if let Some(video_path) = cfg.get_video_path() {
            if let Ok(video_meta) = data.read_video(video_path).await {
                cfg.on_video_load(video_meta);
            }
        }
        if let Some(daq_path) = cfg.get_daq_path() {
            if let Ok(daq_meta) = data.read_daq(daq_path).await {
                cfg.on_daq_load(daq_meta);
            }
        }

        if let Ok(g2_param) = cfg.get_g2_param() {
            data.build_g2(g2_param).filter(cfg.filter_method);
        }

        debug!("{:#?}", cfg);

        Self {
            cfg: RwLock::new(cfg),
            data,
        }
    }

    pub async fn load_config<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut cfg = TLCConfig::from_path(path).await?;
        self.data.reset().await;

        if let Some(video_path) = cfg.get_video_path() {
            let video_meta = self.data.read_video(video_path).await?;
            cfg.on_video_load(video_meta);
        }
        if let Some(daq_path) = cfg.get_daq_path() {
            let daq_meta = self.data.read_daq(daq_path).await?;
            cfg.on_daq_load(daq_meta);
        }

        debug!("{:#?}", cfg);

        *self.cfg.write() = cfg;

        Ok(())
    }

    pub fn get_save_root_dir(&self) -> Result<PathBuf> {
        Ok(self.cfg.read().get_save_root_dir()?.to_owned())
    }

    pub fn set_save_root_dir<P: AsRef<Path>>(&self, save_root_dir: P) {
        self.cfg.write().save_root_dir = Some(save_root_dir.as_ref().to_owned());
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        self.data.get_frame(frame_index).await
    }

    pub fn get_video_meta(&self) -> Result<VideoMeta> {
        self.cfg
            .read()
            .get_video_meta()
            .ok_or_else(|| anyhow!("video path unset"))
    }

    pub fn get_daq_meta(&self) -> Result<DAQMeta> {
        self.cfg
            .read()
            .get_daq_meta()
            .ok_or_else(|| anyhow!("daq path unset"))
    }

    // `set_video_path` has two side effects:
    // 1. Another thread is spawned to read from new video path.
    // 2. Some configurations are no longer valid so we need to update/invalidate them.
    pub async fn set_video_path<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMeta> {
        let video_meta = self.data.read_video(&video_path).await?;
        self.cfg.write().on_video_load(video_meta.clone());

        Ok(video_meta)
    }

    pub fn get_daq(&self) -> ArcArray2<f64> {
        self.data.get_daq()
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&self, daq_path: P) -> Result<DAQMeta> {
        let daq_meta = self.data.read_daq(&daq_path).await?;
        self.cfg.write().on_daq_load(daq_meta.clone());

        Ok(daq_meta)
    }

    pub fn set_start_frame(&self, start_frame: usize) -> Result<usize> {
        let mut cfg = self.cfg.write();
        let g2_param = cfg.set_start_frame(start_frame)?.get_g2_param()?;
        let cal_num = g2_param.frame_num;

        self.data.build_g2(g2_param).filter(cfg.filter_method);

        Ok(cal_num)
    }

    pub fn set_start_row(&self, start_row: usize) -> Result<usize> {
        let mut cfg = self.cfg.write();
        let g2_param = cfg.set_start_row(start_row)?.get_g2_param()?;
        let cal_num = g2_param.frame_num;

        self.data.build_g2(g2_param).filter(cfg.filter_method);

        Ok(cal_num)
    }

    pub fn set_area(&self, area: (u32, u32, u32, u32)) -> Result<()> {
        let mut cfg = self.cfg.write();
        let g2_param = cfg.set_area(area)?.get_g2_param()?;

        self.data.build_g2(g2_param).filter(cfg.filter_method);

        Ok(())
    }

    pub fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let mut cfg = self.cfg.write();
        if cfg.filter_method == filter_method {
            bail!("filter method same as before, no need to rebuild g2");
        }
        cfg.filter_method = filter_method;
        cfg.get_g2_param()?;

        self.data.filter(cfg.filter_method);

        Ok(())
    }

    pub async fn filter_single_point(
        &self,
        filter_method: FilterMethod,
        (y, x): (usize, usize),
    ) -> Result<Vec<u8>> {
        let (_, _, h, w) = self
            .cfg
            .read()
            .get_area()
            .ok_or_else(|| anyhow!("area unset"))?;

        let (h, w) = (h as usize, w as usize);
        if y >= h || x >= w {
            bail!("out of bounds")
        }
        let pos = y * w + x;
        let g1 = self.data.filter_single_point(filter_method, pos).await?;

        Ok(g1)
    }

    pub fn get_build_progress(&self) -> Option<CalProgress> {
        self.data.get_build_progress()
    }

    pub fn get_filter_progress(&self) -> Option<CalProgress> {
        self.data.get_filter_progress()
    }
}
