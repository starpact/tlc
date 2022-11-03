use std::sync::Arc;

use ndarray::Array2;
use tlc_video::{GmaxId, Green2Id, Packet, Parameters, VideoId, VideoMeta};

use crate::{
    daq::{DaqId, DaqMeta, InterpId, Interpolator},
    solve::SolveId,
};

pub enum Output {
    ReadVideoMeta {
        video_id: VideoId,
        video_meta: VideoMeta,
        parameters: Parameters,
    },
    LoadVideoPacket {
        video_id: Arc<VideoId>,
        packet: Packet,
    },
    ReadDaq {
        daq_id: DaqId,
        daq_meta: DaqMeta,
        daq_raw: Array2<f64>,
    },
    BuildGreen2 {
        green2_id: Green2Id,
        green2: Array2<u8>,
    },
    DetectPeak {
        gmax_id: GmaxId,
        gmax_frame_indexes: Vec<usize>,
    },
    Interp {
        interp_id: InterpId,
        interpolator: Interpolator,
    },
    Solve {
        solve_id: SolveId,
        nu2: Array2<f64>,
        nu_nan_mean: f64,
    },
}
