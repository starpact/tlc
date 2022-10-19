mod interpolation;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Result};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::{prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tracing::{info, instrument};

use crate::{setting::SettingStorage, video::Green2Metadata};
pub use interpolation::{InterpolationMethod, Interpolator};

pub struct DaqManager<S: SettingStorage> {
    inner: Arc<DaqManagerInner<S>>,
}

impl<S: SettingStorage> Clone for DaqManager<S> {
    fn clone(&self) -> Self {
        DaqManager {
            inner: self.inner.clone(),
        }
    }
}

struct DaqManagerInner<S: SettingStorage> {
    setting_storage: Arc<Mutex<S>>,
    daq_data: Mutex<DaqData>,
}

struct DaqData {
    raw: Option<ArcArray2<f64>>,
    interpolator: Option<Interpolator>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DaqMetadata {
    pub path: PathBuf,
    pub nrows: usize,
    pub ncols: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Thermocouple {
    /// Column index of this thermocouple in the DAQ file.
    pub column_index: usize,
    /// Position of this thermocouple(y, x). Thermocouples
    /// may not be in the video area, so coordinate can be negative.
    pub position: (i32, i32),
}

impl<S: SettingStorage> DaqManager<S> {
    pub fn new(setting_storage: Arc<Mutex<S>>) -> Self {
        let inner = DaqManagerInner {
            setting_storage,
            daq_data: Mutex::new(DaqData {
                raw: None,
                interpolator: None,
            }),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn read_daq(&self, daq_path: Option<PathBuf>) -> Result<()> {
        let daq_manager = (*self).clone();
        spawn_blocking(move || daq_manager.inner.read_daq(daq_path)).await?
    }

    pub fn daq_raw(&self) -> Option<ArcArray2<f64>> {
        self.inner.daq_data.lock().unwrap().raw.clone()
    }

    pub async fn interpolate(&self) -> Result<()> {
        let daq_manager = self.clone();
        spawn_blocking(move || daq_manager.inner.interpolate()).await?
    }

    pub fn interpolator(&self) -> Option<Interpolator> {
        self.inner.daq_data.lock().unwrap().interpolator.clone()
    }
}

impl<S: SettingStorage> DaqManagerInner<S> {
    #[instrument(skip(self), err)]
    fn read_daq(&self, daq_path: Option<PathBuf>) -> Result<()> {
        let daq_path = match daq_path {
            Some(daq_path) => daq_path,
            None => self.setting_storage.lock().unwrap().daq_metadata()?.path,
        };

        let raw = match daq_path
            .extension()
            .ok_or_else(|| anyhow!("invalid daq path: {:?}", daq_path))?
            .to_str()
        {
            Some("lvm") => read_daq_lvm(&daq_path),
            Some("xlsx") => read_daq_excel(&daq_path),
            _ => bail!("only .lvm and .xlsx are supported"),
        }?;

        let nrows = raw.nrows();
        let ncols = raw.ncols();
        let daq_metadata = DaqMetadata {
            path: daq_path,
            nrows,
            ncols,
        };

        info!(?daq_metadata);
        let mut daq_data = self.daq_data.lock().unwrap();
        self.setting_storage
            .lock()
            .unwrap()
            .set_daq_metadata(&daq_metadata)?;
        daq_data.raw = Some(raw.into_shared());

        Ok(())
    }

    #[instrument(skip(self), err)]
    fn interpolate(&self) -> Result<()> {
        let mut daq_data = self.daq_data.lock().unwrap();
        let daq_raw = daq_data
            .raw
            .as_ref()
            .ok_or_else(|| anyhow!("daq not loaded yet"))?
            .view();
        let setting_storage = self.setting_storage.lock().unwrap();
        let interpolation_method = setting_storage.interpolation_method()?;
        let Green2Metadata { area, cal_num, .. } = setting_storage.green2_metadata()?;
        let start_row = setting_storage.start_index()?.start_row;
        let thermocouples = setting_storage.thermocouples()?;

        let mut temperature2 = Array2::zeros((thermocouples.len(), cal_num));
        daq_raw
            .rows()
            .into_iter()
            .skip(start_row)
            .take(cal_num)
            .zip(temperature2.columns_mut())
            .for_each(|(daq_row, mut col)| {
                thermocouples
                    .iter()
                    .zip(col.iter_mut())
                    .for_each(|(tc, t)| *t = daq_row[tc.column_index]);
            });

        let interpolator =
            Interpolator::new(temperature2, interpolation_method, area, &thermocouples);

        daq_data.interpolator = Some(interpolator);

        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        setting::{MockSettingStorage, StartIndex},
        util,
        video::Green2Metadata,
    };

    const DAQ_PATH: &str = "./tests/imp_20000_1.lvm";

    #[tokio::test]
    async fn test_full() {
        util::log::init();

        let daq_metadata = DaqMetadata {
            path: PathBuf::from(DAQ_PATH),
            nrows: 66666666,
            ncols: 66666666,
        };

        let cal_num = 2000;
        let mut mock = MockSettingStorage::new();
        mock.expect_daq_metadata().return_once(|| Ok(daq_metadata));
        mock.expect_set_daq_metadata().return_once(|_| Ok(()));
        mock.expect_interpolation_method()
            .return_once(|| Ok(InterpolationMethod::Horizontal));
        mock.expect_green2_metadata().return_once(move || {
            Ok(Green2Metadata {
                start_frame: 1,
                cal_num,
                area: (10, 10, 600, 800),
                video_path: PathBuf::from("FAKE"),
            })
        });
        mock.expect_start_index().return_once(|| {
            Ok(StartIndex {
                start_frame: 1,
                start_row: 20,
            })
        });
        mock.expect_thermocouples().return_once(|| {
            Ok(vec![
                Thermocouple {
                    column_index: 1,
                    position: (0, 0),
                },
                Thermocouple {
                    column_index: 2,
                    position: (0, 200),
                },
                Thermocouple {
                    column_index: 3,
                    position: (0, 500),
                },
                Thermocouple {
                    column_index: 4,
                    position: (0, 800),
                },
            ])
        });

        let daq_manager = DaqManager::new(Arc::new(Mutex::new(mock)));

        daq_manager.read_daq(None).await.unwrap();
        {
            let daq_data = daq_manager.inner.daq_data.lock().unwrap();
            assert_eq!(daq_data.raw.as_ref().unwrap().dim(), (2589, 10));
            assert!(daq_data.interpolator.is_none());
        }

        daq_manager.interpolate().await.unwrap();
        let interpolator = daq_manager.interpolator().unwrap();
        let temperature_distribution = interpolator.interpolate_single_frame(0).unwrap();
        println!("{:?}", temperature_distribution);
        let temperature_distribution = interpolator.interpolate_single_frame(cal_num - 1).unwrap();
        println!("{:?}", temperature_distribution);
        let temperature_history = interpolator.interpolate_single_point(10000);
        println!("{:?}", temperature_history);
    }
}
