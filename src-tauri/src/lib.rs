pub mod calculate;
pub mod view;

use std::path::Path;

use calculate::error;
use calculate::error::TLCResult;
use calculate::TLCData;

pub fn cal_batch<P: AsRef<Path>>(config_path: P) -> TLCResult<()> {
    let mut tlc_data = TLCData::from_path(config_path)?;

    tlc_data.solve()?.save_nu()?.plot_nu()?;

    println!("{}", tlc_data.get_nu_ave().ok_or(err!())?);
    Ok(())
}
