use std::{path::Path, sync::Arc};

use crate::{
    config::TlcConfig,
    video::{self, VideoCache, VideoMetadata},
};

use anyhow::Result;
use parking_lot::RwLock;

pub struct TlcState {
    config: TlcConfig,
    video_cache: Arc<RwLock<VideoCache>>,
}

impl TlcState {
    pub fn new() -> Self {
        TlcState {
            config: TlcConfig::from_default_path(),
            video_cache: Default::default(),
        }
    }

    pub async fn set_video_path<P: AsRef<Path>>(&mut self, video_path: P) -> Result<VideoMetadata> {
        if let Some(video_metadata) = self.config.video_metadata() {
            if video_metadata.path == video_path.as_ref() {
                return Ok(video_metadata.clone());
            }
        }

        let video_metadata = video::load_packets(self.video_cache.clone(), video_path).await?;
        self.config.set_video_metadata(video_metadata.clone());

        Ok(video_metadata)
    }

    pub async fn read_single_frame(&self, frame_index: usize) -> Result<String> {
        video::read_single_frame(self.video_cache.clone(), frame_index).await
    }
}
