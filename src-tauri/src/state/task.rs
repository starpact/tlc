use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Result};
use crossbeam::channel::{RecvTimeoutError, Sender};
use ndarray::ArcArray2;
use tlc_video::{
    filter_detect_peak, read_video, DecoderManager, GmaxId, Green2Id, Packet, VideoId,
};
use tracing::{debug, error, info_span, instrument};

use super::{output::Output, GlobalState};
use crate::{
    daq::{interp, read_daq, DaqId, InterpId, Interpolator},
    request::Responder,
    solve::{self, nan_mean, SolveId},
};

const NUM_TASK_TYPES: usize = 6;

const TYPE_ID_READ_VIDEO: usize = 0;
const TYPE_ID_READ_DAQ: usize = 1;
const TYPE_ID_BUILD_GREEN2: usize = 2;
const TYPE_ID_DETECT_PEAK: usize = 3;
const TYPE_ID_INTERP: usize = 4;
const TYPE_ID_SOLVE: usize = 5;

static DEPENDENCY_GRAPH: [&[usize]; NUM_TASK_TYPES] = [
    &[],                                     // read_video
    &[],                                     // read_daq
    &[TYPE_ID_READ_VIDEO, TYPE_ID_READ_DAQ], // build_green2
    &[TYPE_ID_BUILD_GREEN2],                 // detect_peak
    &[TYPE_ID_READ_VIDEO, TYPE_ID_READ_DAQ], // interp
    &[TYPE_ID_DETECT_PEAK, TYPE_ID_INTERP],  // solve
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
enum TaskId {
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

#[derive(Clone)]
enum TaskState {
    AlreadyCompleted,
    ReadyToGo(Task),
    DispatchedToOthers,
    CannotStart {
        #[allow(dead_code)] // used in log
        reason: String,
    },
}

impl std::fmt::Debug for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskState::AlreadyCompleted => f.debug_struct("AlreadyCompleted").finish(),
            TaskState::ReadyToGo(_) => f.debug_struct("ReadyToGo").finish(),
            TaskState::DispatchedToOthers => f.debug_struct("DispatchedToOthers").finish(),
            TaskState::CannotStart { reason } => f
                .debug_struct("CannotStart")
                .field("reason", reason)
                .finish(),
        }
    }
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
pub struct TaskRegistry {
    last_task_ids: [Option<Arc<TaskId>>; NUM_TASK_TYPES],
}

impl TaskRegistry {
    #[instrument(level = "debug", skip_all, err)]
    fn register(&mut self, task_id: TaskId) -> Result<Arc<TaskId>> {
        debug!(?task_id);
        let last_task_id = &mut self.last_task_ids[task_id.type_id()];
        if let Some(last_task_id) = last_task_id {
            if Arc::strong_count(last_task_id) == 1 {
                debug!("last task has already finished");
            } else if **last_task_id != task_id {
                debug!("last task has not finished but parameters are different");
            } else {
                bail!("last task with same parameters is still executing");
            }
        } else {
            debug!("no last task");
        }

        Ok(last_task_id.insert(Arc::new(task_id)).clone())
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

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_tasks(&self) -> Vec<Task> {
        let eval_read_video = || self.eval_read_video();
        let eval_read_daq = || self.eval_read_daq();
        let eval_build_green2 = || self.eval_build_green2();
        let eval_detect_peak = || self.eval_detect_peak();
        let eval_interp = || self.eval_interp();
        let eval_solve = || self.eval_solve();
        let mut evaluators = [
            LazyEvaluator::new(&eval_read_video),
            LazyEvaluator::new(&eval_read_daq),
            LazyEvaluator::new(&eval_build_green2),
            LazyEvaluator::new(&eval_detect_peak),
            LazyEvaluator::new(&eval_interp),
            LazyEvaluator::new(&eval_solve),
        ];

        let mut tasks = Vec::new();
        traverse_dependency_graph(&mut evaluators, &mut tasks, DEPENDENCY_GRAPH.len() - 1);

        tasks
    }

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_read_video(&self) -> TaskState {
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

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_read_daq(&self) -> TaskState {
        match self.daq_id() {
            Ok(daq_id) => match self.daq_data {
                Some(_) => TaskState::AlreadyCompleted,
                None => TaskState::ReadyToGo(Task::ReadDaq { daq_id }),
            },
            Err(e) => TaskState::CannotStart {
                reason: e.to_string(),
            },
        }
    }

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_build_green2(&self) -> TaskState {
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

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_detect_peak(&self) -> TaskState {
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

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_interp(&self) -> TaskState {
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

    #[instrument(level = "debug", skip(self), ret)]
    fn eval_solve(&self) -> TaskState {
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

    fn spawn<F>(&self, task_id: Arc<TaskId>, f: F)
    where
        F: FnOnce(Sender<Output>) + Send + 'static,
    {
        let output_sender = self.output_sender.clone();
        std::thread::spawn(move || {
            let _task_id = task_id;
            f(output_sender);
        });
    }

    pub fn spawn_read_video(&mut self, video_id: VideoId, responder: Option<Responder<()>>) {
        let task_id = match self
            .task_registry
            .register(TaskId::ReadVideo(video_id.clone()))
        {
            Ok(task_id) => task_id,
            Err(e) => {
                if let Some(responder) = responder {
                    responder.respond_err(e);
                }
                return;
            }
        };

        let progress_bar = self.video_controller.prepare_read_video();
        self.spawn(task_id, |output_sender| {
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
            output_sender
                .send(Output::ReadVideoMeta {
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
            for cnt in 0.. {
                match packet_rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(packet) => output_sender
                        .send(Output::LoadVideoPacket {
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
        });
    }

    pub fn spawn_read_daq(&mut self, daq_id: DaqId, responder: Option<Responder<()>>) {
        let task_id = match self.task_registry.register(TaskId::ReadDaq(daq_id.clone())) {
            Ok(task_id) => task_id,
            Err(e) => {
                if let Some(responder) = responder {
                    responder.respond_err(e);
                }
                return;
            }
        };

        self.spawn(task_id, |output_sender| match read_daq(&daq_id.daq_path) {
            Ok((daq_meta, daq_raw)) => {
                output_sender
                    .send(Output::ReadDaq {
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
        });
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

        let Ok(task_id) = self.task_registry.register(TaskId::BuildGreen2(green2_id.clone())) else {
            return;
        };
        let progress_bar = self.video_controller.prepare_build_green2();
        self.spawn(task_id, move |output_sender| {
            if let Ok(green2) =
                decoder_manager.decode_all(packets, start_frame, cal_num, area, progress_bar)
            {
                output_sender
                    .send(Output::BuildGreen2 { green2_id, green2 })
                    .unwrap();
            }
        });
    }

    fn spawn_detect_peak(&mut self, gmax_id: GmaxId, green2: ArcArray2<u8>) {
        let Ok(task_id) = self.task_registry.register(TaskId::DetectPeak(gmax_id.clone())) else {
            return;
        };
        let progress_bar = self.video_controller.prepare_detect_peak();
        self.spawn(task_id, move |output_sender| {
            if let Ok(gmax_frame_indexes) =
                filter_detect_peak(green2, gmax_id.filter_method, progress_bar)
            {
                output_sender
                    .send(Output::DetectPeak {
                        gmax_id,
                        gmax_frame_indexes,
                    })
                    .unwrap();
            }
        });
    }

    fn spawn_interp(&mut self, interp_id: InterpId, daq_raw: ArcArray2<f64>) {
        let Ok(task_id) = self.task_registry.register(TaskId::Interp(interp_id.clone())) else {
            return;
        };
        self.spawn(task_id, |output_sender| {
            if let Ok(interpolator) = interp(&interp_id, daq_raw) {
                output_sender
                    .send(Output::Interp {
                        interp_id,
                        interpolator,
                    })
                    .unwrap();
            }
        });
    }

    fn spawn_solve(
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
        let Ok(task_id) = self.task_registry.register(TaskId::Solve(solve_id.clone())) else {
            return;
        };
        let progress_bar = self.solve_controller.prepare_solve();
        self.spawn(task_id, move |output_sender| {
            if let Ok(nu2) = solve::solve(
                &gmax_frame_indexes,
                &interpolator,
                physical_param,
                iteration_method,
                frame_rate,
                progress_bar,
            ) {
                let nu_nan_mean = nan_mean(nu2.view());
                debug!(nu_nan_mean);

                output_sender
                    .send(Output::Solve {
                        solve_id,
                        nu2,
                        nu_nan_mean,
                    })
                    .unwrap();
            }
        });
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
    use std::{assert_matches::assert_matches, path::PathBuf};

    use tlc_video::{Parameters, VideoData, VideoMeta};

    use crate::{
        daq::{DaqData, DaqMeta, InterpMethod, Thermocouple},
        setting::{new_db_in_memory, CreateRequest, StartIndex},
    };

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
    fn test_traverse_dependency_graph_all() {
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
    fn test_eval_tasks_empty_state() {
        tlc_util::log::init();
        let global_state = GlobalState::new(new_db_in_memory());
        assert!(global_state.eval_tasks().is_empty());
    }

    #[test]
    fn test_eval_read_video() {
        tlc_util::log::init();
        let mut global_state = empty_global_state();

        assert_matches!(
            global_state.eval_read_video(),
            TaskState::CannotStart { reason } if reason == "video path unset",
        );

        global_state
            .setting
            .set_video_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(global_state.eval_read_video(), TaskState::ReadyToGo(..));

        global_state.video_data = Some(fake_video_data());
        assert_matches!(global_state.eval_read_video(), TaskState::AlreadyCompleted);
    }

    #[test]
    fn test_eval_read_daq() {
        tlc_util::log::init();
        let mut global_state = empty_global_state();

        assert_matches!(
            global_state.eval_read_daq(),
            TaskState::CannotStart { reason } if reason == "daq path unset",
        );

        global_state
            .setting
            .set_daq_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(global_state.eval_read_daq(), TaskState::ReadyToGo(..));

        global_state.daq_data = Some(fake_daq_data());
        assert_matches!(global_state.eval_read_daq(), TaskState::AlreadyCompleted);
    }

    #[test]
    fn test_eval_build_green2() {
        tlc_util::log::init();
        let mut global_state = empty_global_state();

        assert_matches!(
            global_state.eval_build_green2(),
            TaskState::CannotStart { reason } if reason == "video not loaded yet",
        );

        global_state.video_data = Some(fake_video_data());
        assert_matches!(
            global_state.eval_build_green2(),
            TaskState::CannotStart { reason } if reason == "loading packets not finished yet",
        );

        for i in 0..fake_video_data().video_meta().nframes {
            let mut packet = Packet::empty();
            packet.set_dts(Some(i as i64));
            global_state
                .video_data
                .as_mut()
                .unwrap()
                .push_packet(Arc::new(packet))
                .unwrap();
        }
        assert_matches!(
            global_state.eval_build_green2(),
            TaskState::CannotStart { reason } if reason == "video path unset",
        );

        global_state
            .setting
            .set_video_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(
            global_state.eval_build_green2(),
            TaskState::CannotStart { reason } if reason == "daq not loaded yet",
        );

        global_state.daq_data = Some(fake_daq_data());
        assert_matches!(
            global_state.eval_build_green2(),
            TaskState::CannotStart { reason } if reason == "video and daq not synchronized yet",
        );

        global_state
            .setting
            .set_start_index(
                &global_state.db,
                Some(StartIndex {
                    start_frame: 10,
                    start_row: 2,
                }),
            )
            .unwrap();
        assert_matches!(
            global_state.eval_build_green2(),
            TaskState::CannotStart { reason } if reason == "area not selected yet",
        );

        global_state
            .setting
            .set_area(&global_state.db, Some((10, 10, 200, 200)))
            .unwrap();
        assert_matches!(global_state.eval_build_green2(), TaskState::ReadyToGo(..));
    }

    #[test]
    fn test_eval_detect_peak() {
        tlc_util::log::init();
        let mut global_state = empty_global_state();

        assert_matches!(
            global_state.eval_detect_peak(),
            TaskState::CannotStart { reason } if reason == "video not loaded yet",
        );

        global_state.video_data = Some(fake_video_data());
        assert_matches!(
            global_state.eval_detect_peak(),
            TaskState::CannotStart { reason } if reason == "video path unset",
        );

        global_state
            .setting
            .set_video_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(
            global_state.eval_detect_peak(),
            TaskState::CannotStart { reason } if reason == "daq not loaded yet",
        );

        global_state.daq_data = Some(fake_daq_data());
        assert_matches!(
            global_state.eval_detect_peak(),
            TaskState::CannotStart { reason } if reason == "video and daq not synchronized yet",
        );

        global_state
            .setting
            .set_start_index(
                &global_state.db,
                Some(StartIndex {
                    start_frame: 10,
                    start_row: 2,
                }),
            )
            .unwrap();
        assert_matches!(
            global_state.eval_detect_peak(),
            TaskState::CannotStart { reason } if reason == "area not selected yet",
        );

        global_state
            .setting
            .set_area(&global_state.db, Some((10, 10, 200, 200)))
            .unwrap();
        assert_matches!(
            global_state.eval_detect_peak(),
            TaskState::CannotStart { reason } if reason == "green2 not built yet",
        );

        global_state
            .video_data
            .as_mut()
            .unwrap()
            .set_green2(Some(ArcArray2::zeros((1, 1))));
        assert_matches!(global_state.eval_detect_peak(), TaskState::ReadyToGo(..));
    }

    #[test]
    fn test_eval_interp() {
        tlc_util::log::init();
        let mut global_state = empty_global_state();

        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "daq not loaded yet",
        );

        global_state.daq_data = Some(fake_daq_data());
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "daq path unset",
        );

        global_state
            .setting
            .set_daq_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "video and daq not synchronized yet",
        );

        global_state
            .setting
            .set_start_index(
                &global_state.db,
                Some(StartIndex {
                    start_frame: 10,
                    start_row: 2,
                }),
            )
            .unwrap();
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "video path unset",
        );

        global_state
            .setting
            .set_video_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "video not loaded yet",
        );

        global_state.video_data = Some(fake_video_data());
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "area not selected yet",
        );

        global_state
            .setting
            .set_area(&global_state.db, Some((10, 10, 200, 200)))
            .unwrap();
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "thermocouples unset",
        );

        global_state
            .setting
            .set_thermocouples(
                &global_state.db,
                Some(&[
                    Thermocouple {
                        column_index: 1,
                        position: (10, 20),
                    },
                    Thermocouple {
                        column_index: 2,
                        position: (100, 200),
                    },
                ]),
            )
            .unwrap();
        assert_matches!(
            global_state.eval_interp(),
            TaskState::CannotStart { reason } if reason == "interp method unset",
        );

        global_state
            .setting
            .set_interp_method(&global_state.db, InterpMethod::Horizontal)
            .unwrap();
        assert_matches!(global_state.eval_interp(), TaskState::ReadyToGo(..));
    }

    #[test]
    fn test_eval_solve() {
        tlc_util::log::init();
        let mut global_state = empty_global_state();

        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "video path unset",
        );

        global_state
            .setting
            .set_video_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "video not loaded yet",
        );

        global_state.video_data = Some(fake_video_data());
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "daq not loaded yet",
        );

        global_state.daq_data = Some(fake_daq_data());
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "video and daq not synchronized yet",
        );

        global_state
            .setting
            .set_start_index(
                &global_state.db,
                Some(StartIndex {
                    start_frame: 10,
                    start_row: 2,
                }),
            )
            .unwrap();
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "area not selected yet",
        );

        global_state
            .setting
            .set_area(&global_state.db, Some((10, 10, 200, 200)))
            .unwrap();
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "daq path unset",
        );

        global_state
            .setting
            .set_daq_path(&global_state.db, &PathBuf::from("aaa"))
            .unwrap();
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "thermocouples unset",
        );

        global_state
            .setting
            .set_thermocouples(
                &global_state.db,
                Some(&[
                    Thermocouple {
                        column_index: 1,
                        position: (10, 20),
                    },
                    Thermocouple {
                        column_index: 2,
                        position: (100, 200),
                    },
                ]),
            )
            .unwrap();
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "interp method unset",
        );

        global_state
            .setting
            .set_interp_method(&global_state.db, InterpMethod::Horizontal)
            .unwrap();
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "not detect peak yet",
        );

        global_state
            .video_data
            .as_mut()
            .unwrap()
            .set_gmax_frame_indexes(Some(Arc::new(Vec::new())));
        assert_matches!(
            global_state.eval_solve(),
            TaskState::CannotStart { reason } if reason == "not interp yet",
        );

        global_state
            .daq_data
            .as_mut()
            .unwrap()
            .set_interpolator(Some(Interpolator::default()));
        assert_matches!(global_state.eval_solve(), TaskState::ReadyToGo(..));

        println!("{}", std::mem::size_of::<ArcArray2<f64>>());
    }

    #[test]
    fn test_task_controller_accept_same_type_different_param() {
        tlc_util::log::init();

        let mut task_controller = TaskRegistry::default();
        let _task_id = task_controller
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("aaa"),
            }))
            .unwrap();
        let _task_id = task_controller
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("bbb"),
            }))
            .unwrap();
    }

    #[test]
    fn test_task_controller_reject_same_type_same_param() {
        tlc_util::log::init();

        let mut task_controller = TaskRegistry::default();
        let _task_id = task_controller
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("aaa"),
            }))
            .unwrap();
        let _task_id = task_controller
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("aaa"),
            }))
            .unwrap_err();
    }

    #[test]
    fn test_task_controller_accept_different_type() {
        tlc_util::log::init();

        let mut task_controller = TaskRegistry::default();
        let _task_id = task_controller
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("aaa"),
            }))
            .unwrap();
        let _task_id = task_controller
            .register(TaskId::ReadDaq(DaqId {
                daq_path: PathBuf::from("aaa"),
            }))
            .unwrap();
    }

    #[test]
    fn test_task_controller_accept_same_type_same_param_after_finished() {
        tlc_util::log::init();

        let mut task_registry = TaskRegistry::default();
        let _task_id = task_registry
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("aaa"),
            }))
            .unwrap();
        drop(_task_id);

        let _task_id = task_registry
            .register(TaskId::ReadVideo(VideoId {
                video_path: PathBuf::from("aaa"),
            }))
            .unwrap();
    }

    fn empty_global_state() -> GlobalState {
        let mut global_state = GlobalState::new(new_db_in_memory());
        global_state
            .setting
            .create_setting(&global_state.db, CreateRequest::default())
            .unwrap();
        global_state
    }

    fn fake_video_data() -> VideoData {
        VideoData::new(
            VideoMeta {
                frame_rate: 25,
                nframes: 100,
                shape: (1024, 1280),
            },
            Parameters::default(),
        )
    }

    fn fake_daq_data() -> DaqData {
        DaqData::new(
            DaqMeta {
                nrows: 120,
                ncols: 10,
            },
            ArcArray2::zeros((120, 10)),
        )
    }
}
