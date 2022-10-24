use std::path::PathBuf;

use anyhow::Result;
use ndarray::{ArcArray2, Array2};
use tracing::warn;

use super::{GlobalState, Outcome};
use crate::{
    daq::{self, DaqMeta, InterpMeta, InterpMethod},
    request::Responder,
    setting::SettingStorage,
    video::VideoMeta,
};

impl<S: SettingStorage> GlobalState<S> {
    pub fn get_video_meta(&self) -> Result<VideoMeta> {
        todo!()
    }

    pub fn set_video_path(&self, _video_path: PathBuf, _responder: Responder<()>) {
        todo!()
    }

    pub fn get_daq_meta(&self) -> Result<DaqMeta> {
        let daq_path = self.setting_storage.daq_path()?;
        let daq_data = self.daq_data()?;
        if daq_data.daq_meta().path != daq_path {
            warn!("new daq not loaded yet, return old data anyway");
        }

        Ok(daq_data.daq_meta().clone())
    }

    pub fn set_daq_path(&self, daq_path: PathBuf, responder: Responder<()>) {
        if let Err(e) = self.setting_storage.set_daq_path(&daq_path) {
            responder.respond_err(e);
            return;
        }

        let event_sender = self.outcome_sender.clone();
        std::thread::spawn(move || match make_event_read_daq(daq_path) {
            Ok(event) => {
                event_sender.send(event).unwrap();
                responder.respond_ok(());
            }
            Err(e) => responder.respond_err(e),
        });
    }

    pub fn get_daq_raw(&self) -> Result<ArcArray2<f64>> {
        self.setting_storage.daq_path()?;
        Ok(self.daq_data()?.daq_raw())
    }

    pub fn set_interp_method(&self, interp_method: InterpMethod, responder: Responder<()>) {
        let daq_raw = match self.daq_data() {
            Ok(daq_data) => daq_data.daq_raw(),
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };

        let interp_meta = match self.setting_storage.interp_meta() {
            Ok(mut interp_meta) => {
                if interp_meta.interp_method == interp_method {
                    responder.respond_ok(());
                    return;
                }
                interp_meta.interp_method = interp_method;
                interp_meta
            }
            Err(e) => {
                responder.respond_err(e);
                return;
            }
        };

        if let Err(e) = self.setting_storage.set_interp_method(interp_method) {
            responder.respond_err(e);
            return;
        }

        let event_sender = self.outcome_sender.clone();
        std::thread::spawn(move || match make_event_interp(interp_meta, daq_raw) {
            Ok(event) => {
                event_sender.send(event).unwrap();
                responder.respond_ok(());
            }
            Err(e) => responder.respond_err(e),
        });
    }

    pub fn interp_single_frame(&self, frame_index: usize, responder: Responder<Array2<f64>>) {
        match self.interpolator() {
            Ok(interpolator) => {
                std::thread::spawn(
                    move || match interpolator.interp_single_frame(frame_index) {
                        Ok(temp2) => responder.respond_ok(temp2),
                        Err(e) => responder.respond_err(e),
                    },
                );
            }
            Err(e) => responder.respond_err(e),
        }
    }
}

fn make_event_read_daq(daq_path: PathBuf) -> Result<Outcome> {
    let (daq_meta, daq_raw) = daq::read_daq(daq_path)?;
    Ok(Outcome::ReadDaq {
        daq_meta,
        daq_raw: daq_raw.into_shared(),
    })
}

fn make_event_interp(interp_meta: InterpMeta, daq_raw: ArcArray2<f64>) -> Result<Outcome> {
    let interpolator = daq::interp(interp_meta, daq_raw)?;
    Ok(Outcome::Interp { interpolator })
}
