use std::path::PathBuf;

use anyhow::Result;
use crossbeam::channel::Sender;
use function_name::named;
use ndarray::{ArcArray2, Array2};
use tokio::sync::oneshot;

use crate::{
    daq::{DaqMeta, InterpMethod},
    event::{
        Event::{self, *},
        Responder,
    },
    video::VideoMeta,
};

#[named]
pub async fn get_video_meta(event_sender: &Sender<Event>) -> Result<VideoMeta> {
    let (tx, rx) = oneshot::channel();
    let _ = event_sender.try_send(GetVideoMeta {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.unwrap()
}

#[named]
pub async fn set_video_path(video_path: PathBuf, event_sender: &Sender<Event>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("video_path: {video_path:?}");
    let _ = event_sender.try_send(SetVideoPath {
        video_path,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.unwrap()
}

#[named]
pub async fn get_daq_meta(event_sender: &Sender<Event>) -> Result<DaqMeta> {
    let (tx, rx) = oneshot::channel();
    let _ = event_sender.try_send(GetDaqMeta {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.unwrap()
}

#[named]
pub async fn set_daq_path(daq_path: PathBuf, event_sender: &Sender<Event>) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("daq_path: {daq_path:?}");
    let _ = event_sender.try_send(SetDaqPath {
        daq_path,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.unwrap()
}

#[named]
pub async fn get_daq_raw(event_sender: &Sender<Event>) -> Result<ArcArray2<f64>> {
    let (tx, rx) = oneshot::channel();
    let _ = event_sender.try_send(GetDaqRaw {
        responder: Responder::new(function_name!(), None, tx),
    });
    rx.await.unwrap()
}

#[named]
pub async fn set_interp_method(
    interp_method: InterpMethod,
    event_sender: &Sender<Event>,
) -> Result<()> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("interp_method: {interp_method:?}");
    let _ = event_sender.try_send(SetInterpMethod {
        interp_method,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.unwrap()
}

#[named]
pub async fn interp_single_frame(
    frame_index: usize,
    event_sender: &Sender<Event>,
) -> Result<Array2<f64>> {
    let (tx, rx) = oneshot::channel();
    let payload = format!("frame_index: {frame_index}");
    let _ = event_sender.try_send(InterpSingleFrame {
        frame_index,
        responder: Responder::new(function_name!(), Some(payload), tx),
    });
    rx.await.unwrap()
}
