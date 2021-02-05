use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Instant;

use crate::calculate::error::TLCResult;
use crate::*;

const CFG_PATH: &str = "./cache/config.json";

pub fn init(rx: Receiver<u8>) -> TLCResult<()> {
    thread::spawn(move || -> TLCResult<()> {
        let mut tlc_data = None;

        loop {
            match rx.recv() {
                Ok(1) => {
                    println!(
                        "tlc config: {:#?}",
                        tlc_data
                            .get_or_insert(TLCData::from_path(CFG_PATH)?)
                            .get_config()
                    );
                }
                Ok(2) => cal_batch(CFG_PATH)?,
                Ok(3) => {
                    let start = Instant::now();
                    tlc_data
                        .get_or_insert(TLCData::from_path(CFG_PATH)?)
                        .solve()?
                        .plot_nu()?
                        .save_config()?
                        .save_nu()?;
                    println!("{:?}", start.elapsed());
                }
                Ok(4) => {
                    tlc_data
                        .get_or_insert(TLCData::from_path(CFG_PATH)?)
                        .set_video_path(
                            "D:\\research\\exp_20201206\\imp\\videos\\imp_40000_1_up.avi"
                                .to_owned(),
                        )?;
                }
                _ => {}
            }
        }
    });

    Ok(())
}
