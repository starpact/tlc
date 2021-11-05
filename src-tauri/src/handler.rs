use std::path::Path;

use anyhow::Result;
use tokio::sync::RwLock;

use crate::config::{SaveInfo, TLCConfig};
use crate::data::TLCData;

pub struct TLCHandler {
    cfg: RwLock<TLCConfig>,
    data: RwLock<TLCData>,
}

impl TLCHandler {
    pub async fn new() -> Self {
        let cfg = TLCConfig::from_default_path().await;
        let data = TLCData::default();

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
        cfg.set_video_path(path.as_ref())?;

        // `set_video_path` has two side effects:
        // 1. Another thread is spawned to read from new video path.
        let video_info = self.data.read().await.read_video(path.as_ref()).await?;
        // 2. Some configurations are no longer valid so we need to update/invalidate them.
        cfg.update_with_video_info(video_info);

        Ok(())
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<usize> {
        self.data.read().await.get_frame(frame_index).await
    }
}
