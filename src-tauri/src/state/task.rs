#![allow(dead_code)]

use std::{
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use crossbeam::channel::{RecvTimeoutError, Sender};
use ndarray::ArcArray2;
use tlc_video::{
    filter_detect_peak, read_video, DecoderManager, GmaxId, Green2Id, Packet, VideoId,
};
use tracing::{debug, error, info_span, instrument};

use super::{outcome_handler::Outcome, GlobalState};
use crate::{
    daq::{interp, read_daq, DaqId, InterpId, Interpolator},
    request::Responder,
    solve::{self, nan_mean, SolveId},
};

const NUM_TASK_TYPES: usize = 6;

const TASK_TYPE_ID_READ_VIDEO: usize = 0;
const TASK_TYPE_ID_READ_DAQ: usize = 1;
const TASK_TYPE_ID_BUILD_GREEN2: usize = 2;
const TASK_TYPE_ID_DETECT_PEAK: usize = 3;
const TASK_TYPE_ID_INTERP: usize = 4;
const TASK_TYPE_ID_SOLVE: usize = 5;

const NO_BUSY_TASK: i64 = -1;

static DEPENDENCY_GRAPH: [&[usize]; NUM_TASK_TYPES] = [
    &[],                                               // read_video
    &[],                                               // read_daq
    &[TASK_TYPE_ID_READ_VIDEO, TASK_TYPE_ID_READ_DAQ], // build_green2
    &[TASK_TYPE_ID_BUILD_GREEN2],                      // detect_peak
    &[TASK_TYPE_ID_READ_VIDEO, TASK_TYPE_ID_READ_DAQ], // interp
    &[TASK_TYPE_ID_DETECT_PEAK, TASK_TYPE_ID_INTERP],  // solve
];

#[derive(Clone)]
enum Task {
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
            Task::ReadVideo {
                video_id: video_path,
            } => f
                .debug_struct("Task::ReadVideo")
                .field("video_path", video_path)
                .finish(),
            Task::ReadDaq { daq_id: daq_path } => f
                .debug_struct("Task::ReadDaq")
                .field("daq_path", daq_path)
                .finish(),
            Task::BuildGreen2 { green2_id, .. } => f
                .debug_struct("Task::BuildGreen2")
                .field("green2_id", green2_id)
                .finish(),
            Task::DetectPeak { gmax_id, .. } => f
                .debug_struct("Task::DetectPeak")
                .field("gmax_id", gmax_id)
                .finish(),
            Task::Interp { interp_id, .. } => f
                .debug_struct("Task::Interp")
                .field("interp_id", interp_id)
                .finish(),
            Task::Solve { solve_id, .. } => f
                .debug_struct("Task::Solve")
                .field("solve_id", solve_id)
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
enum TaskState {
    AlreadyCompleted,
    ReadyToGo(Task),
    DispatchedToOthers,
    CannotStart { reason: String },
}

impl From<Result<Option<Task>>> for TaskState {
    fn from(x: Result<Option<Task>>) -> Self {
        match x {
            Ok(None) => TaskState::AlreadyCompleted,
            Ok(Some(task)) => TaskState::ReadyToGo(task),
            Err(e) => TaskState::CannotStart {
                reason: e.to_string(),
            },
        }
    }
}

impl From<Result<Task>> for TaskState {
    fn from(x: Result<Task>) -> Self {
        match x {
            Ok(task) => TaskState::ReadyToGo(task),
            Err(e) => TaskState::CannotStart {
                reason: e.to_string(),
            },
        }
    }
}

#[derive(Default)]
pub struct TaskController {
    task_states: [Arc<AtomicI64>; NUM_TASK_TYPES],
}

#[derive(Debug)]
struct TaskGuard {
    state: Arc<AtomicI64>,
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.state.store(NO_BUSY_TASK, Ordering::Relaxed);
    }
}

impl TaskController {
    fn register(&self, task_type_id: usize, task_hash: u64) -> Result<TaskGuard> {
        let state = &self.task_states[task_type_id];
        let update_ret = state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
            let new = task_hash as i64;
            if old != new {
                Some(new)
            } else {
                None
            }
        });
        if update_ret.is_err() {
            bail!("already executing with same parameters");
        }

        Ok(TaskGuard {
            state: state.clone(),
        })
    }
}

struct LazyEvaluator<'a> {
    expr: &'a dyn Fn() -> TaskState,
    value: Option<TaskState>,
}

impl<'a> LazyEvaluator<'a> {
    fn new(expr: &dyn Fn() -> TaskState) -> LazyEvaluator {
        LazyEvaluator { expr, value: None }
    }

    fn eval(&mut self) -> TaskState {
        match &self.value {
            Some(value) => value.clone(),
            None => match (self.expr)() {
                task_state @ TaskState::ReadyToGo(_) => {
                    self.value = Some(TaskState::DispatchedToOthers);
                    task_state
                }
                TaskState::DispatchedToOthers => unreachable!(),
                task_state => self.value.insert(task_state).clone(),
            },
        }
    }
}

impl GlobalState {
    pub fn reconcile(&mut self) {
        for task in self.eval_tasks() {
            self.spawn_execute_task(task);
        }
    }

    #[instrument(skip(self), ret)]
    fn eval_tasks(&self) -> Vec<Task> {
        let should_read_video = || self.should_read_video();
        let should_read_daq = || self.should_read_daq();
        let should_build_green2 = || self.should_build_green2();
        let should_detect_peak = || self.should_detect_peak();
        let should_interp = || self.should_interp();
        let should_solve = || self.should_solve();
        let mut evaluators = [
            LazyEvaluator::new(&should_read_video),
            LazyEvaluator::new(&should_read_daq),
            LazyEvaluator::new(&should_build_green2),
            LazyEvaluator::new(&should_detect_peak),
            LazyEvaluator::new(&should_interp),
            LazyEvaluator::new(&should_solve),
        ];

        let mut tasks = Vec::new();
        traverse_dependency_graph(&mut evaluators, &mut tasks, DEPENDENCY_GRAPH.len() - 1);

        tasks
    }

    #[instrument(skip(self), ret)]
    fn should_read_video(&self) -> TaskState {
        match self.video_id() {
            Ok(video_id) => match self.video_data {
                Some(_) => TaskState::AlreadyCompleted,
                None => TaskState::ReadyToGo(Task::ReadVideo { video_id }),
            },
            Err(e) => TaskState::CannotStart {
                reason: e.to_string(),
            },
        }
    }

    #[instrument(skip(self), ret)]
    fn should_read_daq(&self) -> TaskState {
        match self.daq_id() {
            Ok(daq_id) => match self.video_data {
                Some(_) => TaskState::AlreadyCompleted,
                None => TaskState::ReadyToGo(Task::ReadDaq { daq_id }),
            },
            Err(e) => TaskState::CannotStart {
                reason: e.to_string(),
            },
        }
    }

    #[instrument(skip(self), ret)]
    fn should_build_green2(&self) -> TaskState {
        let f = || -> Result<Option<Task>> {
            let video_data = self.video_data()?;
            if video_data.green2().is_some() {
                return Ok(None);
            }
            let decoder_manager = video_data.decoder_manager();
            let packets = video_data.packets()?;
            let green2_id = self.green2_id()?;
            Ok(Some(Task::BuildGreen2 {
                green2_id,
                decoder_manager,
                packets,
            }))
        };

        f().into()
    }

    #[instrument(skip(self), ret)]
    fn should_detect_peak(&self) -> TaskState {
        let f = || -> Result<Option<Task>> {
            let video_data = self.video_data()?;
            if video_data.gmax_frame_indexes().is_some() {
                return Ok(None);
            }
            let green2_id = self.green2_id()?;
            let green2 = video_data
                .green2()
                .ok_or_else(|| anyhow!("green2 not built yet"))?;
            let filter_method = self.setting.filter_method(&self.db)?;
            let gmax_id = GmaxId {
                green2_id,
                filter_method,
            };
            Ok(Some(Task::DetectPeak { gmax_id, green2 }))
        };

        f().into()
    }

    #[instrument(skip(self), ret)]
    fn should_interp(&self) -> TaskState {
        let f = || -> Result<Option<Task>> {
            let daq_data = self.daq_data()?;
            if daq_data.interpolator().is_some() {
                return Ok(None);
            }
            let interp_id = self.interp_id()?;
            let daq_raw = daq_data.daq_raw();
            Ok(Some(Task::Interp { interp_id, daq_raw }))
        };

        f().into()
    }

    #[instrument(skip(self), ret)]
    fn should_solve(&self) -> TaskState {
        if self.nu_data.is_some() {
            return TaskState::AlreadyCompleted;
        }

        let f = || -> Result<Task> {
            let solve_id = self.solve_id()?;
            let gmax_frame_indexes = self
                .video_data()?
                .gmax_frame_indexes()
                .ok_or_else(|| anyhow!("not detect peak yet"))?;
            let interpolator = self
                .daq_data()?
                .interpolator()
                .cloned()
                .ok_or_else(|| anyhow!("not interp yet"))?;
            Ok(Task::Solve {
                solve_id,
                gmax_frame_indexes,
                interpolator,
            })
        };

        f().into()
    }

    fn spawn_execute_task(&mut self, task: Task) {
        match task {
            Task::ReadVideo { video_id } => self.spawn_read_video(video_id, None),
            Task::ReadDaq { daq_id } => self.spawn_read_daq(daq_id, None),
            Task::BuildGreen2 {
                green2_id,
                decoder_manager,
                packets,
            } => self.spawn_build_green2(green2_id, decoder_manager, packets),
            Task::DetectPeak { gmax_id, green2 } => self.spawn_detect_peak(gmax_id, green2),
            Task::Interp { interp_id, daq_raw } => self.spawn_interp(interp_id, daq_raw),
            Task::Solve {
                solve_id,
                gmax_frame_indexes,
                interpolator,
            } => self.spawn_solve(solve_id, gmax_frame_indexes, interpolator),
        }
    }

    fn spawn<F>(&self, task_id: usize, task_hash: u64, f: F)
    where
        F: FnOnce(Sender<Outcome>) + Send + 'static,
    {
        if let Ok(task_guard) = self.task_controller.register(task_id, task_hash) {
            let outcome_sender = self.outcome_sender.clone();
            std::thread::spawn(move || {
                let _task_guard = task_guard;
                f(outcome_sender);
            });
        }
    }

    pub fn spawn_read_video(&mut self, video_id: VideoId, responder: Option<Responder<()>>) {
        let progress_bar = self.video_controller.prepare_read_video();
        self.spawn(
            TASK_TYPE_ID_READ_VIDEO,
            video_id.eval_hash(),
            |outcome_sender| {
                let (video_meta, parameters, packet_rx) =
                    match read_video(&video_id.video_path, progress_bar) {
                        Ok(ret) => ret,
                        Err(e) => {
                            match responder {
                                Some(responder) => responder.respond_err(e),
                                None => error!(%e),
                            }
                            return;
                        }
                    };

                let nframes = video_meta.nframes;
                outcome_sender
                    .send(Outcome::ReadVideoMeta {
                        video_id: video_id.clone(),
                        video_meta,
                        parameters,
                    })
                    .unwrap();
                if let Some(responder) = responder {
                    responder.respond_ok(());
                }

                let _span = info_span!("receive_loaded_packets", nframes).entered();
                let video_id = Arc::new(video_id);
                for cnt in 1.. {
                    match packet_rx.recv_timeout(Duration::from_secs(1)) {
                        Ok(packet) => outcome_sender
                            .send(Outcome::LoadVideoPacket {
                                video_id: video_id.clone(),
                                packet,
                            })
                            .unwrap(),
                        Err(e) => {
                            match e {
                                RecvTimeoutError::Timeout => {
                                    error!("load packets got stuck for some reason");
                                }
                                RecvTimeoutError::Disconnected => debug_assert_eq!(cnt, nframes),
                            }
                            return;
                        }
                    }
                    debug_assert!(cnt < nframes);
                }
            },
        );
    }

    pub fn spawn_read_daq(&mut self, daq_id: DaqId, responder: Option<Responder<()>>) {
        self.spawn(
            TASK_TYPE_ID_READ_DAQ,
            daq_id.eval_hash(),
            |outcome_sender| match read_daq(&daq_id.daq_path) {
                Ok((daq_meta, daq_raw)) => {
                    outcome_sender
                        .send(Outcome::ReadDaq {
                            daq_id,
                            daq_meta,
                            daq_raw,
                        })
                        .unwrap();
                    if let Some(responder) = responder {
                        responder.respond_ok(());
                    }
                }
                Err(e) => match responder {
                    Some(responder) => responder.respond_err(e),
                    None => error!(%e),
                },
            },
        );
    }

    fn spawn_build_green2(
        &mut self,
        green2_id: Green2Id,
        decoder_manager: DecoderManager,
        packets: Vec<Arc<Packet>>,
    ) {
        let Green2Id {
            start_frame,
            cal_num,
            area,
            ..
        } = green2_id;

        let progress_bar = self.video_controller.prepare_build_green2();
        self.spawn(
            TASK_TYPE_ID_BUILD_GREEN2,
            green2_id.eval_hash(),
            move |outcome_sender| {
                if let Ok(green2) =
                    decoder_manager.decode_all(packets, start_frame, cal_num, area, progress_bar)
                {
                    outcome_sender
                        .send(Outcome::BuildGreen2 { green2_id, green2 })
                        .unwrap();
                }
            },
        );
    }

    pub fn spawn_detect_peak(&mut self, gmax_id: GmaxId, green2: ArcArray2<u8>) {
        let progress_bar = self.video_controller.prepare_detect_peak();
        self.spawn(
            TASK_TYPE_ID_DETECT_PEAK,
            gmax_id.eval_hash(),
            move |outcome_sender| {
                if let Ok(gmax_frame_indexes) =
                    filter_detect_peak(green2, gmax_id.filter_method, progress_bar)
                {
                    outcome_sender
                        .send(Outcome::DetectPeak {
                            gmax_id,
                            gmax_frame_indexes,
                        })
                        .unwrap();
                }
            },
        );
    }

    pub fn spawn_interp(&mut self, interp_id: InterpId, daq_raw: ArcArray2<f64>) {
        self.spawn(
            TASK_TYPE_ID_INTERP,
            interp_id.eval_hash(),
            |outcome_sender| {
                if let Ok(interpolator) = interp(&interp_id, daq_raw) {
                    outcome_sender
                        .send(Outcome::Interp {
                            interp_id,
                            interpolator,
                        })
                        .unwrap();
                }
            },
        );
    }

    pub fn spawn_solve(
        &mut self,
        solve_id: SolveId,
        gmax_frame_indexes: Arc<Vec<usize>>,
        interpolator: Interpolator,
    ) {
        let SolveId {
            frame_rate,
            iteration_method,
            physical_param,
            ..
        } = solve_id;
        self.spawn(
            TASK_TYPE_ID_SOLVE,
            solve_id.eval_hash(),
            move |outcome_sender| {
                let nu2 = solve::solve(
                    gmax_frame_indexes,
                    &interpolator,
                    physical_param,
                    iteration_method,
                    frame_rate,
                );

                let nu_nan_mean = nan_mean(nu2.view());
                debug!(nu_nan_mean);

                outcome_sender
                    .send(Outcome::Solve {
                        solve_id,
                        nu2,
                        nu_nan_mean,
                    })
                    .unwrap();
            },
        );
    }
}

fn traverse_dependency_graph(
    evaluators: &mut [LazyEvaluator],
    tasks: &mut Vec<Task>,
    task_id: usize,
) -> bool {
    assert_eq!(evaluators.len(), DEPENDENCY_GRAPH.len());

    let mut all_dependencies_ready = true;
    for &task_id in DEPENDENCY_GRAPH[task_id] {
        if !traverse_dependency_graph(evaluators, tasks, task_id) {
            all_dependencies_ready = false;
        }
    }
    if !all_dependencies_ready {
        return false;
    }

    match evaluators[task_id].eval() {
        TaskState::AlreadyCompleted => true,
        TaskState::ReadyToGo(task) => {
            tasks.push(task);
            false
        }
        TaskState::DispatchedToOthers | TaskState::CannotStart { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    impl PartialEq for Task {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (
                    Task::ReadVideo {
                        video_id: video_path1,
                    },
                    Task::ReadVideo {
                        video_id: video_path2,
                    },
                ) => video_path1 == video_path2,
                (Task::ReadDaq { daq_id: daq_path1 }, Task::ReadDaq { daq_id: daq_path2 }) => {
                    daq_path1 == daq_path2
                }
                (
                    Task::BuildGreen2 {
                        green2_id: green2_id1,
                        ..
                    },
                    Task::BuildGreen2 {
                        green2_id: green2_id2,
                        ..
                    },
                ) => green2_id1 == green2_id2,
                _ => false,
            }
        }
    }

    #[test]
    fn test_eval_tasks() {
        // Use `ReadVideo` to represent all tasks because it's easy to construct.
        fn fake_task(id: &str) -> Task {
            Task::ReadVideo {
                video_id: VideoId {
                    video_path: PathBuf::from(id),
                },
            }
        }

        for (case_name, exprs, ready_, tasks_) in [
            (
                "all_ready",
                [
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                ],
                true,
                vec![],
            ),
            (
                "read_video_and_daq",
                [
                    || TaskState::ReadyToGo(fake_task("1")),
                    || TaskState::ReadyToGo(fake_task("2")),
                    || unreachable!(),
                    || unreachable!(),
                    || unreachable!(),
                    || unreachable!(),
                ],
                false,
                vec![fake_task("1"), fake_task("2")],
            ),
            (
                "read_video_cannot_read_daq",
                [
                    || TaskState::ReadyToGo(fake_task("1")),
                    || TaskState::CannotStart {
                        reason: "xxx".to_owned(),
                    },
                    || unreachable!(),
                    || unreachable!(),
                    || unreachable!(),
                    || unreachable!(),
                ],
                false,
                vec![fake_task("1")],
            ),
            (
                "build_green2_and_interp",
                [
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::ReadyToGo(fake_task("2")),
                    || unreachable!(),
                    || TaskState::ReadyToGo(fake_task("4")),
                    || unreachable!(),
                ],
                false,
                vec![fake_task("2"), fake_task("4")],
            ),
            (
                "build_green2_no_need_interp",
                [
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::ReadyToGo(fake_task("3")),
                    || unreachable!(),
                    || TaskState::AlreadyCompleted,
                    || unreachable!(),
                ],
                false,
                vec![fake_task("3")],
            ),
            (
                "cannot_build_green2_because_prerequisites_not_met",
                [
                    || TaskState::AlreadyCompleted,
                    || TaskState::CannotStart {
                        reason: "xxx".to_owned(),
                    },
                    || TaskState::ReadyToGo(fake_task("3")),
                    || unreachable!(),
                    || unreachable!(),
                    || unreachable!(),
                ],
                false,
                vec![],
            ),
            (
                "detect_peak_and_interp",
                [
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::ReadyToGo(fake_task("3")),
                    || TaskState::ReadyToGo(fake_task("4")),
                    || unreachable!(),
                ],
                false,
                vec![fake_task("3"), fake_task("4")],
            ),
            (
                "solve",
                [
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::AlreadyCompleted,
                    || TaskState::ReadyToGo(fake_task("5")),
                ],
                false,
                vec![fake_task("5")],
            ),
        ] {
            println!("case_name: {case_name}");
            let mut evaluators: Vec<_> = exprs.iter().map(|x| LazyEvaluator::new(x)).collect();
            let mut tasks = Vec::new();
            let ready =
                traverse_dependency_graph(&mut evaluators, &mut tasks, DEPENDENCY_GRAPH.len() - 1);
            assert_eq!(ready, ready_,);
            assert_eq!(tasks, tasks_);
        }
    }

    #[test]
    fn test_task_controller() {
        let task_controller = TaskController::default();
        let task_guard = task_controller
            .register(TASK_TYPE_ID_BUILD_GREEN2, 666)
            .unwrap();
        task_controller
            .register(TASK_TYPE_ID_READ_DAQ, 666)
            .unwrap();
        task_controller
            .register(TASK_TYPE_ID_BUILD_GREEN2, 666)
            .unwrap_err();
        task_controller
            .register(TASK_TYPE_ID_BUILD_GREEN2, 777)
            .unwrap();

        std::thread::spawn(move || {
            let _task_guard = task_guard;
        })
        .join()
        .unwrap();

        task_controller
            .register(TASK_TYPE_ID_BUILD_GREEN2, 777)
            .unwrap();
    }
}
