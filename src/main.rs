use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use tlc::calculate::error::TLCError;

const CFG_DIR: &str = "D:\\research\\EXP\\exp_20201206\\config";

fn main() -> Result<(), Box<dyn Error>> {
    let mut bm = BTreeMap::new();

    let cfg_dir = Path::new(CFG_DIR);
    for f in cfg_dir.read_dir()? {
        let file_name = f?.file_name();
        if filter(file_name.to_str().ok_or("")?) {
            let p = cfg_dir.to_owned().join(&file_name);
            println!("\n================\n{:?}...", p);
            let nu = match tlc::cal(p) {
                Ok(nu) => nu,
                Err(TLCError::ConfigFormatError(e)) => panic!(e),
                Err(TLCError::CreateDirFailedError(e)) => panic!(e),
                Err(TLCError::VideoError(e)) => panic!(e),
                Err(TLCError::ConfigIOError(s))
                | Err(TLCError::DAQIOError(s))
                | Err(TLCError::VideoIOError(s))
                | Err(TLCError::NuIOError(s))
                | Err(TLCError::PlotError(s))
                | Err(TLCError::UnKnown(s)) => panic!(s),
            };
            bm.insert(file_name, format!("{:.2}", nu));
        }
    }
    println!("{:#?}", bm);

    Ok(())
}

fn filter(file_name: &str) -> bool {
    match file_name {
        // "imp_50000_2_up.json" => true,
        "imp_40000_1_up.json" => true,
        "imp_40000_2_up.json" => true,
        _ => false,
    }
}
