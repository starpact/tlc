use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::Array2;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::util;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DaqMetadata {
    /// Path of TLC data acquisition file.
    pub path: PathBuf,
    /// Total raws of DAQ data.
    #[serde(skip_deserializing)]
    pub nrows: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Thermocouple {
    /// Column index of this thermocouple in the DAQ file.
    column_index: usize,
    /// Position of this thermocouple(y, x). Thermocouples
    /// may not be in the video area, so coordinate can be negative.
    position: (i32, i32),
}

pub fn read_daq<P: AsRef<Path>>(daq_path: P) -> Result<Array2<f64>> {
    info!("daq_path: {:?}", daq_path.as_ref());
    let mut timer = util::timing::start("reading daq");

    let daq = match daq_path
        .as_ref()
        .extension()
        .ok_or_else(|| anyhow!("invalid daq path: {:?}", daq_path.as_ref()))?
        .to_str()
    {
        Some("lvm") => read_daq_lvm(&daq_path),
        Some("xlsx") => read_daq_excel(&daq_path),
        _ => bail!("only .lvm and .xlsx are supported"),
    }?;

    timer.finish();
    debug!("daq:\n{:?}", daq);

    Ok(daq)
}

fn read_daq_lvm<P: AsRef<Path>>(daq_path: P) -> Result<Array2<f64>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(&daq_path)
        .map_err(|e| anyhow!("failed to read daq from {:?}: {}", daq_path.as_ref(), e))?;

    let mut h = 0;
    let mut daq = Vec::new();
    for row in rdr.records() {
        h += 1;
        for v in &row? {
            daq.push(
                v.parse().map_err(|e| {
                    anyhow!("failed to read daq from {:?}: {}", daq_path.as_ref(), e)
                })?,
            );
        }
    }
    let w = daq.len() / h;
    if h * w != daq.len() {
        bail!(
            "failed to read daq from {:?}: not all rows are equal in length",
            daq_path.as_ref()
        );
    }
    let daq = Array2::from_shape_vec((h, w), daq)?;

    Ok(daq)
}

fn read_daq_excel<P: AsRef<Path>>(daq_path: P) -> Result<Array2<f64>> {
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
                    .ok_or_else(|| anyhow!("invalid daq: {:?}", daq_path.as_ref()))?;
            }
        }
    }

    Ok(daq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_daq() {
        util::log::init();

        assert_eq!(
            read_daq("/home/yhj/Documents/2021yhj/EXP/imp/daq/imp_20000_1.lvm").unwrap(),
            read_daq("/home/yhj/Documents/2021yhj/EXP/imp/daq/imp_20000_1.xlsx").unwrap()
        );
    }
}
