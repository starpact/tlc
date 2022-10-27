use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Result};
use crossbeam::channel::{bounded, RecvTimeoutError, Sender};
use ndarray::{ArcArray2, Array2};
use tokio::sync::oneshot;
use tracing::{error, info_span, warn};
use video::{read_video, ProgressBar, VideoMeta};

use super::{GlobalState, Outcome};
use crate::{
    daq::{interp, read_daq, DaqMeta, InterpMeta, InterpMethod},
    request::Responder,
    setting::SettingStorage,
};

impl<S: SettingStorage> GlobalState<S> {
    pub fn on_get_save_root_dir(&self, responder: Responder<PathBuf>) {
        responder.respond(self.setting_storage.save_root_dir());
    }

    pub fn on_set_save_root_dir(&self, save_root_dir: PathBuf, responder: Responder<()>) {
        responder.respond(self.setting_storage.set_save_root_dir(&save_root_dir));
    }

    pub fn on_get_video_meta(&self, responder: Responder<VideoMeta>) {
        responder.respond(self.video_meta())
    }

    pub fn on_set_video_path(&mut self, video_path: PathBuf, responder: Responder<()>) {
        if let Err(e) = self.setting_storage.set_video_path(&video_path) {
            responder.respond_err(e);
            return;
        }

        let progress_bar = self.video_controller.prepare_read_video();
        self.spawn(|outcome_sender| {
            if let Err(e) = do_read_video(video_path, responder, outcome_sender, progress_bar) {
                error!(?e);
            }
        });
    }

    pub fn on_get_daq_meta(&self, responder: Responder<DaqMeta>) {
        responder.respond(self.daq_meta());
    }

    pub fn on_set_daq_path(&self, daq_path: PathBuf, responder: Responder<()>) {
        if let Err(e) = self.setting_storage.set_daq_path(&daq_path) {
            responder.respond_err(e);
            return;
        }

        self.spawn(|outcome_sender| {
            dispatch_outcome(do_read_daq(daq_path), outcome_sender, responder)
        });
    }

    pub fn on_get_daq_raw(&self, responder: Responder<ArcArray2<f64>>) {
        responder.respond(self.daq_raw())
    }

    pub fn on_set_interp_method(&self, interp_method: InterpMethod, responder: Responder<()>) {
        let (interp_meta, daq_raw) = match self.set_interp_method_and_prepare_interp(interp_method)
        {
            Ok(ret) => ret,
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };

        self.spawn(|outcome_sender| {
            dispatch_outcome(do_interp(interp_meta, daq_raw), outcome_sender, responder)
        });
    }

    pub fn on_interp_single_frame(&self, frame_index: usize, responder: Responder<Array2<f64>>) {
        match self.interpolator() {
            Ok(interpolator) => {
                std::thread::spawn(move || {
                    responder.respond(interpolator.interp_single_frame(frame_index))
                });
            }
            Err(e) => responder.respond_err(e),
        }
    }

    fn set_interp_method_and_prepare_interp(
        &self,
        interp_method: InterpMethod,
    ) -> Result<(InterpMeta, ArcArray2<f64>)> {
        let daq_raw = self.daq_data()?.daq_raw();

        let mut interp_meta = self.setting_storage.interp_meta()?;
        if interp_meta.interp_method == interp_method {
            warn!("interp method unchanged, compute again anyway");
        } else {
            interp_meta.interp_method = interp_method;
        }

        self.setting_storage.set_interp_method(interp_method)?;

        Ok((interp_meta, daq_raw))
    }
}

fn do_read_video(
    video_path: PathBuf,
    responder: Responder<()>,
    outcome_sender: Sender<Outcome>,
    progress_bar: ProgressBar,
) -> Result<()> {
    let (meta_tx, meta_rx) = oneshot::channel();
    let (packet_tx, packet_rx) = bounded(3); // cap doesn't matter

    std::thread::spawn(move || read_video(video_path, progress_bar, meta_tx, packet_tx));
    let (video_meta, parameters) = meta_rx.blocking_recv()?;
    let nframes = video_meta.nframes;
    outcome_sender
        .send(Outcome::ReadVideoMeta {
            video_meta,
            parameters,
        })
        .unwrap();
    responder.respond_ok(());

    let _span = info_span!("receive_loaded_packets", nframes).entered();
    for cnt in 1.. {
        // This is an ideal cancel point.
        match packet_rx.recv_timeout(Duration::from_secs(1)) {
            Ok((video_path, packet)) => outcome_sender
                .send(Outcome::LoadVideoPacket { video_path, packet })
                .unwrap(),
            Err(e) => match e {
                RecvTimeoutError::Timeout => bail!("load packets got stuck for some reason"),
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

fn do_read_daq(daq_path: PathBuf) -> Result<Outcome> {
    let (daq_meta, daq_raw) = read_daq(daq_path)?;
    Ok(Outcome::ReadDaq {
        daq_meta,
        daq_raw: daq_raw.into_shared(),
    })
}

fn do_interp(interp_meta: InterpMeta, daq_raw: ArcArray2<f64>) -> Result<Outcome> {
    let interpolator = interp(interp_meta, daq_raw)?;
    Ok(Outcome::Interp { interpolator })
}

fn dispatch_outcome(
    outcome: Result<Outcome>,
    outcome_sender: Sender<Outcome>,
    responder: Responder<()>,
) {
    match outcome {
        Ok(outcome) => {
            outcome_sender.send(outcome).unwrap();
            responder.respond_ok(());
        }
        Err(e) => responder.respond_err(e),
    }
}
