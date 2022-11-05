use anyhow::{anyhow, Result};
use tlc_video::GmaxId;
use tracing::instrument;

use super::{
    task::{
        Task, NUM_TASK_TYPES, TYPE_ID_BUILD_GREEN2, TYPE_ID_DETECT_PEAK, TYPE_ID_INTERP,
        TYPE_ID_READ_DAQ, TYPE_ID_READ_VIDEO,
    },
    GlobalStateInner,
};

static DEPENDENCY_GRAPH: [&[usize]; NUM_TASK_TYPES] = [
    &[],                                     // read_video
    &[],                                     // read_daq
    &[TYPE_ID_READ_VIDEO, TYPE_ID_READ_DAQ], // build_green2
    &[TYPE_ID_BUILD_GREEN2],                 // detect_peak
    &[TYPE_ID_READ_VIDEO, TYPE_ID_READ_DAQ], // interp
    &[TYPE_ID_DETECT_PEAK, TYPE_ID_INTERP],  // solve
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

struct LazyEvaluator<'a> {
    eval: &'a dyn Fn() -> TaskState,
    value: Option<TaskState>,
}

impl<'a> LazyEvaluator<'a> {
    fn new(eval: &dyn Fn() -> TaskState) -> LazyEvaluator {
        LazyEvaluator { eval, value: None }
    }

    fn eval(&mut self) -> TaskState {
        match &self.value {
            Some(value) => value.clone(),
            None => match (self.eval)() {
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

impl GlobalStateInner {
    #[instrument(level = "debug", skip(self), ret)]
    pub fn eval_tasks(&self) -> Vec<Task> {
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
    use std::{assert_matches::assert_matches, path::PathBuf, sync::Arc};

    use ndarray::ArcArray2;
    use tlc_video::{Packet, Parameters, VideoData, VideoId, VideoMeta};

    use crate::{
        daq::{DaqData, DaqMeta, InterpMethod, Interpolator, Thermocouple},
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
        let global_state = GlobalStateInner::new(new_db_in_memory());
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

    fn empty_global_state() -> GlobalStateInner {
        let mut global_state = GlobalStateInner::new(new_db_in_memory());
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
