mod cfg;
mod data;

use std::path::Path;

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::debug;

pub use cfg::SaveInfo;
use cfg::TLCConfig;
use data::TLCData;

pub struct TLCHandler {
    /// `cfg` can be mapped to a calculation result set and will be saved to disk.
    cfg: RwLock<TLCConfig>,
    /// `data` stores all runtime data and the calculation result set.
    data: RwLock<TLCData>,
}

impl TLCHandler {
    pub async fn new() -> Self {
        let mut cfg = TLCConfig::from_default_path().await;
        let mut data = TLCData::default();

        if let Some(video_path) = cfg.get_video_path() {
            if let Ok(video_info) = data.read_video(video_path).await {
                cfg.on_video_change(video_info);
            }
        }
        if let Some(daq_path) = cfg.get_daq_path() {
            if let Ok(total_frames) = data.read_daq(daq_path).await {
                cfg.on_daq_change(total_frames);
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
        *self.cfg.write().await = TLCConfig::from_path(path).await?;

        // If the config is reloaded, all data are invalidated.
        *self.data.write().await = TLCData::default();

        Ok(())
    }

    pub async fn set_video_path<P: AsRef<Path>>(&self, video_path: P) -> Result<()> {
        // `set_video_path` has two side effects:
        // 1. Another thread is spawned to read from new video path.
        // 2. Some configurations are no longer valid so we need to update/invalidate them.
        let video_info = self
            .data
            .read()
            .await
            .read_video(&video_path)
            .await
            .with_context(|| format!("failed to read video: {:?}", video_path.as_ref()))?;

        let mut cfg = self.cfg.write().await;
        cfg.set_video_path(&video_path)?.on_video_change(video_info);

        Ok(())
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&self, daq_path: P) -> Result<()> {
        let total_rows = self.data.write().await.read_daq(&daq_path).await?;

        let mut cfg = self.cfg.write().await;
        cfg.set_daq_path(&daq_path)?.on_daq_change(total_rows);

        Ok(())
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        self.data.read().await.get_frame(frame_index).await
    }

    pub async fn set_region(&self, region: [u32; 4]) -> Result<()> {
        let g2d_builder = self.cfg.write().await.set_region(region)?;
        self.data.write().await.build_g2d(g2d_builder).await?;

        Ok(())
    }

    pub async fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        let g2d_builder = self.cfg.write().await.set_start_frame(start_frame)?;
        self.data.write().await.build_g2d(g2d_builder).await?;

        Ok(())
    }
}
