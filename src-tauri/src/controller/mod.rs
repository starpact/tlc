mod cfg;
mod data;

use std::path::Path;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::debug;

pub use cfg::SaveInfo;
use cfg::TLCConfig;
pub use data::FilterMethod;
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
                let _ = cfg.on_video_load(video_meta);
            }
        }
        if let Some(daq_path) = cfg.get_daq_path() {
            if let Ok(daq_meta) = data.read_daq(daq_path).await {
                let _ = cfg.on_daq_load(daq_meta);
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
        let mut new_cfg = TLCConfig::from_path(path).await?;
        let mut new_data = TLCData::default();

        if let Some(video_path) = new_cfg.get_video_path() {
            let video_meta = new_data.read_video(video_path).await?;
            let _ = new_cfg.on_video_load(video_meta);
        }
        if let Some(daq_path) = new_cfg.get_daq_path() {
            let daq_meta = new_data.read_daq(daq_path).await?;
            let _ = new_cfg.on_daq_load(daq_meta);
        }

        debug!("{:#?}", new_cfg);

        *self.cfg.write().await = new_cfg;
        *self.data.write().await = new_data;

        Ok(())
    }

    pub async fn set_video_path<P: AsRef<Path>>(&self, video_path: P) -> Result<()> {
        // `set_video_path` has two side effects:
        // 1. Another thread is spawned to read from new video path.
        // 2. Some configurations are no longer valid so we need to update/invalidate them.
        let video_meta = self.data.read().await.read_video(&video_path).await?;

        let mut cfg = self.cfg.write().await;
        cfg.on_video_load(video_meta)?;
        let g2d_parameter = cfg.get_g2d_parameter()?;
        self.data
            .write()
            .await
            .build_g2d(g2d_parameter)
            .await?
            .filter(cfg.filter_method)
            .await?;

        Ok(())
    }

    pub async fn set_daq_path<P: AsRef<Path>>(&self, daq_path: P) -> Result<()> {
        let daq_meta = self.data.write().await.read_daq(&daq_path).await?;

        let mut cfg = self.cfg.write().await;
        cfg.on_daq_load(daq_meta)?;

        Ok(())
    }

    pub async fn get_frame(&self, frame_index: usize) -> Result<String> {
        self.data.read().await.get_frame(frame_index).await
    }

    pub async fn set_start_frame(&self, start_frame: usize) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        let g2d_builder = cfg.set_start_frame(start_frame)?;
        self.data
            .write()
            .await
            .build_g2d(g2d_builder)
            .await?
            .filter(cfg.filter_method)
            .await?;

        Ok(())
    }

    pub async fn set_area(&self, area: (u32, u32, u32, u32)) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        let g2d_builder = cfg.set_area(area)?;
        self.data
            .write()
            .await
            .build_g2d(g2d_builder)
            .await?
            .filter(cfg.filter_method)
            .await?;

        Ok(())
    }

    pub async fn set_filter_method(&self, filter_method: FilterMethod) -> Result<()> {
        let mut cfg = self.cfg.write().await;
        cfg.filter_method = filter_method;
        self.data.write().await.filter(filter_method).await?;

        Ok(())
    }
}
