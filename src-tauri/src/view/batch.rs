use std::path::Path;

use crate::calculate::{error::TLCResult, TLCData};

/// 批处理
#[allow(dead_code)]
pub fn cal_batch<P: AsRef<Path>>(config_path: P) -> TLCResult<()> {
    let mut tlc_data = TLCData::from_path(config_path)?;

    tlc_data.solve()?.save_nu()?.plot_nu()?;

    println!("{}", tlc_data.get_nu_ave().unwrap());
    Ok(())
}
