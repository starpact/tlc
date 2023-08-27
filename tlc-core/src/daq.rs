mod interp;

use std::path::Path;

use anyhow::{anyhow, bail};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::{ArcArray2, Array2};
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub use interp::{InterpMethod, Interpolator};

#[derive(Debug, Serialize, Clone, Copy)]
pub struct DaqMeta {
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

#[instrument(fields(daq_path = ?daq_path.as_ref()), err)]
pub fn read_daq<P: AsRef<Path>>(daq_path: P) -> anyhow::Result<ArcArray2<f64>> {
    let daq_path = daq_path.as_ref();
    let daq_data = match daq_path
        .extension()
        .ok_or_else(|| anyhow!("invalid daq path: {daq_path:?}"))?
        .to_str()
    {
        Some("lvm") => read_daq_lvm(daq_path),
        Some("xlsx") => read_daq_excel(daq_path),
        _ => bail!("only .lvm and .xlsx are supported"),
    }?;
    Ok(daq_data.into_shared())
}

fn read_daq_lvm(daq_path: &Path) -> anyhow::Result<Array2<f64>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(daq_path)
        .map_err(|e| anyhow!("failed to read daq from {daq_path:?}: {e}"))?;

    let mut h = 0;
    let mut daq = Vec::new();
    for row in rdr.records() {
        h += 1;
        for v in &row? {
            daq.push(
                v.parse()
                    .map_err(|e| anyhow!("failed to read daq from {daq_path:?}: {e}"))?,
            );
        }
    }
    let w = daq.len() / h;
    if h * w != daq.len() {
        bail!("failed to read daq from {daq_path:?}: not all rows are equal in length");
    }
    let daq = Array2::from_shape_vec((h, w), daq)?;
    Ok(daq)
}

fn read_daq_excel(daq_path: &Path) -> anyhow::Result<Array2<f64>> {
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
                    .ok_or_else(|| anyhow!("invalid daq: {daq_path:?}"))?;
            }
        }
    }
    Ok(daq)
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;
    use crate::util::log;

    #[test]
    fn test_read_daq_lvm_and_xlsx() {
        log::init();
        assert_relative_eq!(
            read_daq("./testdata/imp_20000_1.lvm").unwrap(),
            read_daq("./testdata/imp_20000_1.xlsx").unwrap()
        );
    }

    #[test]
    fn test_read_daq_unsupported_extension() {
        assert!(read_daq("./testdata/imp_20000_1.csv").is_err());
    }
}
