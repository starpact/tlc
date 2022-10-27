mod outcome_handler;
mod request_handler;

use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
use crossbeam::{
    channel::{bounded, Receiver, Sender},
    select,
};
use ndarray::ArcArray2;
use tracing::{error, warn};
use video::{Packet, Parameters, VideoController, VideoData, VideoMeta};

use crate::{
    daq::{DaqData, DaqMeta, Interpolator},
    request::Request,
    setting::{SettingStorage, SqliteSettingStorage},
};

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

struct GlobalState<S: SettingStorage> {
    setting_storage: S,

    outcome_sender: Sender<Outcome>,
    outcome_receiver: Receiver<Outcome>,

    video_data: Option<VideoData>,
    video_controller: VideoController,

    daq_data: Option<DaqData>,
}

enum Outcome {
    ReadVideoMeta {
        video_meta: VideoMeta,
        parameters: Parameters,
    },
    LoadVideoPacket {
        video_path: Arc<PathBuf>,
        packet: Packet,
    },
    ReadDaq {
        daq_meta: DaqMeta,
        daq_raw: ArcArray2<f64>,
    },
    Interp {
        interpolator: Interpolator,
    },
}

pub fn main_loop(request_receiver: Receiver<Request>) {
    let setting_storage = SqliteSettingStorage::new(SQLITE_FILEPATH);
    let mut global_state = GlobalState::new(setting_storage);
    loop {
        if let Err(e) = global_state.handle(&request_receiver) {
            error!("{e}");
        }
    }
}

impl<S: SettingStorage> GlobalState<S> {
    fn new(setting_storage: S) -> Self {
        let (outcome_sender, outcome_receiver) = bounded(3);
        Self {
            setting_storage,
            outcome_sender,
            outcome_receiver,
            video_data: None,
            daq_data: None,
            video_controller: VideoController::default(),
        }
    }

    /// `handle` keeps receiving `Request`(frontend message) and `Outcome`(computation
    /// result), then make decision what to do next based on the current global state.
    /// It should NEVER block or do any heavy computations, all blocking/time-consuming
    /// tasks should be executed in other threads and send back results asynchronously
    /// through `outcome_sender`.
    fn handle(&mut self, request_receiver: &Receiver<Request>) -> Result<()> {
        select! {
            recv(request_receiver)  -> request => self.handle_request(request?),
            recv(self.outcome_receiver) -> outcome => self.handle_outcome(outcome?)?,
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) {
        use Request::*;
        match request {
            GetSaveRootDir { responder } => self.on_get_save_root_dir(responder),
            SetSaveRootDir {
                save_root_dir,
                responder,
            } => self.on_set_save_root_dir(save_root_dir, responder),
            GetVideoMeta { responder } => self.on_get_video_meta(responder),
            SetVideoPath {
                video_path,
                responder,
            } => self.on_set_video_path(video_path, responder),
            GetDaqMeta { responder } => self.on_get_daq_meta(responder),
            SetDaqPath {
                daq_path,
                responder,
            } => self.on_set_daq_path(daq_path, responder),
            GetDaqRaw { responder } => self.on_get_daq_raw(responder),
            SetInterpMethod {
                interp_method,
                responder,
            } => self.on_set_interp_method(interp_method, responder),
            InterpSingleFrame {
                frame_index,
                responder,
            } => self.on_interp_single_frame(frame_index, responder),
        }
    }

    fn handle_outcome(&mut self, outcome: Outcome) -> Result<()> {
        use Outcome::*;
        match outcome {
            ReadVideoMeta {
                video_meta,
                parameters,
            } => self.on_complete_read_video_meta(video_meta, parameters)?,
            LoadVideoPacket { video_path, packet } => {
                self.on_complete_load_video_packet(video_path, packet)?;
            }
            ReadDaq { daq_meta, daq_raw } => self.on_complete_read_daq(daq_meta, daq_raw)?,
            Interp { interpolator } => self.on_complete_interp(interpolator)?,
        }
        self.reconcile();

        Ok(())
    }

    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(Sender<Outcome>) + Send + 'static,
    {
        let outcome_sender = self.outcome_sender.clone();
        std::thread::spawn(move || f(outcome_sender));
    }

    fn video_data(&self) -> Result<&VideoData> {
        self.video_data
            .as_ref()
            .ok_or_else(|| anyhow!("video not loaded yet"))
    }

    fn video_meta(&self) -> Result<VideoMeta> {
        let video_path = self.setting_storage.video_path()?;
        let video_meta = self.video_data()?.video_meta();
        if video_meta.path != video_path {
            bail!("new video not loaded yet");
        }

        Ok(video_meta.clone())
    }

    fn daq_data(&self) -> Result<&DaqData> {
        self.daq_data
            .as_ref()
            .ok_or_else(|| anyhow!("daq not loaded yet"))
    }

    fn daq_meta(&self) -> Result<DaqMeta> {
        let daq_path = self.setting_storage.daq_path()?;
        let daq_meta = self.daq_data()?.daq_meta();
        if daq_meta.path != daq_path {
            bail!("new daq not loaded yet");
        }

        Ok(daq_meta.clone())
    }

    fn daq_raw(&self) -> Result<ArcArray2<f64>> {
        let daq_path = self.setting_storage.daq_path()?;
        let daq_data = self.daq_data()?;
        if daq_data.daq_meta().path != daq_path {
            warn!("new daq not loaded yet, return old data anyway");
        }

        Ok(daq_data.daq_raw())
    }

    fn interpolator(&self) -> Result<Interpolator> {
        Ok(self
            .daq_data()?
            .interpolator()
            .ok_or_else(|| anyhow!("not interpolated yet"))?
            .clone())
    }
}
