use std::{path::Path, sync::Arc};

use crate::{
    config::TlcConfig,
    video::{read_video, VideoCache, VideoMetadata},
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
        let video_metadata = read_video(self.video_cache.clone(), video_path).await?;
        self.config.set_video_metadata(video_metadata.clone());

        Ok(video_metadata)
    }

    pub async fn read_frame(&self, frame_index: usize) -> Result<String> {
        Ok("".to_owned())
    }
}
