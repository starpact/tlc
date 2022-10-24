mod outcome_handler;
mod request_handler;

use anyhow::{anyhow, Result};
use crossbeam::{
    channel::{unbounded, Receiver, Sender},
    select,
};
use ndarray::ArcArray2;
use tracing::error;

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
    daq_data: Option<DaqData>,
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

enum Outcome {
    ReadDaq {
        daq_meta: DaqMeta,
        daq_raw: ArcArray2<f64>,
    },
    Interp {
        interpolator: Interpolator,
    },
}

impl<S: SettingStorage> GlobalState<S> {
    fn new(setting_storage: S) -> Self {
        let (outcome_sender, outcome_receiver) = unbounded();
        Self {
            setting_storage,
            outcome_sender,
            outcome_receiver,
            daq_data: None,
        }
    }

    /// `handle` keeps receiving `Request`(frontend message) and `Outcome`(computation
    /// result), then make decision what to do next based on the current global state.
    /// It should NEVER block or do any heavy computations, all blocking/time-consuming
    /// tasks should be executed in other threads and send back results asynchronously
    /// through `outcome_sender`.
    ///
    /// There are two types of handlers for different kinds of events:
    /// - Return a `Result`, then caller will send the result. For fast handlers which
    /// can be executed in the same thread.
    /// - Take the `responder` and return nothing. The `responder` will be passed all
    /// the way down to other threads and used to send the result.
    fn handle(&mut self, request_receiver: &Receiver<Request>) -> Result<()> {
        select! {
            recv(request_receiver)  -> request => self.handle_request(request?),
            recv(self.outcome_receiver) -> outcome => self.handle_outcome(outcome?),
        }
    }

    fn handle_request(&mut self, request: Request) -> Result<()> {
        use Request::*;
        match request {
            GetVideoMeta { responder } => responder.respond(self.get_video_meta()),
            SetVideoPath {
                video_path,
                responder,
            } => self.set_video_path(video_path, responder),
            GetDaqMeta { responder } => responder.respond(self.get_daq_meta()),
            SetDaqPath {
                daq_path,
                responder,
            } => self.set_daq_path(daq_path, responder),
            GetDaqRaw { responder } => responder.respond(self.get_daq_raw()),
            SetInterpMethod {
                interp_method,
                responder,
            } => self.set_interp_method(interp_method, responder),
            InterpSingleFrame {
                frame_index,
                responder,
            } => self.interp_single_frame(frame_index, responder),
        }

        Ok(())
    }

    fn handle_outcome(&mut self, outcome: Outcome) -> Result<()> {
        use Outcome::*;
        match outcome {
            ReadDaq { daq_meta, daq_raw } => self.on_event_read_daq(daq_meta, daq_raw)?,
            Interp { interpolator } => self.on_event_interp(interpolator)?,
        }

        Ok(())
    }

    fn daq_data(&self) -> Result<&DaqData> {
        self.daq_data
            .as_ref()
            .ok_or_else(|| anyhow!("daq not loaded yet"))
    }

    fn daq_data_mut(&mut self) -> Result<&mut DaqData> {
        self.daq_data
            .as_mut()
            .ok_or_else(|| anyhow!("daq not loaded yet"))
    }

    fn interpolator(&self) -> Result<Interpolator> {
        Ok(self
            .daq_data()?
            .interpolator()
            .ok_or_else(|| anyhow!("not interpolated yet"))?
            .clone())
    }
}
