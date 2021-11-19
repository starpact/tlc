use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use calamine::{open_workbook, Reader, Xlsx};
use ndarray::Array2;
use tracing::debug;

use crate::util::timing;

pub fn read_daq<P: AsRef<Path>>(daq_path: P) -> Result<Array2<f64>> {
    let _timing = timing::start("reading daq");
    debug!("{:?}", daq_path.as_ref());

    let daq = match daq_path
        .as_ref()
        .extension()
        .ok_or(anyhow!("invalid daq path: {:?}", daq_path.as_ref()))?
        .to_str()
    {
        Some("lvm") => read_daq_from_lvm(&daq_path),
        Some("xlsx") => read_daq_from_excel(&daq_path),
        _ => bail!("only .lvm and .xlsx are supported"),
    }?;

    debug!("\n{:?}", daq);

    Ok(daq)
}

fn read_daq_from_lvm<P: AsRef<Path>>(daq_path: P) -> Result<Array2<f64>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .from_path(&daq_path)
        .with_context(|| format!("invalid daq path: {:?}", daq_path.as_ref()))?;

    let mut h = 0;
    let mut daq = Vec::new();
    for row in rdr.records() {
        h += 1;
        for v in &row? {
            daq.push(
                v.parse()
                    .with_context(|| format!("invalid daq: {:?}", daq_path.as_ref()))?,
            );
        }
    }
    let w = daq.len() / h;
    if h * w != daq.len() {
        bail!(
            "invalid daq: {:?}, not all rows are equal in length",
            daq_path.as_ref()
        );
    }
    let daq = Array2::from_shape_vec((h, w), daq)?;

    Ok(daq)
}

fn read_daq_from_excel<P: AsRef<Path>>(daq_path: P) -> Result<Array2<f64>> {
    let mut excel: Xlsx<_> = open_workbook(&daq_path)?;
    let sheet = excel
        .worksheet_range_at(0)
        .ok_or(anyhow!("no worksheet"))??;

    let mut daq = Array2::zeros(sheet.get_size());
    let mut daq_it = daq.iter_mut();
    for row in sheet.rows() {
        for v in row {
            if let Some(daq_v) = daq_it.next() {
                *daq_v = v
                    .get_float()
                    .ok_or(anyhow!("invalid daq: {:?}", daq_path.as_ref()))?;
            }
        }
    }

    Ok(daq)
}
