use anyhow::{anyhow, Result};
use tlc_video::{filter_detect_peak, GmaxMeta};
use tracing::{debug, instrument, warn};

use crate::{
    daq::interp,
    post_processing::save_matrix,
    solve::{self, nan_mean, SolveMeta},
};

use super::{outcome_handler::Outcome, GlobalState};

impl GlobalState {
    #[instrument(skip(self), err)]
    pub fn spwan_build_green2(&mut self) -> Result<()> {
        let video_data = self.video_data()?;
        let decoder_manager = video_data.decoder_manager();
        let packets = video_data.packets()?;
        let green2_meta = self.green2_meta()?;
        let progress_bar = self.video_controller.prepare_build_green2();

        self.spawn(move |outcome_sender| {
            if let Ok(green2) = decoder_manager.decode_all(packets, &green2_meta, progress_bar) {
                outcome_sender
                    .send(Outcome::BuildGreen2 {
                        green2_meta,
                        green2,
                    })
                    .unwrap();
            }
        });

        Ok(())
    }

    #[instrument(skip(self), err)]
    pub fn spawn_detect_peak(&mut self) -> Result<()> {
        let green2_meta = self.green2_meta()?;
        let green2 = self
            .video_data()?
            .green2()
            .ok_or_else(|| anyhow!("green2 not built yet"))?;
        let filter_method = self.setting.filter_method(&self.db)?;
        let progress_bar = self.video_controller.prepare_detect_peak();
        let gmax_meta = GmaxMeta {
            filter_method,
            green2_meta,
        };

        self.spawn(move |outcome_sender| {
            if let Ok(gmax_frame_indexes) = filter_detect_peak(green2, filter_method, progress_bar)
            {
                outcome_sender
                    .send(Outcome::DetectPeak {
                        gmax_meta,
                        gmax_frame_indexes,
                    })
                    .unwrap();
            }
        });

        Ok(())
    }

    #[instrument(skip(self), err)]
    pub fn spawn_interp(&self) -> Result<()> {
        let daq_raw = self.daq_data()?.daq_raw();
        let interp_meta = self.interp_meta()?;

        self.spawn(|outcome_sender| {
            if let Ok(interpolator) = interp(interp_meta, daq_raw) {
                outcome_sender
                    .send(Outcome::Interp { interpolator })
                    .unwrap();
            }
        });

        Ok(())
    }

    #[instrument(skip(self), err)]
    pub fn spawn_solve(&self) -> Result<()> {
        let video_data = self.video_data()?;
        let gmax_frame_indexes = video_data
            .gmax_frame_indexes()
            .ok_or_else(|| anyhow!("not detect peak yet"))?;
        let gmax_meta = self.gmax_meta()?;
        let frame_rate = video_data.video_meta().frame_rate;
        let interpolator = self
            .daq_data()?
            .interpolator()
            .cloned()
            .ok_or_else(|| anyhow!("not interp yet"))?;
        let physical_param = self.setting.physical_param(&self.db)?;
        let iteration_method = self.setting.iteration_method(&self.db)?;

        self.spawn(move |outcome_sender| {
            let nu2 = solve::solve(
                gmax_frame_indexes,
                &interpolator,
                physical_param,
                iteration_method,
                frame_rate,
            );

            let nu_nan_mean = nan_mean(nu2.view());
            debug!(nu_nan_mean);

            let solve_meta = SolveMeta {
                interp_meta: interpolator.meta().clone(),
                gmax_meta,
                iteration_method,
                physical_param,
            };

            outcome_sender
                .send(Outcome::Solve {
                    solve_meta,
                    nu2,
                    nu_nan_mean,
                })
                .unwrap();
        });

        Ok(())
    }

    #[instrument(skip(self), err)]
    pub fn spawn_save_nu(&self) -> Result<()> {
        let nu2 = self
            .nu_data
            .as_ref()
            .ok_or_else(|| anyhow!("not solved yet"))?
            .nu2
            .clone();
        let nu_path = self.nu_path()?;
        if nu_path.exists() {
            warn!("nu_path({nu_path:?}) already exists, overwrite")
        }

        let setting_snapshot = self.setting_snapshot()?;
        let setting_snapshot_path = self.setting_snapshot_path()?;

        std::thread::spawn(move || {
            let _ = save_matrix(nu_path, nu2.view());
            let _ = setting_snapshot.save(setting_snapshot_path);
        });

        Ok(())
    }
}
