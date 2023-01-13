use crate::util::progress_bar::{Progress, ProgressBar};

#[derive(Default)]
pub struct VideoController {
    read_video: ProgressBar,
    build_green2: ProgressBar,
    detect_peak: ProgressBar,
}

impl VideoController {
    pub fn read_video_progress(&self) -> Progress {
        self.read_video.get()
    }

    pub fn build_green2_progress(&self) -> Progress {
        self.build_green2.get()
    }

    pub fn detect_peak_progress(&self) -> Progress {
        self.detect_peak.get()
    }

    pub fn prepare_read_video(&mut self) -> ProgressBar {
        std::mem::take(&mut self.read_video).cancel();
        std::mem::take(&mut self.build_green2).cancel();
        std::mem::take(&mut self.detect_peak).cancel();
        self.read_video.clone()
    }

    pub fn prepare_build_green2(&mut self) -> ProgressBar {
        std::mem::take(&mut self.build_green2).cancel();
        std::mem::take(&mut self.detect_peak).cancel();
        self.build_green2.clone()
    }

    pub fn prepare_detect_peak(&mut self) -> ProgressBar {
        std::mem::take(&mut self.detect_peak).cancel();
        self.detect_peak.clone()
    }
}
