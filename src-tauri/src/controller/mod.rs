mod cfg;
mod data;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use ndarray::ArcArray2;
use tokio::sync::RwLock;
use tracing::debug;

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
            data.build_g2(g2_param).await.filter(cfg.filter_method);
        }

        debug!("{:#?}", cfg);

        Self {
            cfg: RwLock::new(cfg),
            data,
        }
    }

    pub async fn load_config<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut cfg = TLCConfig::from_path(path).await?;
        self.data.reset();

        if let Some(video_path) = cfg.get_video_path() {
            let video_meta = self.data.read_video(video_path).await?;
            cfg.on_video_load(video_meta);
        }
        if let Some(daq_path) = cfg.get_daq_path() {
            let daq_meta = self.data.read_daq(daq_path).await?;
            cfg.on_daq_load(daq_meta);
        }

        debug!("{:#?}", cfg);

        *self.cfg.write().await = cfg;

        Ok(())
    }

    pub async fn get_save_root_dir(&self) -> Result<PathBuf> {
        Ok(self.cfg.read().await.get_save_root_dir()?.to_owned())
    }

    pub async fn set_save_root_dir<P: AsRef<Path>>(&self, save_root_dir: P) {
        self.cfg.write().await.save_root_dir = Some(save_root_dir.as_ref().to_owned());
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        self.data.get_frame(frame_index).await
    }

    pub async fn get_video_meta(&self) -> Result<VideoMeta> {
        self.cfg
            .read()
            .await
            .get_video_meta()
            .ok_or_else(|| anyhow!("video path unset"))
    }

    pub async fn get_daq_meta(&self) -> Result<DAQMeta> {
        self.cfg
            .read()
            .await
            .get_daq_meta()
            .ok_or_else(|| anyhow!("daq path unset"))
    }

    // `set_video_path` has two side effects:
    // 1. Another thread is spawned to read from new video path.
    // 2. Some configurations are no longer valid so we need to update/invalidate them.
    pub async fn set_video_path<P: AsRef<Path>>(&self, video_path: P) -> Result<VideoMeta> {
        let video_meta = self.data.read_video(&video_path).await?;
        self.cfg.write().await.on_video_load(video_meta.clone());

        Ok(video_meta)
    }

    pub async fn get_daq(&self) -> ArcArray2<f64> {
        self.data.get_daq()
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&self, daq_path: P) -> Result<DAQMeta> {
        let daq_meta = self.data.read_daq(&daq_path).await?;
        self.cfg.write().await.on_daq_load(daq_meta.clone());

        Ok(daq_meta)
    }

    pub async fn set_start_frame(&self, start_frame: usize) -> Result<usize> {
        let mut cfg = self.cfg.write().await;
        let g2_param = cfg.set_start_frame(start_frame)?.get_g2_param()?;

        let cal_num = g2_param.frame_num;
        self.data.build_g2(g2_param).await.filter(cfg.filter_method);

        Ok(cal_num)
    }

    pub async fn set_start_row(&self, start_row: usize) -> Result<usize> {
        let mut cfg = self.cfg.write().await;
        let g2_param = cfg.set_start_row(start_row)?.get_g2_param()?;

        let cal_num = g2_param.frame_num;
        self.data.build_g2(g2_param).await.filter(cfg.filter_method);

        Ok(cal_num)
    }

    pub async fn set_area(&self, area: (u32, u32, u32, u32)) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        let g2_param = cfg.set_area(area)?.get_g2_param()?;

        self.data.build_g2(g2_param).await.filter(cfg.filter_method);

        Ok(())
    }

    pub async fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        if cfg.filter_method == filter_method {
            bail!("filter method same as before, no need to rebuild g2");
        }
        cfg.get_g2_param()?;
        cfg.filter_method = filter_method;

        self.data.filter(cfg.filter_method);

        Ok(())
    }
}
