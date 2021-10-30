use super::TLCHandler;
use anyhow::Result;
use std::path::PathBuf;

impl TLCHandler {
    pub fn set_video_path(&mut self, video_path: PathBuf) -> Result<()> {
        self.cfg.storage.video_path = Some(video_path);
        self.data.packets.clear();

        Ok(())
    }
}
