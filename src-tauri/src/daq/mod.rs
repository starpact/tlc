mod interp;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::{prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub use interp::{interp, InterpMeta, InterpMethod, Interpolator};

pub struct DaqData {
    daq_meta: DaqMeta,
    daq_raw: ArcArray2<f64>,
    interpolator: Option<Interpolator>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DaqMeta {
    pub path: PathBuf,
    pub nrows: usize,
    pub ncols: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct Thermocouple {
    /// Column index of this thermocouple in the DAQ file.
    pub column_index: usize,
    /// Position of this thermocouple(y, x). Thermocouples
    /// may not be in the video area, so coordinate can be negative.
    pub position: (i32, i32),
}

impl DaqData {
    pub fn new(daq_meta: DaqMeta, daq_raw: ArcArray2<f64>) -> Self {
        Self {
            daq_meta,
            daq_raw,
            interpolator: None,
        }
    }

    pub fn daq_meta(&self) -> &DaqMeta {
        &self.daq_meta
    }

    pub fn daq_raw(&self) -> ArcArray2<f64> {
        self.daq_raw.clone()
    }

    pub fn interpolator(&self) -> Option<&Interpolator> {
        self.interpolator.as_ref()
    }

    pub fn set_interpolator(&mut self, interpolator: Interpolator) -> Result<()> {
        if self.daq_meta.path != interpolator.meta().daq_path {
            bail!("daq path changed");
        }
        self.interpolator = Some(interpolator);

        Ok(())
    }
}

#[instrument(fields(daq_path = daq_path.as_ref().to_str().unwrap_or_default()))]
pub fn read_daq<P: AsRef<Path>>(daq_path: P) -> Result<(DaqMeta, Array2<f64>)> {
    let daq_path = daq_path.as_ref();
    let daq_raw = match daq_path
        .extension()
        .ok_or_else(|| anyhow!("invalid daq path: {:?}", daq_path))?
        .to_str()
    {
        Some("lvm") => read_daq_lvm(daq_path),
        Some("xlsx") => read_daq_excel(daq_path),
        _ => bail!("only .lvm and .xlsx are supported"),
    }?;

    let nrows = daq_raw.nrows();
    let ncols = daq_raw.ncols();
    let daq_meta = DaqMeta {
        path: daq_path.to_owned(),
        nrows,
        ncols,
    };

    Ok((daq_meta, daq_raw))
}

fn read_daq_lvm(daq_path: &Path) -> Result<Array2<f64>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(daq_path)
        .map_err(|e| anyhow!("failed to read daq from {:?}: {}", daq_path, e))?;

    let mut h = 0;
    let mut daq = Vec::new();
    for row in rdr.records() {
        h += 1;
        for v in &row? {
            daq.push(
                v.parse()
                    .map_err(|e| anyhow!("failed to read daq from {:?}: {}", daq_path, e))?,
            );
        }
    }
    let w = daq.len() / h;
    if h * w != daq.len() {
        bail!(
            "failed to read daq from {:?}: not all rows are equal in length",
            daq_path
        );
    }
    let daq = Array2::from_shape_vec((h, w), daq)?;

    Ok(daq)
}

fn read_daq_excel(daq_path: &Path) -> Result<Array2<f64>> {
    let mut excel: Xlsx<_> = open_workbook(daq_path)?;
    let sheet = excel
        .worksheet_range_at(0)
        .ok_or_else(|| anyhow!("no worksheet"))??;

    let mut daq = Array2::zeros(sheet.get_size());
    let mut daq_it = daq.iter_mut();
    for row in sheet.rows() {
        for v in row {
            if let Some(daq_v) = daq_it.next() {
                *daq_v = v
                    .get_float()
                    .ok_or_else(|| anyhow!("invalid daq: {:?}", daq_path))?;
            }
        }
    }

    Ok(daq)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     use crate::{
//         setting::{MockSettingStorage, StartIndex},
//         util,
//         video::Green2Metadata,
//     };
//
//     const DAQ_PATH: &str = "./tests/imp_20000_1.lvm";
//
//     #[tokio::test]
//     async fn test_full() {
//         util::log::init();
//
//         let daq_metadata = DaqMetadata {
//             path: PathBuf::from(DAQ_PATH),
//             nrows: 66666666,
//             ncols: 66666666,
//         };
//         let cal_num = 2000;
//
//         let mut mock = MockSettingStorage::new();
//         mock.expect_daq_metadata().return_once(|| Ok(daq_metadata));
//         mock.expect_set_daq_metadata().return_once(|_| Ok(()));
//         mock.expect_interpolation_method()
//             .return_once(|| Ok(InterpolationMethod::Horizontal));
//         mock.expect_green2_metadata().return_once(move || {
//             Ok(Green2Metadata {
//                 start_frame: 1,
//                 cal_num,
//                 area: (10, 10, 600, 800),
//                 video_path: PathBuf::from("FAKE"),
//             })
//         });
//         mock.expect_start_index().return_once(|| {
//             Ok(StartIndex {
//                 start_frame: 1,
//                 start_row: 20,
//             })
//         });
//         mock.expect_thermocouples().return_once(|| {
//             Ok(vec![
//                 Thermocouple {
//                     column_index: 1,
//                     position: (0, 0),
//                 },
//                 Thermocouple {
//                     column_index: 2,
//                     position: (0, 200),
//                 },
//                 Thermocouple {
//                     column_index: 3,
//                     position: (0, 500),
//                 },
//                 Thermocouple {
//                     column_index: 4,
//                     position: (0, 800),
//                 },
//             ])
//         });
//
//         let daq_manager = DaqManager::new(Arc::new(Mutex::new(mock)));
//
//         daq_manager.read_daq(None).await.unwrap();
//         {
//             let daq_data = daq_manager.inner.daq_data.lock().unwrap();
//             assert_eq!(daq_data.raw.as_ref().unwrap().dim(), (2589, 10));
//             assert!(daq_data.interpolator.is_none());
//         }
//
//         daq_manager.interpolate().await.unwrap();
//         let interpolator = daq_manager.interpolator().unwrap();
//         let temperature_distribution = interpolator.interpolate_single_frame(0).unwrap();
//         println!("{:?}", temperature_distribution);
//         let temperature_distribution = interpolator.interpolate_single_frame(cal_num - 1).unwrap();
//         println!("{:?}", temperature_distribution);
//         let temperature_history = interpolator.interpolate_single_point(10000);
//         println!("{:?}", temperature_history);
//     }
// }
