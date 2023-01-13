use std::path::Path;

use anyhow::{anyhow, bail, Result};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::Array2;
use tracing::instrument;

use super::DaqMeta;

#[instrument(fields(daq_path = ?daq_path.as_ref()), err)]
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
    let daq_meta = DaqMeta { nrows, ncols };

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

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;
    use crate::util;

    const DAQ_PATH_LVM: &str = "./testdata/imp_20000_1.lvm";
    const DAQ_PATH_XLSX: &str = "./testdata/imp_20000_1.xlsx";
    const DAQ_PATH_UNSUPPORTED: &str = "./testdata/imp_20000_1.csv";

    #[test]
    fn test_read_daq_lvm_and_xlsx() {
        util::log::init();

        let (daq_meta, daq_raw_lvm) = read_daq(DAQ_PATH_LVM).unwrap();
        assert_eq!(
            daq_meta,
            DaqMeta {
                nrows: 2589,
                ncols: 10,
            }
        );

        let (daq_meta, daq_raw_xlsx) = read_daq(DAQ_PATH_XLSX).unwrap();
        assert_eq!(
            daq_meta,
            DaqMeta {
                nrows: 2589,
                ncols: 10,
            }
        );

        assert_relative_eq!(daq_raw_lvm, daq_raw_xlsx);
    }

    #[test]
    fn test_read_daq_unsupported_extension() {
        assert!(read_daq(DAQ_PATH_UNSUPPORTED).is_err());
    }
}
