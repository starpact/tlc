use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
use crossbeam::channel::{Receiver, RecvTimeoutError};
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
    pub fn handle_read_video_output1(
        &self,
        video_id: &VideoId,
        video_meta: VideoMeta,
        parameters: Parameters,
    ) -> Result<()> {
        debug!(?video_id, ?video_meta);
        let mut state = self.inner.lock();
        if &state.video_id()? != video_id {
            bail!("video id changed");
        }
        state.video_data = Some(VideoData::new(video_meta, parameters));

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn handle_read_video_output2(
        &self,
        video_id: &VideoId,
        packet_rx: Receiver<Packet>,
        nframes: usize,
    ) -> Result<()> {
        debug!(?video_id, nframes);
        for cnt in 0.. {
            match packet_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(packet) => {
                    let mut state = self.inner.lock();
                    if &state.video_id()? != video_id {
                        bail!("video path changed");
                    }
                    let video_data = state
                        .video_data
                        .as_mut()
                        .ok_or_else(|| anyhow!("video not loaded yet"))?;
                    video_data.push_packet(Arc::new(packet))?;

                    if video_data
                        .packet(video_data.video_meta().nframes - 1)
                        .is_ok()
                    {
                        self.reconcile(state);
                    }
                }
                Err(e) => match e {
                    RecvTimeoutError::Timeout => {
                        bail!("load packets got stuck for some reason");
                    }
                    RecvTimeoutError::Disconnected => {
                        debug_assert_eq!(cnt, nframes);
                        break;
                    }
                },
            }
            debug_assert!(cnt < nframes);
        }

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn handle_read_daq_output(
        &self,
        daq_id: &DaqId,
        daq_meta: DaqMeta,
        daq_raw: Array2<f64>,
    ) -> Result<()> {
        debug!(?daq_id, ?daq_meta);
        let mut state = self.inner.lock();
        if &state.daq_id()? != daq_id {
            bail!("daq path changed");
        }

        state.daq_data = Some(DaqData::new(daq_meta, daq_raw.into_shared()));
        self.reconcile(state);

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn handle_build_green2_output(
        &self,
        green2_id: &Green2Id,
        green2: Array2<u8>,
    ) -> Result<()> {
        debug!(?green2_id);
        let mut state = self.inner.lock();
        if &state.green2_id()? != green2_id {
            bail!("green2 meta changed");
        }
        state
            .video_data
            .as_mut()
            .unwrap() // already checked above
            .set_green2(Some(green2.into_shared()));
        self.reconcile(state);

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn handle_detect_peak_output(
        &self,
        gmax_id: &GmaxId,
        gmax_frame_indexes: Vec<usize>,
    ) -> Result<()> {
        debug!(?gmax_id);
        let mut state = self.inner.lock();
        if &state.gmax_id()? != gmax_id {
            bail!("gmax id changed");
        }

        state
            .video_data
            .as_mut()
            .unwrap() // already checked above
            .set_gmax_frame_indexes(Some(Arc::new(gmax_frame_indexes)));
        self.reconcile(state);

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn handle_interp_output(
        &self,
        interp_id: &InterpId,
        interpolator: Interpolator,
    ) -> Result<()> {
        debug!(?interp_id);
        let mut state = self.inner.lock();
        if &state.interp_id()? != interp_id {
            bail!("interp id changed");
        }

        state
            .daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .set_interpolator(Some(interpolator));
        self.reconcile(state);

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub fn handle_solve_output(
        &self,
        solve_id: &SolveId,
        nu2: Array2<f64>,
        nu_nan_mean: f64,
    ) -> Result<()> {
        debug!(?solve_id, nu_nan_mean);
        let mut state = self.inner.lock();
        if &state.solve_id()? != solve_id {
            bail!("solve id changed");
        }

        let nu2 = nu2.into_shared();
        state.nu_data = Some(NuData {
            nu2: nu2.clone(),
            nu_nan_mean,
        });

        let setting_snapshot = state.setting_snapshot(nu_nan_mean)?;

        let nu_path = state.nu_path()?;
        if nu_path.exists() {
            warn!("overwrite: {nu_path:?}")
        }
        let setting_snapshot_path = state.setting_snapshot_path()?;
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
