use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct FrameCache {
    /// frame data.
    pub frames: Vec<usize>,
    /// Identifies the current video.
    pub path: Option<PathBuf>,
}

impl FrameCache {
    pub fn reset<P: AsRef<Path>>(&mut self, path: P) {
        // Actual frame data will be dropped(in place).
        // Allocated capacity of `frames` won't change.
        self.frames.clear();
        self.path = Some(path.as_ref().to_owned());
    }

    pub fn path_changed<P: AsRef<Path>>(&self, token: P) -> bool {
        let old = match self.path {
            Some(ref path) => path,
            None => return true,
        };
        let new = token.as_ref();

        old != new
    }
}
