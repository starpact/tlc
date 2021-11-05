use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;
use tracing::debug;

#[derive(Debug, Default)]
pub struct FrameCache {
    /// Identifies the current video.
    pub path: Option<PathBuf>,
    /// Total frame number of the current video.
    /// This is used to validate the `frame_index` of `get_frame`.
    pub total_frames: usize,
    /// Frame data.
    pub frames: Vec<usize>,
}

impl FrameCache {
    pub fn reset<P: AsRef<Path>>(&mut self, path: P, total_frames: usize) {
        self.path = Some(path.as_ref().to_owned());
        self.total_frames = total_frames;
        // Actual frame data will be dropped in place.
        // Allocated capacity of `frames` won't change.
        self.frames.clear();
    }

    pub fn path_changed<P: AsRef<Path>>(&self, path: P) -> bool {
        let old = match self.path {
            Some(ref path) => path,
            None => return true,
        };
        let new = path.as_ref();

        old != new
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct VideoInfo {
    pub frame_rate: usize,
    pub total_frames: usize,
    pub shape: (usize, usize),
}

pub fn get_video_info<P: AsRef<Path>>(_path: P) -> Result<VideoInfo> {
    let video_info = VideoInfo {
        frame_rate: 25,
        total_frames: 1000,
        shape: (500, 500),
    };

    debug!("{:#?}", video_info);

    Ok(video_info)
}
