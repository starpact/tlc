use anyhow::{anyhow, Result};
use tlc_video::GmaxId;
use tracing::instrument;

use super::{
    GlobalState, Task, NUM_TASK_TYPES, TYPE_ID_BUILD_GREEN2, TYPE_ID_DETECT_PEAK, TYPE_ID_INTERP,
    TYPE_ID_READ_DAQ, TYPE_ID_READ_VIDEO,
};

static DEPENDENCY_GRAPH: [&[usize]; NUM_TASK_TYPES] = [
    &[],                                     // read_video
    &[],                                     // read_daq
    &[TYPE_ID_READ_VIDEO, TYPE_ID_READ_DAQ], // build_green2
    &[TYPE_ID_BUILD_GREEN2],                 // detect_peak
    &[TYPE_ID_READ_VIDEO, TYPE_ID_READ_DAQ], // interp
    &[TYPE_ID_DETECT_PEAK, TYPE_ID_INTERP],  // solve
];

static EVALUATORS: [Eval; NUM_TASK_TYPES] = [
    GlobalState::eval_read_video,
    GlobalState::eval_read_daq,
    GlobalState::eval_build_green2,
    GlobalState::eval_detect_peak,
    GlobalState::eval_interp,
    GlobalState::eval_solve,
];

#[derive(Clone)]
enum TaskState {
    AlreadyCompleted,
    ReadyToGo(Task),
    DispatchedToOthers,
    CannotStart { reason: String },
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
    fn from(x: Result<Option<Task>>) -> TaskState {
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
    fn from(x: Result<Task>) -> TaskState {
        match x {
            Ok(task) => TaskState::ReadyToGo(task),
            Err(e) => TaskState::CannotStart {
                reason: e.to_string(),
            },
        }
    }
}

type Eval = fn(&GlobalState) -> TaskState;

struct LazyEvaluator {
    eval: Eval,
    value: Option<TaskState>,
}

impl LazyEvaluator {
    fn new(eval: fn(&GlobalState) -> TaskState) -> LazyEvaluator {
        LazyEvaluator { eval, value: None }
    }

    fn eval(&mut self, global_state: &GlobalState) -> TaskState {
        match &self.value {
            Some(value) => value.clone(),
            None => match (self.eval)(global_state) {
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
    #[instrument(level = "debug", skip(self), ret)]
    pub fn eval_tasks(&self) -> Vec<Task> {
        let mut tasks = Vec::new();
        self.traverse_dependency_graph(
            &mut EVALUATORS.map(LazyEvaluator::new),
            DEPENDENCY_GRAPH,
            &mut tasks,
            DEPENDENCY_GRAPH.len() - 1,
        );

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

    fn traverse_dependency_graph<const N: usize>(
        &self,
        evaluators: &mut [LazyEvaluator; N],
        dependency_graph: [&[usize]; N],
        tasks: &mut Vec<Task>,
        task_id: usize,
    ) -> bool {
        let mut all_dependencies_ready = true;
        for &task_id in dependency_graph[task_id] {
            if !self.traverse_dependency_graph(evaluators, dependency_graph, tasks, task_id) {
                all_dependencies_ready = false;
            }
        }
        if !all_dependencies_ready {
            return false;
        }

        match evaluators[task_id].eval(self) {
            TaskState::AlreadyCompleted => true,
            TaskState::ReadyToGo(task) => {
                tasks.push(task);
                false
            }
            TaskState::DispatchedToOthers | TaskState::CannotStart { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{assert_matches::assert_matches, path::PathBuf, sync::Arc};

    use ndarray::ArcArray2;
    use tlc_video::{Packet, Parameters, VideoData, VideoId, VideoMeta};

    use crate::{
        daq::{DaqData, DaqId, DaqMeta, InterpMethod, Interpolator, Thermocouple},
        setting::{new_db_in_memory, CreateRequest, StartIndex},
        state::task::{TaskId, TaskRegistry},
    };

    use super::*;

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

        fn eval_already_completed(_: &GlobalState) -> TaskState {
            TaskState::AlreadyCompleted
        }

        fn eval_unreachable(_: &GlobalState) -> TaskState {
            unreachable!()
        }

        fn eval_go_1(_: &GlobalState) -> TaskState {
            TaskState::ReadyToGo(fake_task("1"))
        }

        fn eval_go_2(_: &GlobalState) -> TaskState {
            TaskState::ReadyToGo(fake_task("2"))
        }

        fn eval_go_3(_: &GlobalState) -> TaskState {
            TaskState::ReadyToGo(fake_task("3"))
        }

        fn eval_go_4(_: &GlobalState) -> TaskState {
            TaskState::ReadyToGo(fake_task("4"))
        }

        fn eval_go_5(_: &GlobalState) -> TaskState {
            TaskState::ReadyToGo(fake_task("5"))
        }

        fn eval_cannot_start(_: &GlobalState) -> TaskState {
            TaskState::CannotStart {
                reason: "".to_owned(),
            }
        }

        let cases: [(_, [Eval; 6], _, _); 8] = [
            (
                "all_ready",
                [
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                ],
                true,
                vec![],
            ),
            (
                "read_video_and_daq",
                [
                    eval_go_1,
                    eval_go_2,
                    eval_unreachable,
                    eval_unreachable,
                    eval_unreachable,
                    eval_unreachable,
                ],
                false,
                vec![fake_task("1"), fake_task("2")],
            ),
            (
                "read_video_cannot_read_daq",
                [
                    eval_go_1,
                    eval_cannot_start,
                    eval_unreachable,
                    eval_unreachable,
                    eval_unreachable,
                    eval_unreachable,
                ],
                false,
                vec![fake_task("1")],
            ),
            (
                "build_green2_and_interp",
                [
                    eval_already_completed,
                    eval_already_completed,
                    eval_go_2,
                    eval_unreachable,
                    eval_go_4,
                    eval_unreachable,
                ],
                false,
                vec![fake_task("2"), fake_task("4")],
            ),
            (
                "build_green2_no_need_interp",
                [
                    eval_already_completed,
                    eval_already_completed,
                    eval_go_3,
                    eval_unreachable,
                    eval_already_completed,
                    eval_unreachable,
                ],
                false,
                vec![fake_task("3")],
            ),
            (
                "cannot_build_green2_because_prerequisites_not_met",
                [
                    eval_already_completed,
                    eval_cannot_start,
                    eval_go_3,
                    eval_unreachable,
                    eval_unreachable,
                    eval_unreachable,
                ],
                false,
                vec![],
            ),
            (
                "detect_peak_and_interp",
                [
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                    eval_go_3,
                    eval_go_4,
                    eval_unreachable,
                ],
                false,
                vec![fake_task("3"), fake_task("4")],
            ),
            (
                "solve",
                [
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                    eval_already_completed,
                    eval_go_5,
                    eval_unreachable,
                ],
                false,
                vec![fake_task("5")],
            ),
        ];

        for (case_name, evals, ready_expected, tasks_expected) in cases {
            println!("case_name: {case_name}");
            let mut evaluators = evals.map(LazyEvaluator::new);
            let mut tasks = Vec::new();
            let ready = GlobalState::new(new_db_in_memory()).traverse_dependency_graph(
                &mut evaluators,
                DEPENDENCY_GRAPH,
                &mut tasks,
                DEPENDENCY_GRAPH.len() - 1,
            );
            assert_eq!(ready_expected, ready);
            assert_eq!(tasks_expected, tasks);
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
}
