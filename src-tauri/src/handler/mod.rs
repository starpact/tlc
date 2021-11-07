mod cfg;
mod data;

use std::path::Path;

use anyhow::Result;
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
        let data = TLCData::default();

        if let Some(video_path) = cfg.get_video_path() {
            if let Ok(video_timing_info) = data.read_video(video_path).await {
                cfg.on_video_change(video_timing_info);
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

    pub async fn set_video_path<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        cfg.set_video_path(&path)?;

        // `set_video_path` has two side effects:
        // 1. Another thread is spawned to read from new video path.
        let video_info = self.data.read().await.read_video(&path).await?;
        // 2. Some configurations are no longer valid so we need to update/invalidate them.
        cfg.on_video_change(video_info);

        Ok(())
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        self.data.read().await.get_frame(frame_index).await
    }
}
