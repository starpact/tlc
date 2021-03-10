use std::path::Path;

use crate::cal::{error::TLCResult, TLCData};

/// 批处理
#[allow(dead_code)]
pub fn cal_batch<P: AsRef<Path>>(config_path: P) -> TLCResult<()> {
    TLCData::from_path(config_path)?
        .save_nu()?;
        // .get_nu_img(None)?;

    Ok(())
}
