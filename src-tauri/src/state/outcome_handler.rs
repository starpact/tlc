use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use ndarray::ArcArray2;
use tlc_video::{GmaxMeta, Green2Meta, Packet, Parameters, VideoData, VideoMeta};

use super::GlobalState;
use crate::{
    daq::{DaqData, DaqMeta, Interpolator},
    setting::SettingStorage,
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
        video_meta: Arc<VideoMeta>,
        packet: Arc<Packet>,
    ) -> Result<()> {
        if self.setting_storage.video_path()? != *video_meta.path {
            bail!("video path changed");
        }

        self.video_data_mut()?.push_packet(&video_meta, packet)
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

    pub fn on_complete_build_green2(
        &mut self,
        green2_meta: Green2Meta,
        green2: ArcArray2<u8>,
    ) -> Result<()> {
        if self.setting_storage.video_path()? != green2_meta.video_meta.path {
            bail!("video path changed");
        }
        if self.green2_meta()? != green2_meta {
            bail!("green2 meta changed");
        }
        self.video_data_mut()?.set_green2(Some(green2));

        let _ = self.spawn_interp();

        Ok(())
    }

    pub fn on_complete_detect_peak(
        &mut self,
        gmax_meta: GmaxMeta,
        gmax_frame_indexes: Arc<Vec<usize>>,
    ) -> Result<()> {
        if self.setting_storage.video_path()? != gmax_meta.green2_meta.video_meta.path {
            bail!("video path changed");
        }
        if self.green2_meta()? != gmax_meta.green2_meta {
            bail!("green2 meta changed");
        }
        if self.setting_storage.filter_method()? != gmax_meta.filter_method {
            bail!("filter method changed");
        }

        self.video_data_mut()?
            .set_gmax_frame_indexes(Some(gmax_frame_indexes));

        Ok(())
    }

    pub fn on_complete_interp(&mut self, interpolator: Interpolator) -> Result<()> {
        if &self.interp_meta()? != interpolator.meta() {
            bail!("interp meta changed, abort this result");
        }
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(interpolator)
    }
}
