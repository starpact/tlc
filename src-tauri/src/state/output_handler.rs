use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use ndarray::Array2;
use tlc_video::{GmaxId, Green2Id, Packet, Parameters, VideoData, VideoId, VideoMeta};
use tracing::{debug, instrument, warn};

use super::GlobalState;
use crate::{
    daq::{DaqData, DaqId, DaqMeta, InterpId, Interpolator},
    post_processing::save_matrix,
    solve::{NuData, SolveId},
};

impl GlobalState {
    #[instrument(level = "debug", skip_all, err)]
    pub fn on_complete_read_video_meta(
        &mut self,
        video_id: VideoId,
        video_meta: VideoMeta,
        parameters: Parameters,
    ) -> Result<()> {
        debug!(?video_id, ?video_meta);
        if self.video_id()? != video_id {
            bail!("video id changed");
        }

        self.video_data = Some(VideoData::new(video_meta, parameters));
        self.reconcile();

        Ok(())
    }

    pub fn on_complete_load_video_packet(
        &mut self,
        video_id: Arc<VideoId>,
        packet: Packet,
    ) -> Result<()> {
        if self.video_id()? != *video_id {
            bail!("video path changed");
        }

        let video_data = self
            .video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))?;
        video_data.push_packet(Arc::new(packet))?;

        if video_data
            .packet(video_data.video_meta().nframes - 1)
            .is_ok()
        {
            self.reconcile();
        }

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn on_complete_read_daq(
        &mut self,
        daq_id: DaqId,
        daq_meta: DaqMeta,
        daq_raw: Array2<f64>,
    ) -> Result<()> {
        debug!(?daq_id, ?daq_meta);
        if self.daq_id()? != daq_id {
            bail!("daq path changed");
        }

        self.daq_data = Some(DaqData::new(daq_meta, daq_raw.into_shared()));
        self.reconcile();

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn on_complete_build_green2(
        &mut self,
        green2_id: Green2Id,
        green2: Array2<u8>,
    ) -> Result<()> {
        debug!(?green2_id);
        if self.green2_id()? != green2_id {
            bail!("green2 meta changed");
        }
        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(Some(green2.into_shared()));
        self.reconcile();

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn on_complete_detect_peak(
        &mut self,
        gmax_id: GmaxId,
        gmax_frame_indexes: Vec<usize>,
    ) -> Result<()> {
        debug!(?gmax_id);
        if self.gmax_id()? != gmax_id {
            bail!("gmax id changed");
        }

        self.video_data
            .as_mut()
            .unwrap() // already checked above
            .set_gmax_frame_indexes(Some(Arc::new(gmax_frame_indexes)));
        self.reconcile();

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn on_complete_interp(
        &mut self,
        interp_id: InterpId,
        interpolator: Interpolator,
    ) -> Result<()> {
        debug!(?interp_id);
        if self.interp_id()? != interp_id {
            bail!("interp id changed");
        }

        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(Some(interpolator));
        self.reconcile();

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn on_complete_solve(
        &mut self,
        solve_id: SolveId,
        nu2: Array2<f64>,
        nu_nan_mean: f64,
    ) -> Result<()> {
        debug!(?solve_id, nu_nan_mean);
        if self.solve_id()? != solve_id {
            bail!("solve id changed");
        }

        let nu2 = nu2.into_shared();
        self.nu_data = Some(NuData {
            nu2: nu2.clone(),
            nu_nan_mean,
        });

        let setting_snapshot = self.setting_snapshot(nu_nan_mean)?;

        let nu_path = self.nu_path()?;
        if nu_path.exists() {
            warn!("overwrite: {nu_path:?}")
        }
        let setting_snapshot_path = self.setting_snapshot_path()?;
        if setting_snapshot_path.exists() {
            warn!("overwrite: {setting_snapshot_path:?}")
        }

        std::thread::spawn(move || {
            let _ = save_matrix(nu_path, nu2.view());
            let _ = setting_snapshot.save(setting_snapshot_path);
        });

        Ok(())
    }
}
