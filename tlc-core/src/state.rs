#![allow(dead_code)]

use std::{path::PathBuf, sync::atomic::AtomicBool};

use crossbeam::channel::{bounded, Sender};
use ndarray::ArcArray2;

use crate::video::VideoData;

enum TaskState<T> {
    InProcess(AtomicBool),
    Done(T),
}

struct Task<I, T> {
    inputs: I,
    state: TaskState<T>,
}

struct TaskStateTable {
    read_video: Task<PathBuf, VideoData>,
    read_daq: Task<PathBuf, ArcArray2<f64>>,
}

fn spawn_executor() {}

struct Executor {
    notifier: Sender<()>,
}

impl Executor {
    fn start(self) -> Sender<()> {
        let (tx, rx) = bounded::<()>(1);
        std::thread::spawn(move || loop {
            _ = rx.recv();
        });
        tx
    }

    fn make_plan(&self) -> ! {
        todo!()
    }

    fn execute(&self) {
        let notifier = self.notifier.clone();
        std::thread::spawn(move || {
            // do some work
            _ = notifier.send(());
        });
    }
}
