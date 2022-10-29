use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use crossbeam::channel::{RecvTimeoutError, Sender};
use ndarray::{ArcArray2, Array2};
use tlc_video::{read_video, DecoderManager, Packet, ProgressBar, VideoMeta};
use tracing::{error, info_span, warn};

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
            do_read_video(video_path, responder, outcome_sender, progress_bar);
        });
    }

    pub fn on_decode_frame_base64(&self, frame_index: usize, responder: Responder<String>) {
        let video_data = match self.video_data() {
            Ok(video_data) => video_data,
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };
        let packet = match video_data.packet(frame_index) {
            Ok(packet) => packet,
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };
        let decoder_manager = video_data.decoder_manager();

        std::thread::spawn(move || responder.respond(decoder_manager.decode_frame_base64(packet)));
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
) {
    let (video_meta, parameters, packet_rx) = match read_video(video_path, progress_bar) {
        Ok(ret) => ret,
        Err(e) => {
            responder.respond_err(e);
            return;
        }
    };

    let nframes = video_meta.nframes;
    outcome_sender
        .send(Outcome::ReadVideoMeta {
            video_meta: video_meta.clone(),
            parameters,
        })
        .unwrap();
    responder.respond_ok(());

    let _span = info_span!("receive_loaded_packets", nframes).entered();
    let video_meta = Arc::new(video_meta);
    for cnt in 1.. {
        match packet_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(packet) => outcome_sender
                .send(Outcome::LoadVideoPacket {
                    video_meta: video_meta.clone(),
                    packet: Arc::new(packet),
                })
                .unwrap(),
            Err(e) => {
                match e {
                    RecvTimeoutError::Timeout => error!("load packets got stuck for some reason"),
                    RecvTimeoutError::Disconnected => debug_assert_eq!(cnt, nframes),
                }
                return;
            }
        }
        debug_assert!(cnt < nframes);
    }
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
