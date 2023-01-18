use std::{io::Write, path::Path};

use image::ColorType::Rgb8;
use ndarray::{prelude::*, ArrayView, Dimension};
use once_cell::sync::OnceCell;
use plotters::prelude::*;
use serde::Serialize;
use tracing::{info, instrument};

use crate::{
    daq::{read_daq, DaqMeta, DaqPathId, InterpMethodId, ThermocouplesId},
    solve::{IterMethodId, Nu2Id, PhysicalParamId},
    state::{NameId, SaveRootDirId, StartIndexId},
    video::{read_video, AreaId, FilterMethodId, VideoPathId},
    FilterMethod, InterpMethod, IterMethod, PhysicalParam, Thermocouple, VideoMeta,
};

/// `SettingSnapshot` will be saved together with the results for later check.
#[derive(Debug, Serialize)]
struct Setting<'a> {
    /// User defined unique name of this experiment setting.
    pub name: &'a str,

    /// Directory in which you save your data(parameters and results) of this experiment.
    /// * setting_path: {root_dir}/setting_{expertiment_name}.json
    /// * nu_matrix_path: {root_dir}/nu_matrix_{expertiment_name}.csv
    /// * nu_plot_path: {root_dir}/nu_plot_{expertiment_name}.png
    pub save_root_dir: &'a Path,

    pub video_path: &'a Path,

    pub video_meta: VideoMeta,

    pub daq_path: &'a Path,

    pub daq_meta: DaqMeta,

    /// Start frame of video involved in the calculation.
    /// Updated simultaneously with start_row.
    pub start_frame: usize,

    /// Start row of DAQ data involved in the calculation.
    /// Updated simultaneously with start_frame.
    pub start_row: usize,

    /// Calculation area(top_left_y, top_left_x, area_height, area_width).
    pub area: (u32, u32, u32, u32),

    /// Columns in the csv file and positions of thermocouples.
    pub thermocouples: &'a [Thermocouple],

    /// Filter method of green matrix along the time axis.
    pub filter_method: FilterMethod,

    /// Interpolation method for calculating thermocouple temperature distribution.
    pub interp_method: InterpMethod,

    /// Iteration method for solving heat transfer equataion.
    pub iter_method: IterMethod,

    /// All physical parameters used when solving heat transfer equation.
    pub physical_param: PhysicalParam,

    /// Final result.
    pub nu_nan_mean: f64,

    /// Timestamp in milliseconds.
    #[serde(with = "time::serde::rfc3339")]
    pub saved_at: time::OffsetDateTime,
}

#[salsa::tracked]
pub(crate) fn save_setting(
    db: &dyn crate::Db,
    name_id: NameId,
    save_root_dir_id: SaveRootDirId,
    video_path_id: VideoPathId,
    daq_path_id: DaqPathId,
    start_index_id: StartIndexId,
    area_id: AreaId,
    thermocouples_id: ThermocouplesId,
    filter_method_id: FilterMethodId,
    interp_method_id: InterpMethodId,
    iter_method_id: IterMethodId,
    physical_param_id: PhysicalParamId,
    nu2_id: Nu2Id,
) -> Result<(), String> {
    let video_data_id = read_video(db, video_path_id)?;
    let video_meta = VideoMeta {
        frame_rate: video_data_id.frame_rate(db),
        nframes: video_data_id.packets(db).0.len(),
        shape: video_data_id.shape(db),
    };
    let daq_data = read_daq(db, daq_path_id)?.data(db).0;
    let daq_meta = DaqMeta {
        nrows: daq_data.nrows(),
        ncols: daq_data.ncols(),
    };

    let nu2 = nu2_id.nu2(db).0;
    let setting_path = save_root_dir_id
        .save_root_dir(db)
        .join(format!("{}_setting", name_id.name(db)))
        .with_extension("json");

    let setting = Setting {
        name: name_id.name(db),
        save_root_dir: save_root_dir_id.save_root_dir(db),
        video_path: video_path_id.path(db),
        video_meta,
        daq_path: daq_path_id.path(db),
        daq_meta,
        start_frame: start_index_id.start_frame(db),
        start_row: start_index_id.start_row(db),
        area: area_id.area(db),
        thermocouples: thermocouples_id.thermocouples(db),
        filter_method: filter_method_id.filter_method(db),
        interp_method: interp_method_id.interp_method(db),
        iter_method: iter_method_id.iter_method(db),
        physical_param: physical_param_id.physical_param(db),
        saved_at: time::OffsetDateTime::now_local().unwrap(),
        nu_nan_mean: nan_mean(nu2.view()),
    };

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(setting_path)
        .map_err(|e| e.to_string())?;
    let buf = serde_json::to_string_pretty(&setting).map_err(|e| e.to_string())?;
    file.write_all(buf.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[salsa::tracked]
pub(crate) fn save_nu_matrix(
    db: &dyn crate::Db,
    name_id: NameId,
    save_root_dir_id: SaveRootDirId,
    nu2_id: Nu2Id,
) -> Result<(), String> {
    let nu2 = nu2_id.nu2(db).0;
    let nu_matrix_path = save_root_dir_id
        .save_root_dir(db)
        .join(format!("{}_nu_matrix", name_id.name(db)))
        .with_extension("csv");
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_path(nu_matrix_path)
        .map_err(|e| e.to_string())?;
    for row in nu2.rows() {
        let v: Vec<_> = row.iter().map(|x| x.to_string()).collect();
        wtr.write_record(&csv::StringRecord::from(v))
            .map_err(|e| e.to_string())?
    }
    Ok(())
}

pub(crate) fn nan_mean<D: Dimension>(data: ArrayView<f64, D>) -> f64 {
    let (sum, non_nan_cnt, cnt) = data.iter().fold((0., 0, 0), |(sum, non_nan_cnt, cnt), &x| {
        if x.is_nan() {
            (sum, non_nan_cnt, cnt + 1)
        } else {
            (sum + x, non_nan_cnt + 1, cnt + 1)
        }
    });
    let nan_ratio = (cnt - non_nan_cnt) as f64 / cnt as f64;
    info!(non_nan_cnt, cnt, nan_ratio);
    sum / non_nan_cnt as f64
}

/// All fields not NAN.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Trunc(pub f64, pub f64);

impl Eq for Trunc {}

impl std::hash::Hash for Trunc {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
        self.1.to_bits().hash(state);
    }
}

#[salsa::interned]
pub(crate) struct TruncId {
    trunc: Option<Trunc>,
}

#[salsa::tracked]
pub(crate) fn save_nu_plot(
    db: &dyn crate::Db,
    name_id: NameId,
    save_root_dir_id: SaveRootDirId,
    nu2_id: Nu2Id,
    trunc_id: TruncId,
) -> Result<String, String> {
    let nu2 = nu2_id.nu2(db).0;
    let nu_nan_mean = nan_mean(nu2.view());
    let trunc = match trunc_id.trunc(db) {
        Some(Trunc(min, max)) => (min, max),
        None => (nu_nan_mean * 0.6, nu_nan_mean * 2.0),
    };
    let buf = draw_area(nu2.view(), trunc).map_err(|e| e.to_string())?;
    let nu_plot_path = save_root_dir_id
        .save_root_dir(db)
        .join(format!("{}_nu_plot", name_id.name(db)))
        .with_extension("png");
    let (h, w) = nu2.dim();
    image::save_buffer(nu_plot_path, &buf, w as u32, h as u32, Rgb8).map_err(|e| e.to_string())?;
    Ok(base64::encode(buf))
}

#[instrument(skip_all, fields(edge_truncation), err)]
fn draw_area(area: ArrayView2<f64>, edge_truncation: (f64, f64)) -> anyhow::Result<Vec<u8>> {
    static CELL: OnceCell<[[f64; 3]; 256]> = OnceCell::new();

    let (h, w) = area.dim();
    let mut buf = vec![0; h * w * 3];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (w as u32, h as u32)).into_drawing_area();
        let chart = ChartBuilder::on(&root).build_cartesian_2d(0..w, 0..h)?;
        let pix_plotter = chart.plotting_area();

        let (min, max) = edge_truncation;
        let jet = CELL.get_or_init(|| JET.map(|[r, g, b]| [r * 255.0, g * 255.0, b * 255.0]));

        let mut iter = area.into_iter();
        for y in 0..h {
            for x in 0..w {
                if let Some(nu) = iter.next() {
                    if nu.is_nan() {
                        pix_plotter.draw_pixel((x, y), &WHITE)?;
                        continue;
                    }
                    let color_index = ((nu.clamp(min, max) - min) / (max - min) * 255.0) as usize;
                    let [r, g, b] = jet[color_index];
                    pix_plotter.draw_pixel((x, y), &RGBColor(r as u8, g as u8, b as u8))?;
                }
            }
        }
    }
    Ok(buf)
}

/// jet colormap from Matlab
const JET: [[f64; 3]; 256] = [
    [0., 0., 0.515625000000000],
    [0., 0., 0.531250000000000],
    [0., 0., 0.546875000000000],
    [0., 0., 0.562500000000000],
    [0., 0., 0.578125000000000],
    [0., 0., 0.593750000000000],
    [0., 0., 0.609375000000000],
    [0., 0., 0.625000000000000],
    [0., 0., 0.640625000000000],
    [0., 0., 0.656250000000000],
    [0., 0., 0.671875000000000],
    [0., 0., 0.687500000000000],
    [0., 0., 0.703125000000000],
    [0., 0., 0.718750000000000],
    [0., 0., 0.734375000000000],
    [0., 0., 0.750000000000000],
    [0., 0., 0.765625000000000],
    [0., 0., 0.781250000000000],
    [0., 0., 0.796875000000000],
    [0., 0., 0.812500000000000],
    [0., 0., 0.828125000000000],
    [0., 0., 0.843750000000000],
    [0., 0., 0.859375000000000],
    [0., 0., 0.875000000000000],
    [0., 0., 0.890625000000000],
    [0., 0., 0.906250000000000],
    [0., 0., 0.921875000000000],
    [0., 0., 0.937500000000000],
    [0., 0., 0.953125000000000],
    [0., 0., 0.968750000000000],
    [0., 0., 0.984375000000000],
    [0., 0., 1.],
    [0., 0.0156250000000000, 1.],
    [0., 0.0312500000000000, 1.],
    [0., 0.0468750000000000, 1.],
    [0., 0.0625000000000000, 1.],
    [0., 0.0781250000000000, 1.],
    [0., 0.0937500000000000, 1.],
    [0., 0.109375000000000, 1.],
    [0., 0.125000000000000, 1.],
    [0., 0.140625000000000, 1.],
    [0., 0.156250000000000, 1.],
    [0., 0.171875000000000, 1.],
    [0., 0.187500000000000, 1.],
    [0., 0.203125000000000, 1.],
    [0., 0.218750000000000, 1.],
    [0., 0.234375000000000, 1.],
    [0., 0.250000000000000, 1.],
    [0., 0.265625000000000, 1.],
    [0., 0.281250000000000, 1.],
    [0., 0.296875000000000, 1.],
    [0., 0.312500000000000, 1.],
    [0., 0.328125000000000, 1.],
    [0., 0.343750000000000, 1.],
    [0., 0.359375000000000, 1.],
    [0., 0.375000000000000, 1.],
    [0., 0.390625000000000, 1.],
    [0., 0.406250000000000, 1.],
    [0., 0.421875000000000, 1.],
    [0., 0.437500000000000, 1.],
    [0., 0.453125000000000, 1.],
    [0., 0.468750000000000, 1.],
    [0., 0.484375000000000, 1.],
    [0., 0.500000000000000, 1.],
    [0., 0.515625000000000, 1.],
    [0., 0.531250000000000, 1.],
    [0., 0.546875000000000, 1.],
    [0., 0.562500000000000, 1.],
    [0., 0.578125000000000, 1.],
    [0., 0.593750000000000, 1.],
    [0., 0.609375000000000, 1.],
    [0., 0.625000000000000, 1.],
    [0., 0.640625000000000, 1.],
    [0., 0.656250000000000, 1.],
    [0., 0.671875000000000, 1.],
    [0., 0.687500000000000, 1.],
    [0., 0.703125000000000, 1.],
    [0., 0.718750000000000, 1.],
    [0., 0.734375000000000, 1.],
    [0., 0.750000000000000, 1.],
    [0., 0.765625000000000, 1.],
    [0., 0.781250000000000, 1.],
    [0., 0.796875000000000, 1.],
    [0., 0.812500000000000, 1.],
    [0., 0.828125000000000, 1.],
    [0., 0.843750000000000, 1.],
    [0., 0.859375000000000, 1.],
    [0., 0.875000000000000, 1.],
    [0., 0.890625000000000, 1.],
    [0., 0.906250000000000, 1.],
    [0., 0.921875000000000, 1.],
    [0., 0.937500000000000, 1.],
    [0., 0.953125000000000, 1.],
    [0., 0.968750000000000, 1.],
    [0., 0.984375000000000, 1.],
    [0., 1., 1.],
    [0.0156250000000000, 1., 0.984375000000000],
    [0.0312500000000000, 1., 0.968750000000000],
    [0.0468750000000000, 1., 0.953125000000000],
    [0.0625000000000000, 1., 0.937500000000000],
    [0.0781250000000000, 1., 0.921875000000000],
    [0.0937500000000000, 1., 0.906250000000000],
    [0.109375000000000, 1., 0.890625000000000],
    [0.125000000000000, 1., 0.875000000000000],
    [0.140625000000000, 1., 0.859375000000000],
    [0.156250000000000, 1., 0.843750000000000],
    [0.171875000000000, 1., 0.828125000000000],
    [0.187500000000000, 1., 0.812500000000000],
    [0.203125000000000, 1., 0.796875000000000],
    [0.218750000000000, 1., 0.781250000000000],
    [0.234375000000000, 1., 0.765625000000000],
    [0.250000000000000, 1., 0.750000000000000],
    [0.265625000000000, 1., 0.734375000000000],
    [0.281250000000000, 1., 0.718750000000000],
    [0.296875000000000, 1., 0.703125000000000],
    [0.312500000000000, 1., 0.687500000000000],
    [0.328125000000000, 1., 0.671875000000000],
    [0.343750000000000, 1., 0.656250000000000],
    [0.359375000000000, 1., 0.640625000000000],
    [0.375000000000000, 1., 0.625000000000000],
    [0.390625000000000, 1., 0.609375000000000],
    [0.406250000000000, 1., 0.593750000000000],
    [0.421875000000000, 1., 0.578125000000000],
    [0.437500000000000, 1., 0.562500000000000],
    [0.453125000000000, 1., 0.546875000000000],
    [0.468750000000000, 1., 0.531250000000000],
    [0.484375000000000, 1., 0.515625000000000],
    [0.500000000000000, 1., 0.500000000000000],
    [0.515625000000000, 1., 0.484375000000000],
    [0.531250000000000, 1., 0.468750000000000],
    [0.546875000000000, 1., 0.453125000000000],
    [0.562500000000000, 1., 0.437500000000000],
    [0.578125000000000, 1., 0.421875000000000],
    [0.593750000000000, 1., 0.406250000000000],
    [0.609375000000000, 1., 0.390625000000000],
    [0.625000000000000, 1., 0.375000000000000],
    [0.640625000000000, 1., 0.359375000000000],
    [0.656250000000000, 1., 0.343750000000000],
    [0.671875000000000, 1., 0.328125000000000],
    [0.687500000000000, 1., 0.312500000000000],
    [0.703125000000000, 1., 0.296875000000000],
    [0.718750000000000, 1., 0.281250000000000],
    [0.734375000000000, 1., 0.265625000000000],
    [0.750000000000000, 1., 0.250000000000000],
    [0.765625000000000, 1., 0.234375000000000],
    [0.781250000000000, 1., 0.218750000000000],
    [0.796875000000000, 1., 0.203125000000000],
    [0.812500000000000, 1., 0.187500000000000],
    [0.828125000000000, 1., 0.171875000000000],
    [0.843750000000000, 1., 0.156250000000000],
    [0.859375000000000, 1., 0.140625000000000],
    [0.875000000000000, 1., 0.125000000000000],
    [0.890625000000000, 1., 0.109375000000000],
    [0.906250000000000, 1., 0.0937500000000000],
    [0.921875000000000, 1., 0.0781250000000000],
    [0.937500000000000, 1., 0.0625000000000000],
    [0.953125000000000, 1., 0.0468750000000000],
    [0.968750000000000, 1., 0.0312500000000000],
    [0.984375000000000, 1., 0.0156250000000000],
    [1., 1., 0.],
    [1., 0.984375000000000, 0.],
    [1., 0.968750000000000, 0.],
    [1., 0.953125000000000, 0.],
    [1., 0.937500000000000, 0.],
    [1., 0.921875000000000, 0.],
    [1., 0.906250000000000, 0.],
    [1., 0.890625000000000, 0.],
    [1., 0.875000000000000, 0.],
    [1., 0.859375000000000, 0.],
    [1., 0.843750000000000, 0.],
    [1., 0.828125000000000, 0.],
    [1., 0.812500000000000, 0.],
    [1., 0.796875000000000, 0.],
    [1., 0.781250000000000, 0.],
    [1., 0.765625000000000, 0.],
    [1., 0.750000000000000, 0.],
    [1., 0.734375000000000, 0.],
    [1., 0.718750000000000, 0.],
    [1., 0.703125000000000, 0.],
    [1., 0.687500000000000, 0.],
    [1., 0.671875000000000, 0.],
    [1., 0.656250000000000, 0.],
    [1., 0.640625000000000, 0.],
    [1., 0.625000000000000, 0.],
    [1., 0.609375000000000, 0.],
    [1., 0.593750000000000, 0.],
    [1., 0.578125000000000, 0.],
    [1., 0.562500000000000, 0.],
    [1., 0.546875000000000, 0.],
    [1., 0.531250000000000, 0.],
    [1., 0.515625000000000, 0.],
    [1., 0.500000000000000, 0.],
    [1., 0.484375000000000, 0.],
    [1., 0.468750000000000, 0.],
    [1., 0.453125000000000, 0.],
    [1., 0.437500000000000, 0.],
    [1., 0.421875000000000, 0.],
    [1., 0.406250000000000, 0.],
    [1., 0.390625000000000, 0.],
    [1., 0.375000000000000, 0.],
    [1., 0.359375000000000, 0.],
    [1., 0.343750000000000, 0.],
    [1., 0.328125000000000, 0.],
    [1., 0.312500000000000, 0.],
    [1., 0.296875000000000, 0.],
    [1., 0.281250000000000, 0.],
    [1., 0.265625000000000, 0.],
    [1., 0.250000000000000, 0.],
    [1., 0.234375000000000, 0.],
    [1., 0.218750000000000, 0.],
    [1., 0.203125000000000, 0.],
    [1., 0.187500000000000, 0.],
    [1., 0.171875000000000, 0.],
    [1., 0.156250000000000, 0.],
    [1., 0.140625000000000, 0.],
    [1., 0.125000000000000, 0.],
    [1., 0.109375000000000, 0.],
    [1., 0.0937500000000000, 0.],
    [1., 0.0781250000000000, 0.],
    [1., 0.0625000000000000, 0.],
    [1., 0.0468750000000000, 0.],
    [1., 0.0312500000000000, 0.],
    [1., 0.0156250000000000, 0.],
    [1., 0., 0.],
    [0.984375000000000, 0., 0.],
    [0.968750000000000, 0., 0.],
    [0.953125000000000, 0., 0.],
    [0.937500000000000, 0., 0.],
    [0.921875000000000, 0., 0.],
    [0.906250000000000, 0., 0.],
    [0.890625000000000, 0., 0.],
    [0.875000000000000, 0., 0.],
    [0.859375000000000, 0., 0.],
    [0.843750000000000, 0., 0.],
    [0.828125000000000, 0., 0.],
    [0.812500000000000, 0., 0.],
    [0.796875000000000, 0., 0.],
    [0.781250000000000, 0., 0.],
    [0.765625000000000, 0., 0.],
    [0.750000000000000, 0., 0.],
    [0.734375000000000, 0., 0.],
    [0.718750000000000, 0., 0.],
    [0.703125000000000, 0., 0.],
    [0.687500000000000, 0., 0.],
    [0.671875000000000, 0., 0.],
    [0.656250000000000, 0., 0.],
    [0.640625000000000, 0., 0.],
    [0.625000000000000, 0., 0.],
    [0.609375000000000, 0., 0.],
    [0.593750000000000, 0., 0.],
    [0.578125000000000, 0., 0.],
    [0.562500000000000, 0., 0.],
    [0.546875000000000, 0., 0.],
    [0.531250000000000, 0., 0.],
    [0.515625000000000, 0., 0.],
    [0.500000000000000, 0., 0.],
];
