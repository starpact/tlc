use std::sync::Arc;

use anyhow::Result;
use ndarray::ArcArray2;
use parking_lot::MutexGuard;
use tauri::async_runtime::spawn_blocking;
use tlc_video::{
    filter_detect_peak, read_video, DecoderManager, GmaxId, Green2Id, Packet, VideoId,
};
use tracing::debug;

use crate::{
    daq::{interp, read_daq, DaqId, InterpId, Interpolator},
    solve::{self, nan_mean, SolveId},
};

use super::{
    task::{Task, TaskId},
    GlobalState, GlobalStateInner,
};

impl GlobalState {
    pub fn spawn<F>(&self, task_id: Arc<TaskId>, f: F)
    where
        F: FnOnce(GlobalState) + Send + 'static,
    {
        let global_state = self.clone();
        spawn_blocking(move || {
            let _task_id = task_id;
            f(global_state);
        });
    }

    pub(super) fn reconcile(&self, mut global_state: MutexGuard<GlobalStateInner>) {
        for task in global_state.eval_tasks() {
            let _ = self.spawn_execute_task(&mut global_state, task);
        }
    }

    fn spawn_execute_task(&self, global_state: &mut GlobalStateInner, task: Task) -> Result<()> {
        match task {
            Task::ReadVideo { video_id } => self.spawn_read_video(global_state, video_id),
            Task::ReadDaq { daq_id } => self.spawn_read_daq(global_state, daq_id),
            Task::BuildGreen2 {
                green2_id,
                decoder_manager,
                packets,
            } => self.spawn_build_green2(global_state, green2_id, decoder_manager, packets),
            Task::DetectPeak { gmax_id, green2 } => {
                self.spawn_detect_peak(global_state, gmax_id, green2)
            }
            Task::Interp { interp_id, daq_raw } => {
                self.spawn_interp(global_state, interp_id, daq_raw)
            }
            Task::Solve {
                solve_id,
                gmax_frame_indexes,
                interpolator,
            } => self.spawn_solve(global_state, solve_id, gmax_frame_indexes, interpolator),
        }
    }

    fn spawn_read_video(
        &self,
        global_state: &mut GlobalStateInner,
        video_id: VideoId,
    ) -> Result<()> {
        let task_id = global_state
            .task_registry
            .register(TaskId::ReadVideo(video_id.clone()))?;
        let progress_bar = global_state.video_controller.prepare_read_video();
        self.spawn(task_id, move |global_state| {
            if let Ok((video_meta, parameters, packet_rx)) =
                read_video(&video_id.video_path, progress_bar)
            {
                if global_state
                    .handle_read_video_output1(&video_id, video_meta, parameters)
                    .is_ok()
                {
                    let _ = global_state.handle_read_video_output2(
                        &video_id,
                        packet_rx,
                        video_meta.nframes,
                    );
                }
            }
        });

        Ok(())
    }

    fn spawn_read_daq(&self, global_state: &mut GlobalStateInner, daq_id: DaqId) -> Result<()> {
        let task_id = global_state
            .task_registry
            .register(TaskId::ReadDaq(daq_id.clone()))?;
        self.spawn(task_id, move |global_state| {
            if let Ok((daq_meta, daq_raw)) = read_daq(&daq_id.daq_path) {
                let _ = global_state.handle_read_daq_output(&daq_id, daq_meta, daq_raw);
            }
        });

        Ok(())
    }

    fn spawn_build_green2(
        &self,
        global_state: &mut GlobalStateInner,
        green2_id: Green2Id,
        decoder_manager: DecoderManager,
        packets: Vec<Arc<Packet>>,
    ) -> Result<()> {
        let task_id = global_state
            .task_registry
            .register(TaskId::BuildGreen2(green2_id.clone()))?;
        let Green2Id {
            start_frame,
            cal_num,
            area,
            ..
        } = green2_id;
        let progress_bar = global_state.video_controller.prepare_build_green2();
        self.spawn(task_id, move |global_state| {
            if let Ok(green2) =
                decoder_manager.decode_all(packets, start_frame, cal_num, area, progress_bar)
            {
                let _ = global_state.handle_build_green2_output(&green2_id, green2);
            }
        });

        Ok(())
    }

    fn spawn_detect_peak(
        &self,
        global_state: &mut GlobalStateInner,
        gmax_id: GmaxId,
        green2: ArcArray2<u8>,
    ) -> Result<()> {
        let task_id = global_state
            .task_registry
            .register(TaskId::DetectPeak(gmax_id.clone()))?;
        let progress_bar = global_state.video_controller.prepare_detect_peak();
        self.spawn(task_id, move |global_state| {
            if let Ok(gmax_frame_indexes) =
                filter_detect_peak(green2, gmax_id.filter_method, progress_bar)
            {
                let _ = global_state.handle_detect_peak_output(&gmax_id, gmax_frame_indexes);
            }
        });

        Ok(())
    }

    fn spawn_interp(
        &self,
        global_state: &mut GlobalStateInner,
        interp_id: InterpId,
        daq_raw: ArcArray2<f64>,
    ) -> Result<()> {
        let task_id = global_state
            .task_registry
            .register(TaskId::Interp(interp_id.clone()))?;
        self.spawn(task_id, move |global_state| {
            if let Ok(interpolator) = interp(&interp_id, daq_raw) {
                let _ = global_state.handle_interp_output(&interp_id, interpolator);
            }
        });

        Ok(())
    }

    fn spawn_solve(
        &self,
        global_state: &mut GlobalStateInner,
        solve_id: SolveId,
        gmax_frame_indexes: Arc<Vec<usize>>,
        interpolator: Interpolator,
    ) -> Result<()> {
        let SolveId {
            frame_rate,
            iteration_method,
            physical_param,
            ..
        } = solve_id;
        let task_id = global_state
            .task_registry
            .register(TaskId::Solve(solve_id.clone()))?;
        let progress_bar = global_state.solve_controller.prepare_solve();
        self.spawn(task_id, move |global_state| {
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
                let _ = global_state.handle_solve_output(&solve_id, nu2, nu_nan_mean);
            }
        });

        Ok(())
    }
}
