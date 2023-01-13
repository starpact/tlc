use std::{sync::Arc, time::Duration};

use crossbeam::channel::{RecvTimeoutError, Sender};
use ndarray::ArcArray2;
use tracing::{debug, debug_span, error};

use super::{GlobalState, Output, Task, TaskId};
use crate::{
    daq::{interp, read_daq, DaqId, InterpId, Interpolator},
    request::Responder,
    solve::{self, nan_mean, SolveId},
    video::{filter_detect_peak, read_video, DecoderManager, GmaxId, Green2Id, Packet, VideoId},
};

impl GlobalState {
    pub fn spawn_execute_task(&mut self, task: Task) {
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

    /// This could be triggered by `request` or `reconcile`, so `responder` is optional.
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

            let _span = debug_span!("receive_loaded_packets", nframes).entered();
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

    /// This could be triggered both by `request` or `reconcile`, so `responder` is optional.
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
}
