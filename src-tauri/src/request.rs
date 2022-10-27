use std::{path::PathBuf, time::Instant};

use anyhow::Result;
use ndarray::{ArcArray2, Array2};
use tokio::sync::oneshot;
use tracing::trace;
use video::VideoMeta;

use crate::daq::{DaqMeta, InterpMethod};

pub enum Request {
    GetSaveRootDir {
        responder: Responder<PathBuf>,
    },
    SetSaveRootDir {
        save_root_dir: PathBuf,
        responder: Responder<()>,
    },
    GetVideoMeta {
        responder: Responder<VideoMeta>,
    },
    SetVideoPath {
        video_path: PathBuf,
        responder: Responder<()>,
    },
    GetDaqMeta {
        responder: Responder<DaqMeta>,
    },
    SetDaqPath {
        daq_path: PathBuf,
        responder: Responder<()>,
    },
    GetDaqRaw {
        responder: Responder<ArcArray2<f64>>,
    },
    SetInterpMethod {
        interp_method: InterpMethod,
        responder: Responder<()>,
    },
    InterpSingleFrame {
        frame_index: usize,
        responder: Responder<Array2<f64>>,
    },
}

pub struct Responder<T> {
    name: String,
    payload: Option<String>,
    start_time: Instant,
    tx: oneshot::Sender<Result<T>>,
}

impl<T> Responder<T> {
    pub fn new(
        name: &str,
        parameter: Option<String>,
        tx: oneshot::Sender<Result<T>>,
    ) -> Responder<T> {
        Responder {
            name: name.to_owned(),
            payload: parameter,
            tx,
            start_time: Instant::now(),
        }
    }

    pub fn respond(self, result: Result<T>) {
        if self.tx.send(result).is_err() {
            panic!("failed to send back response");
        }

        let name = self.name;
        let payload = self.payload;
        let elapsed = self.start_time.elapsed();
        trace!(name, ?payload, ?elapsed);
    }

    pub fn respond_ok(self, v: T) {
        self.respond(Ok(v))
    }

    pub fn respond_err(self, e: anyhow::Error) {
        self.respond(Err(e))
    }
}

#[cfg(test)]
mod tests {
    use crate::util;

    use super::*;

    #[test]
    fn test_respond_log_output() {
        util::log::init();

        let (tx, _rx) = oneshot::channel::<Result<()>>();
        let payload = "some_payload: aaa".to_owned();
        let responder = Responder::new("some_event", Some(payload), tx);
        responder.respond_ok(());
    }
}
