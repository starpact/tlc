use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use ndarray::Array2;
use tlc_video::{GmaxMeta, Green2Meta, Packet, Parameters, VideoData, VideoMeta};

use super::GlobalState;
use crate::{
    daq::{DaqData, DaqMeta, Interpolator},
    solve::{NuData, SolveMeta},
};

pub enum Outcome {
    ReadVideoMeta {
        video_meta: VideoMeta,
        parameters: Parameters,
    },
    LoadVideoPacket {
        video_meta: Arc<VideoMeta>,
        packet: Packet,
    },
    ReadDaq {
        daq_meta: DaqMeta,
        daq_raw: Array2<f64>,
    },
    BuildGreen2 {
        green2_meta: Green2Meta,
        green2: Array2<u8>,
    },
    DetectPeak {
        gmax_meta: GmaxMeta,
        gmax_frame_indexes: Vec<usize>,
    },
    Interp {
        interpolator: Interpolator,
    },
    Solve {
        solve_meta: SolveMeta,
        nu2: Array2<f64>,
        nu_nan_mean: f64,
    },
}

impl GlobalState {
    pub fn handle_outcome(&mut self, outcome: Outcome) -> Result<()> {
        use Outcome::*;
        match outcome {
            ReadVideoMeta {
                video_meta,
                parameters,
            } => self.on_complete_read_video_meta(video_meta, parameters)?,
            LoadVideoPacket { video_meta, packet } => {
                self.on_complete_load_video_packet(video_meta, packet)?;
            }
            ReadDaq { daq_meta, daq_raw } => self.on_complete_read_daq(daq_meta, daq_raw)?,
            BuildGreen2 {
                green2_meta,
                green2,
            } => self.on_complete_build_green2(green2_meta, green2)?,
            Interp { interpolator } => self.on_complete_interp(interpolator)?,
            DetectPeak {
                gmax_meta,
                gmax_frame_indexes,
            } => self.on_complete_detect_peak(gmax_meta, gmax_frame_indexes)?,
            Solve {
                solve_meta,
                nu2,
                nu_nan_mean,
            } => self.on_solve(solve_meta, nu2, nu_nan_mean)?,
        }

        Ok(())
    }

    pub fn on_complete_read_video_meta(
        &mut self,
        video_meta: VideoMeta,
        parameters: Parameters,
    ) -> Result<()> {
        if self.setting.video_path(&self.db)? != video_meta.path {
            bail!("video path changed");
        }

        let video_data = VideoData::new(video_meta, parameters);
        self.video_data = Some(video_data);

        Ok(())
    }

    pub fn on_complete_load_video_packet(
        &mut self,
        video_meta: Arc<VideoMeta>,
        packet: Packet,
    ) -> Result<()> {
        if self.setting.video_path(&self.db)? != *video_meta.path {
            bail!("video path changed");
        }

        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .push_packet(&video_meta, Arc::new(packet))
    }

    pub fn on_complete_read_daq(&mut self, daq_meta: DaqMeta, daq_raw: Array2<f64>) -> Result<()> {
        if self.setting.daq_path(&self.db)? != daq_meta.path {
            bail!("daq path changed");
        }

        self.daq_data = Some(DaqData::new(daq_meta, daq_raw.into_shared()));

        Ok(())
    }

    pub fn on_complete_build_green2(
        &mut self,
        green2_meta: Green2Meta,
        green2: Array2<u8>,
    ) -> Result<()> {
        if self.green2_meta()? != green2_meta {
            bail!("green2 meta changed");
        }
        if self.setting.video_path(&self.db)? != green2_meta.video_meta.path {
            bail!("video path changed");
        }
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_green2(Some(green2.into_shared()));

        let _ = self.spawn_interp();

        Ok(())
    }

    pub fn on_complete_detect_peak(
        &mut self,
        gmax_meta: GmaxMeta,
        gmax_frame_indexes: Vec<usize>,
    ) -> Result<()> {
        if self.green2_meta()? != gmax_meta.green2_meta {
            bail!("green2 meta changed");
        }
        if self.setting.video_path(&self.db)? != gmax_meta.green2_meta.video_meta.path {
            bail!("video path changed");
        }
        if self.setting.filter_method(&self.db)? != gmax_meta.filter_method {
            bail!("filter method changed");
        }

        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?
            .set_gmax_frame_indexes(Some(Arc::new(gmax_frame_indexes)));

        Ok(())
    }

    pub fn on_complete_interp(&mut self, interpolator: Interpolator) -> Result<()> {
        if self.daq_data()?.daq_meta().path != interpolator.meta().daq_meta.path {
            bail!("daq path changed");
        }
        if &self.interp_meta()? != interpolator.meta() {
            bail!("interp meta changed");
        }

        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(Some(interpolator));

        Ok(())
    }

    pub fn on_solve(
        &mut self,
        solve_meta: SolveMeta,
        nu2: Array2<f64>,
        nu_nan_mean: f64,
    ) -> Result<()> {
        if self.gmax_meta()? != solve_meta.gmax_meta {
            bail!("gmax meta changed");
        }
        if self.setting.filter_method(&self.db)? != solve_meta.gmax_meta.filter_method {
            bail!("filter method changed");
        }
        self.nu_data = Some(NuData {
            nu2: nu2.into_shared(),
            nu_nan_mean,
        });

        let _ = self.spawn_save_nu();

        Ok(())
    }
}
