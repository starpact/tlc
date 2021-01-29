use std::io::Read;
use tlc::calculate::*;

// const CFG_PATH: &str = "E:\\research\\EXP\\exp_20201206\\config\\imp_50000_2_up.json";
const CFG_PATH: &str = "./tmp/config/imp_40000_1_up.json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tlc_data = None;

    let mut stdin = std::io::stdin();
    let mut buf = [0; 3];

    loop {
        stdin.read(&mut buf).unwrap();
        let input = buf[0] - 48;
        match input {
            1 => {
                println!(
                    "tlc config: {:#?}",
                    tlc_data
                        .get_or_insert(TLCData::from_path(CFG_PATH)?)
                        .get_cfg()
                );
            }
            2 => tlc::cal_on_end(CFG_PATH)?,
            3 => {
                let start = std::time::Instant::now();
                tlc_data
                    .get_or_insert(TLCData::from_path(CFG_PATH)?)
                    .solve()?
                    .plot_nu()?
                    .save_config()?;
                // .save_nu()?;
                println!("{:?}", start.elapsed());
            }
            4 => {
                tlc_data
                    .get_or_insert(TLCData::from_path(CFG_PATH)?)
                    .set_video_path(
                        "E:\\research\\EXP\\exp_20201206\\imp\\videos\\imp_40000_1_up.avi"
                            .to_owned(),
                    )?;
            }
            _ => {}
        }
    }
}
