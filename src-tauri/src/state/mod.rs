mod outcome_handler;
mod request_handler;

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use crossbeam::{
    channel::{bounded, Receiver, Sender},
    select,
};
use ndarray::ArcArray2;
use tlc_video::{
    filter_detect_peak, FilterMethod, GmaxMeta, Green2Meta, Packet, Parameters, VideoController,
    VideoData, VideoMeta,
};
use tracing::{error, instrument, warn};

use crate::{
    daq::{interp, DaqData, DaqMeta, InterpMeta, InterpMethod, Interpolator},
    request::Request,
    setting::{SettingStorage, SqliteSettingStorage, StartIndex},
};

const SQLITE_FILEPATH: &str = "./var/db.sqlite3";

struct GlobalState<S: SettingStorage> {
    setting_storage: S,

    outcome_sender: Sender<Outcome>,
    outcome_receiver: Receiver<Outcome>,

    video_data: Option<VideoData>,
    video_controller: VideoController,

    daq_data: Option<DaqData>,
}

enum Outcome {
    ReadVideoMeta {
        video_meta: VideoMeta,
        parameters: Parameters,
    },
    LoadVideoPacket {
        video_meta: Arc<VideoMeta>,
        packet: Arc<Packet>,
    },
    ReadDaq {
        daq_meta: DaqMeta,
        daq_raw: ArcArray2<f64>,
    },
    BuildGreen2 {
        green2_meta: Green2Meta,
        green2: ArcArray2<u8>,
    },
    DetectPeak {
        gmax_meta: GmaxMeta,
        gmax_frame_indexes: Arc<Vec<usize>>,
    },
    Interp {
        interpolator: Interpolator,
    },
}

pub fn main_loop(request_receiver: Receiver<Request>) {
    let setting_storage = SqliteSettingStorage::new(SQLITE_FILEPATH);
    let mut global_state = GlobalState::new(setting_storage);
    loop {
        if let Err(e) = global_state.handle(&request_receiver) {
            error!("{e}");
        }
    }
}

impl<S: SettingStorage> GlobalState<S> {
    fn new(setting_storage: S) -> Self {
        let (outcome_sender, outcome_receiver) = bounded(3);
        Self {
            setting_storage,
            outcome_sender,
            outcome_receiver,
            video_data: None,
            daq_data: None,
            video_controller: VideoController::default(),
        }
    }

    /// `handle` keeps receiving `Request`(frontend message) and `Outcome`(computation
    /// result), then make decision what to do next based on the current global state.
    /// It should NEVER block or do any heavy computations, all blocking/time-consuming
    /// tasks should be executed in other threads and send back results asynchronously
    /// through `outcome_sender`.
    fn handle(&mut self, request_receiver: &Receiver<Request>) -> Result<()> {
        select! {
            recv(request_receiver)  -> request => self.handle_request(request?),
            recv(self.outcome_receiver) -> outcome => self.handle_outcome(outcome?)?,
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) {
        use Request::*;
        match request {
            GetSaveRootDir { responder } => self.on_get_save_root_dir(responder),
            SetSaveRootDir {
                save_root_dir,
                responder,
            } => self.on_set_save_root_dir(save_root_dir, responder),
            GetVideoMeta { responder } => self.on_get_video_meta(responder),
            SetVideoPath {
                video_path,
                responder,
            } => self.on_set_video_path(video_path, responder),
            GetReadVideoProgress { responder } => self.on_get_read_video_progress(responder),
            DecodeFrameBase64 {
                frame_index,
                responder,
            } => self.on_decode_frame_base64(frame_index, responder),
            GetDaqMeta { responder } => self.on_get_daq_meta(responder),
            SetDaqPath {
                daq_path,
                responder,
            } => self.on_set_daq_path(daq_path, responder),
            GetDaqRaw { responder } => self.on_get_daq_raw(responder),
            SynchronizeVideoAndDaq {
                start_frame,
                start_row,
                responder,
            } => self.on_synchronize_video_and_daq(start_frame, start_row, responder),
            GetStartIndex { responder } => self.on_get_start_index(responder),
            SetStartFrame {
                start_frame,
                responder,
            } => self.on_set_start_frame(start_frame, responder),
            SetStartRow {
                start_row,
                responder,
            } => self.on_set_start_row(start_row, responder),
            GetArea { responder } => self.on_get_area(responder),
            SetArea { area, responder } => self.on_set_area(area, responder),
            SetInterpMethod {
                interp_method,
                responder,
            } => self.on_set_interp_method(interp_method, responder),
            GetBuildGreen2Progress { responder } => self.on_get_build_green2_progress(responder),
            GetFilterMethod { responder } => self.on_get_filter_method(responder),
            SetFilterMethod {
                filter_method,
                responder,
            } => self.on_set_filter_method(filter_method, responder),
            GetDetectPeakProgress { responder } => self.on_get_detect_peak_progress(responder),
            InterpSingleFrame {
                frame_index,
                responder,
            } => self.on_interp_single_frame(frame_index, responder),
        }
    }

    fn handle_outcome(&mut self, outcome: Outcome) -> Result<()> {
        use Outcome::*;
        match outcome {
            ReadVideoMeta {
                video_meta,
                parameters,
            } => self.on_complete_read_video_meta(video_meta, parameters)?,
            LoadVideoPacket { video_meta, packet } => {
                self.on_complete_load_video_packet(video_meta, packet)?;
            }
            ReadDaq { daq_meta, daq_raw } => self.on_complete_read_daq(daq_meta, daq_raw)?,
            BuildGreen2 {
                green2_meta,
                green2,
            } => self.on_complete_build_green2(green2_meta, green2)?,
            Interp { interpolator } => self.on_complete_interp(interpolator)?,
            DetectPeak {
                gmax_meta,
                gmax_frame_indexes,
            } => self.on_complete_detect_peak(gmax_meta, gmax_frame_indexes)?,
        }

        Ok(())
    }

    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(Sender<Outcome>) + Send + 'static,
    {
        let outcome_sender = self.outcome_sender.clone();
        std::thread::spawn(move || f(outcome_sender));
    }

    fn video_data(&self) -> Result<&VideoData> {
        self.video_data
            .as_ref()
            .ok_or_else(|| anyhow!("video not loaded yet"))
    }

    fn video_data_mut(&mut self) -> Result<&mut VideoData> {
        self.video_data
            .as_mut()
            .ok_or_else(|| anyhow!("video not loaded yet"))
    }

    fn video_meta(&self) -> Result<VideoMeta> {
        let video_path = self.setting_storage.video_path()?;
        let video_meta = self.video_data()?.video_meta();
        if video_meta.path != video_path {
            bail!("new video not loaded yet");
        }

        Ok(video_meta.clone())
    }

    fn daq_data(&self) -> Result<&DaqData> {
        self.daq_data
            .as_ref()
            .ok_or_else(|| anyhow!("daq not loaded yet"))
    }

    fn daq_meta(&self) -> Result<DaqMeta> {
        let daq_path = self.setting_storage.daq_path()?;
        let daq_meta = self.daq_data()?.daq_meta();
        if daq_meta.path != daq_path {
            bail!("new daq not loaded yet");
        }

        Ok(daq_meta.clone())
    }

    fn daq_raw(&self) -> Result<ArcArray2<f64>> {
        let daq_path = self.setting_storage.daq_path()?;
        let daq_data = self.daq_data()?;
        if daq_data.daq_meta().path != daq_path {
            warn!("new daq not loaded yet, return old data anyway");
        }

        Ok(daq_data.daq_raw())
    }

    fn synchronize_video_and_daq(&mut self, start_frame: usize, start_row: usize) -> Result<()> {
        let nframes = self.video_data()?.video_meta().nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }

        let nrows = self.daq_data()?.daq_meta().nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.setting_storage
            .set_start_index(start_frame, start_row)?;

        let video_data = self.video_data_mut()?;
        video_data.set_green2(None);
        video_data.set_gmax_frame_indexes(None);

        Ok(())
    }

    fn set_start_frame(&mut self, start_frame: usize) -> Result<()> {
        let nframes = self.video_data()?.video_meta().nframes;
        if start_frame >= nframes {
            bail!("frame_index({start_frame}) out of range({nframes})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.setting_storage.start_index()?;

        let nrows = self.daq_data()?.daq_meta().nrows;
        if old_start_row + start_frame < old_start_frame {
            bail!("invalid start_frame");
        }

        let start_row = old_start_row + start_frame - old_start_frame;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        self.setting_storage
            .set_start_index(start_frame, start_row)?;

        let video_data = self.video_data_mut()?;
        video_data.set_green2(None);
        video_data.set_gmax_frame_indexes(None);

        Ok(())
    }

    fn set_start_row(&mut self, start_row: usize) -> Result<()> {
        let nrows = self.daq_data()?.daq_meta().nrows;
        if start_row >= nrows {
            bail!("row_index({start_row}) out of range({nrows})");
        }

        let StartIndex {
            start_frame: old_start_frame,
            start_row: old_start_row,
        } = self.setting_storage.start_index()?;

        let nframes = self.video_data()?.video_meta().nframes;
        if old_start_frame + start_row < old_start_row {
            bail!("invalid start_row");
        }
        let start_frame = old_start_frame + start_row - old_start_row;
        if start_frame >= nframes {
            bail!("frames_index({start_frame}) out of range({nframes})");
        }

        self.setting_storage
            .set_start_index(start_frame, start_row)?;

        let video_data = self.video_data_mut()?;
        video_data.set_green2(None);
        video_data.set_gmax_frame_indexes(None);

        Ok(())
    }

    fn set_area(&mut self, area: (u32, u32, u32, u32)) -> Result<()> {
        let (h, w) = self.video_data()?.video_meta().shape;
        let (tl_y, tl_x, cal_h, cal_w) = area;
        if tl_x + cal_w > w {
            bail!("area X out of range: top_left_x({tl_x}) + width({cal_w}) > video_width({w})");
        }
        if tl_y + cal_h > h {
            bail!("area Y out of range: top_left_y({tl_y}) + height({cal_h}) > video_height({h})");
        }

        self.setting_storage.set_area(area)?;

        let video_data = self.video_data_mut()?;
        video_data.set_green2(None);
        video_data.set_gmax_frame_indexes(None);

        Ok(())
    }

    fn set_filter_method(&mut self, filter_method: FilterMethod) -> Result<()> {
        self.setting_storage.set_filter_method(filter_method)?;
        self.video_data_mut()?.set_gmax_frame_indexes(None);
        Ok(())
    }

    #[instrument(skip(self), err)]
    fn spwan_build_green2(&mut self) -> Result<()> {
        let video_data = self.video_data()?;
        let decoder_manager = video_data.decoder_manager();
        let packets = video_data.packets()?;
        let green2_meta = self.green2_meta()?;
        let progress_bar = self.video_controller.prepare_build_green2();

        self.spawn(move |outcome_sender| {
            if let Ok(green2) = decoder_manager.decode_all(packets, &green2_meta, progress_bar) {
                outcome_sender
                    .send(Outcome::BuildGreen2 {
                        green2_meta,
                        green2: green2.into_shared(),
                    })
                    .unwrap();
            }
        });

        Ok(())
    }

    fn spawn_detect_peak(&mut self) -> Result<()> {
        let green2_meta = self.green2_meta()?;
        let green2 = self
            .video_data()?
            .green2()
            .ok_or_else(|| anyhow!("green2 not built yet"))?;
        let filter_method = self.setting_storage.filter_method()?;
        let progress_bar = self.video_controller.prepare_detect_peak();
        let gmax_meta = GmaxMeta {
            filter_method,
            green2_meta,
        };

        self.spawn(move |outcome_sender| {
            if let Ok(gmax_frame_indexes) = filter_detect_peak(green2, filter_method, progress_bar)
            {
                outcome_sender
                    .send(Outcome::DetectPeak {
                        gmax_meta,
                        gmax_frame_indexes: Arc::new(gmax_frame_indexes),
                    })
                    .unwrap();
            }
        });

        Ok(())
    }

    fn set_interp_method(&self, interp_method: InterpMethod) -> Result<()> {
        let mut interp_meta = self.interp_meta()?;
        if interp_meta.interp_method == interp_method {
            warn!("interp method unchanged, compute again anyway");
        } else {
            interp_meta.interp_method = interp_method;
        }

        self.setting_storage.set_interp_method(interp_method)
    }

    #[instrument(skip(self), err)]
    fn spawn_interp(&self) -> Result<()> {
        let daq_raw = self.daq_data()?.daq_raw();
        let interp_meta = self.interp_meta()?;
        let interpolator = interp(interp_meta, daq_raw)?;
        self.spawn(|outcome_sender| {
            outcome_sender
                .send(Outcome::Interp { interpolator })
                .unwrap();
        });

        Ok(())
    }

    fn green2_meta(&self) -> Result<Green2Meta> {
        let video_data = self.video_data()?;
        let video_meta = video_data.video_meta().clone();
        let StartIndex {
            start_frame,
            start_row,
        } = self.setting_storage.start_index()?;
        let nframes = video_meta.nframes;
        let nrows = self.daq_meta()?.nrows;
        let cal_num = (nframes - start_frame).min(nrows - start_row);
        let area = self.setting_storage.area()?;

        Ok(Green2Meta {
            video_meta,
            start_frame,
            cal_num,
            area,
        })
    }

    fn interp_meta(&self) -> Result<InterpMeta> {
        let daq_path = self.setting_storage.daq_path()?;
        let start_row = self.setting_storage.start_index()?.start_row;
        let Green2Meta { cal_num, area, .. } = self.green2_meta()?;
        let thermocouples = self.setting_storage.thermocouples()?;
        let interp_method = self.setting_storage.interp_method()?;

        Ok(InterpMeta {
            daq_path,
            start_row,
            cal_num,
            area,
            interp_method,
            thermocouples,
        })
    }

    fn interpolator(&self) -> Result<Interpolator> {
        Ok(self
            .daq_data()?
            .interpolator()
            .ok_or_else(|| anyhow!("not interpolated yet"))?
            .clone())
    }
}
