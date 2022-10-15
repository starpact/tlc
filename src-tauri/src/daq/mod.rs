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
use tracing::instrument;

pub use interpolation::{InterpolationMethod, Temperature2};

use crate::setting::SettingStorage;

#[derive(Clone)]
pub struct DaqManager<S: SettingStorage> {
    inner: Arc<Mutex<DaqManagerInner<S>>>,
}

struct DaqManagerInner<S: SettingStorage> {
    setting_storage: Arc<Mutex<S>>,
    daq_data: Option<ArcArray2<f64>>,
    temperature2: Option<Temperature2>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DaqMetadata {
    pub path: PathBuf,
    pub nrows: usize,
    pub ncols: usize,
    pub fingerprint: String,
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
            daq_data: None,
            temperature2: None,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    #[instrument(skip(self))]
    pub async fn read_daq(&self, daq_path: PathBuf) -> Result<()> {
        let daq_manager = self.inner.clone();
        spawn_blocking(move || {
            let daq_data = match daq_path
                .extension()
                .ok_or_else(|| anyhow!("invalid daq path: {:?}", daq_path))?
                .to_str()
            {
                Some("lvm") => read_daq_lvm(&daq_path),
                Some("xlsx") => read_daq_excel(&daq_path),
                _ => bail!("only .lvm and .xlsx are supported"),
            }?;

            let daq_metadata = DaqMetadata {
                path: daq_path.to_owned(),
                nrows: daq_data.nrows(),
                ncols: daq_data.ncols(),
                fingerprint: "TODO".to_owned(),
            };

            {
                let mut daq_manager = daq_manager.lock().unwrap();
                daq_manager
                    .setting_storage
                    .lock()
                    .unwrap()
                    .set_daq_metadata(daq_metadata)?;
                daq_manager.daq_data = Some(daq_data.into_shared());
            }

            Ok(())
        })
        .await?
    }

    pub fn daq_data(&self) -> Option<ArcArray2<f64>> {
        self.inner.lock().unwrap().daq_data.clone()
    }
}

fn read_daq_lvm(daq_path: &Path) -> Result<Array2<f64>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(&daq_path)
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
    let mut excel: Xlsx<_> = open_workbook(&daq_path)?;
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
