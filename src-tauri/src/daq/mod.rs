mod interpolation;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Result};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::{ArcArray2, Array2};
use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tracing::{info, instrument};

pub use interpolation::{InterpolationMethod, Temperature2};

use crate::setting::SettingStorage;

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
    temperature2: Option<Temperature2>,
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
                temperature2: None,
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

    pub fn daq_data(&self) -> Option<ArcArray2<f64>> {
        self.inner.daq_data.lock().unwrap().raw.clone()
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
        let fingerprint = "TODO".to_owned();
        let daq_metadata = DaqMetadata {
            path: daq_path,
            nrows,
            ncols,
        };

        info!(nrows, ncols, fingerprint);
        {
            let mut daq_data = self.daq_data.lock().unwrap();
            self.setting_storage
                .lock()
                .unwrap()
                .set_daq_metadata(&daq_metadata)?;
            daq_data.raw = Some(raw.into_shared());
        }

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
