use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
use ffmpeg::codec::{packet::Packet, Parameters};
use ndarray::ArcArray2;

use super::GlobalState;
use crate::{
    daq::{DaqData, DaqMeta, Interpolator},
    setting::SettingStorage,
    video::{VideoData, VideoMeta},
};

impl<S: SettingStorage> GlobalState<S> {
    pub fn on_complete_read_video_meta(
        &mut self,
        video_meta: VideoMeta,
        parameters: Parameters,
    ) -> Result<()> {
        if self.setting_storage.video_path()? != video_meta.path {
            bail!("video path changed");
        }

        let video_data = VideoData::new(video_meta, parameters);
        self.video_data = Some(video_data);

        Ok(())
    }

    pub fn on_complete_load_video_packet(
        &mut self,
        video_path: Arc<PathBuf>,
        packet: Packet,
    ) -> Result<()> {
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .push_packet(&video_path, packet)
    }

    pub fn on_complete_read_daq(
        &mut self,
        daq_meta: DaqMeta,
        daq_raw: ArcArray2<f64>,
    ) -> Result<()> {
        if self.setting_storage.daq_path()? != daq_meta.path {
            bail!("daq path changed");
        }

        self.daq_data = Some(DaqData::new(daq_meta, daq_raw));

        Ok(())
    }

    pub fn on_complete_interp(&mut self, interpolator: Interpolator) -> Result<()> {
        if &self.setting_storage.interp_meta()? != interpolator.meta() {
            bail!("interp meta changed, abort this result");
        }
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(interpolator)
    }

    pub fn reconcile(&mut self) {
        todo!()
    }
}
