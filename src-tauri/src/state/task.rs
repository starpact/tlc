mod eval;
mod execute;
mod output;

use std::sync::Arc;

use anyhow::{bail, Result};
use ndarray::ArcArray2;
use tlc_video::{DecoderManager, GmaxId, Green2Id, Packet, VideoId};
use tracing::{debug, instrument};

use super::GlobalState;
use crate::{
    daq::{DaqId, InterpId, Interpolator},
    solve::SolveId,
};
pub use output::Output;

const NUM_TASK_TYPES: usize = 6;

const TYPE_ID_READ_VIDEO: usize = 0;
const TYPE_ID_READ_DAQ: usize = 1;
const TYPE_ID_BUILD_GREEN2: usize = 2;
const TYPE_ID_DETECT_PEAK: usize = 3;
const TYPE_ID_INTERP: usize = 4;
const TYPE_ID_SOLVE: usize = 5;

#[derive(Clone)]
pub enum Task {
    ReadVideo {
        video_id: VideoId,
    },
    ReadDaq {
        daq_id: DaqId,
    },
    BuildGreen2 {
        green2_id: Green2Id,
        decoder_manager: DecoderManager,
        packets: Vec<Arc<Packet>>,
    },
    DetectPeak {
        gmax_id: GmaxId,
        green2: ArcArray2<u8>,
    },
    Interp {
        interp_id: InterpId,
        daq_raw: ArcArray2<f64>,
    },
    Solve {
        solve_id: SolveId,
        gmax_frame_indexes: Arc<Vec<usize>>,
        interpolator: Interpolator,
    },
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Task::ReadVideo { video_id } => f
                .debug_struct("ReadVideo")
                .field("video_id", video_id)
                .finish(),
            Task::ReadDaq { daq_id } => f.debug_struct("ReadDaq").field("daq_id", daq_id).finish(),
            Task::BuildGreen2 { green2_id, .. } => f
                .debug_struct("BuildGreen2")
                .field("green2_id", green2_id)
                .finish(),
            Task::DetectPeak { gmax_id, .. } => f
                .debug_struct("DetectPeak")
                .field("gmax_id", gmax_id)
                .finish(),
            Task::Interp { interp_id, .. } => f
                .debug_struct("Interp")
                .field("interp_id", interp_id)
                .finish(),
            Task::Solve { solve_id, .. } => {
                f.debug_struct("Solve").field("solve_id", solve_id).finish()
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum TaskId {
    ReadVideo(VideoId),
    ReadDaq(DaqId),
    BuildGreen2(Green2Id),
    DetectPeak(GmaxId),
    Interp(InterpId),
    Solve(SolveId),
}

impl TaskId {
    fn type_id(&self) -> usize {
        match self {
            TaskId::ReadVideo(_) => TYPE_ID_READ_VIDEO,
            TaskId::ReadDaq(_) => TYPE_ID_READ_DAQ,
            TaskId::BuildGreen2(_) => TYPE_ID_BUILD_GREEN2,
            TaskId::DetectPeak(_) => TYPE_ID_DETECT_PEAK,
            TaskId::Interp(_) => TYPE_ID_INTERP,
            TaskId::Solve(_) => TYPE_ID_SOLVE,
        }
    }
}

#[derive(Default)]
pub struct TaskRegistry {
    last_task_ids: [Option<Arc<TaskId>>; NUM_TASK_TYPES],
}

impl TaskRegistry {
    #[instrument(level = "debug", skip_all, err)]
    pub fn register(&mut self, task_id: TaskId) -> Result<Arc<TaskId>> {
        debug!(?task_id);
        let last_task_id = &mut self.last_task_ids[task_id.type_id()];
        if let Some(last_task_id) = last_task_id {
            if Arc::strong_count(last_task_id) == 1 {
                debug!("last task has already finished, ok");
            } else if **last_task_id != task_id {
                debug!("last task has not finished but parameters are different, ok");
            } else {
                bail!("last task with same parameters is still executing");
            }
        } else {
            debug!("no last task, ok");
        }

        Ok(last_task_id.insert(Arc::new(task_id)).clone())
    }
}

impl GlobalState {
    pub fn reconcile(&mut self) {
        for task in self.eval_tasks() {
            self.spawn_execute_task(task);
        }
    }
}
