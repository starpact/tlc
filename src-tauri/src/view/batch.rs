use std::path::Path;

use crate::cal::{error::TLCResult, TLCData};

/// 批处理
#[allow(dead_code)]
pub fn cal_batch<P: AsRef<Path>>(config_path: P) -> TLCResult<()> {
    let nu_ave = TLCData::from_path(config_path)?
        .save_nu()?
        .plot_nu(None)?
        .get_nu_ave()?;
    println!("{}", nu_ave);

    Ok(())
}
