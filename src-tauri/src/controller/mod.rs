mod cfg;
mod data;

use std::path::Path;

use anyhow::{bail, Context, Result};
use tokio::sync::RwLock;
use tracing::debug;

pub use cfg::SaveInfo;
use cfg::TLCConfig;
use data::TLCData;

pub struct TLCController {
    /// `cfg` can be mapped to a calculation result set and will be saved to disk.
    cfg: RwLock<TLCConfig>,
    /// `data` stores all runtime data and the calculation result set.
    data: RwLock<TLCData>,
}

impl TLCController {
    pub async fn new() -> Self {
        let mut cfg = TLCConfig::from_default_path().await;
        let mut data = TLCData::default();

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

        debug!("{:#?}", cfg);

        Self {
            cfg: RwLock::new(cfg),
            data: RwLock::new(data),
        }
    }

    pub async fn get_save_info(&self) -> Result<SaveInfo> {
        self.cfg.read().await.get_save_info()
    }

    pub async fn load_config<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let new_cfg = TLCConfig::from_path(path).await?;
        let mut new_data = TLCData::default();

        if let Some(video_path) = new_cfg.get_video_path() {
            new_data.read_video(video_path).await?;
        }
        if let Some(daq_path) = new_cfg.get_daq_path() {
            new_data.read_daq(daq_path).await?;
        }

        debug!("{:#?}", new_cfg);

        *self.cfg.write().await = new_cfg;
        *self.data.write().await = new_data;

        Ok(())
    }

    pub async fn set_video_path<P: AsRef<Path>>(&self, video_path: P) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        if cfg.get_video_path() == Some(video_path.as_ref()) {
            bail!("video path same as before");
        }

        // `set_video_path` has two side effects:
        // 1. Another thread is spawned to read from new video path.
        // 2. Some configurations are no longer valid so we need to update/invalidate them.
        let video_meta = self
            .data
            .read()
            .await
            .read_video(&video_path)
            .await
            .with_context(|| format!("failed to read video: {:?}", video_path.as_ref()))?;

        cfg.set_video_path(&video_path, video_meta);

        Ok(())
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&self, daq_path: P) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        if cfg.get_daq_path() == Some(daq_path.as_ref()) {
            bail!("daq path same as before");
        }

        let daq_meta = self.data.write().await.read_daq(&daq_path).await?;

        cfg.set_daq_path(&daq_path, daq_meta);

        Ok(())
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        self.data.read().await.get_frame(frame_index).await
    }

    pub async fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        let g2d_builder = self.cfg.write().await.set_start_frame(start_frame)?;
        self.data.write().await.build_g2d(g2d_builder).await?;

        Ok(())
    }

    pub async fn set_region(&self, region: [u32; 4]) -> Result<()> {
        let g2d_builder = self.cfg.write().await.set_region(region)?;
        self.data.write().await.build_g2d(g2d_builder).await?;

        Ok(())
    }
}
