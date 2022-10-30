use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use crossbeam::channel::{RecvTimeoutError, Sender};
use ndarray::{ArcArray2, Array2};
use tlc_video::{read_video, FilterMethod, Progress, ProgressBar, VideoMeta};
use tracing::{error, info_span};

use super::{GlobalState, Outcome};
use crate::{
    daq::{read_daq, DaqMeta, InterpMethod},
    request::Responder,
    setting::{SettingStorage, StartIndex},
};

impl<S: SettingStorage> GlobalState<S> {
    pub fn on_get_save_root_dir(&self, responder: Responder<PathBuf>) {
        responder.respond(self.setting_storage.save_root_dir());
    }

    pub fn on_set_save_root_dir(&mut self, save_root_dir: PathBuf, responder: Responder<()>) {
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

    pub fn on_get_read_video_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.read_video_progress());
    }

    pub fn on_decode_frame_base64(&self, frame_index: usize, responder: Responder<String>) {
        let f = || {
            let video_data = self.video_data()?;
            let packet = video_data.packet(frame_index)?;
            Ok((video_data, packet))
        };

        let (video_data, packet) = match f() {
            Ok(ret) => ret,
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

    pub fn on_set_daq_path(&mut self, daq_path: PathBuf, responder: Responder<()>) {
        if let Err(e) = self.setting_storage.set_daq_path(&daq_path) {
            responder.respond_err(e);
            return;
        }

        self.spawn(|outcome_sender| match do_read_daq(daq_path) {
            Ok(outcome) => {
                outcome_sender.send(outcome).unwrap();
                responder.respond_ok(());
            }
            Err(e) => responder.respond_err(e),
        });
    }

    pub fn on_get_daq_raw(&self, responder: Responder<ArcArray2<f64>>) {
        responder.respond(self.daq_raw())
    }

    pub fn on_synchronize_video_and_daq(
        &mut self,
        start_frame: usize,
        start_row: usize,
        responder: Responder<()>,
    ) {
        let ret = self.synchronize_video_and_daq(start_frame, start_row);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
    }

    pub fn on_get_start_index(&self, responder: Responder<StartIndex>) {
        responder.respond(self.setting_storage.start_index());
    }

    pub fn on_set_start_frame(&mut self, start_frame: usize, responder: Responder<()>) {
        let ret = self.set_start_frame(start_frame);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
    }

    pub fn on_set_start_row(&mut self, start_row: usize, responder: Responder<()>) {
        let ret = self.set_start_row(start_row);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
    }

    pub fn on_get_area(&self, responder: Responder<(u32, u32, u32, u32)>) {
        responder.respond(self.setting_storage.area());
    }

    pub fn on_set_area(&mut self, area: (u32, u32, u32, u32), responder: Responder<()>) {
        let ret = self.set_area(area);
        if ret.is_ok() {
            let _ = self.spwan_build_green2();
        }
        responder.respond(ret);
    }

    pub fn on_set_interp_method(&self, interp_method: InterpMethod, responder: Responder<()>) {
        let ret = self.set_interp_method(interp_method);
        if ret.is_ok() {
            let _ = self.spawn_interp();
        }
        responder.respond(ret);
    }

    pub fn on_get_build_green2_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.build_green2_progress());
    }

    pub fn on_get_filter_method(&self, responder: Responder<FilterMethod>) {
        responder.respond(self.setting_storage.filter_method());
    }

    pub fn on_set_filter_method(&mut self, filter_method: FilterMethod, responder: Responder<()>) {
        let ret = self.set_filter_method(filter_method);
        if ret.is_ok() {
            let _ = self.spawn_detect_peak();
        }
        responder.respond(ret);
    }

    pub fn on_get_detect_peak_progress(&self, responder: Responder<Progress>) {
        responder.respond_ok(self.video_controller.detect_peak_progress());
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
