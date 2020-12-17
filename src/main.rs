use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use tlc::cal;

const CFG_DIR: &str = "E:\\research\\EXP\\exp_20201206\\config";

fn main() -> Result<(), Box<dyn Error>> {
    let mut bm = BTreeMap::new();

    let cfg_dir = Path::new(CFG_DIR);
    for f in cfg_dir.read_dir()? {
        let file_name = f?.file_name();
        if filter(file_name.to_str().ok_or("")?) {
            let p = cfg_dir.to_owned().join(&file_name);
            println!("\n================\n{:?}...", p);
            let nu = cal(p)?;
            bm.insert(file_name, format!("{:.2}", nu));
        }
    }
    println!("{:#?}", bm);

    Ok(())
}

fn filter(file_name: &str) -> bool {
    match file_name {
        "deprecated"
        | "imp_20000_1_up.json"
        | "imp_20000_1_down.json"
        | "imp_30000_2_up.json"
        | "imp_30000_2_down.json"
        | "imp_40000_1_up.json"
        | "imp_40000_1_down.json"
        | "imp_50000_2_up.json"
        | "imp_50000_2_down.json"
        | "sta_20000_1_up.json"
        | "sta_20000_1_down.json"
        | "sta_30000_2_up.json"
        | "sta_30000_2_down.json"
        | "sta_40000_2_up.json"
        | "sta_40000_2_down.json"
        | "sta_50000_2_up.json"
        | "sta_50000_2_down.json"
        | "rib_20000_2_up.json"
        | "rib_20000_2_down.json"
        | "rib_30000_2_up.json"
        | "rib_30000_2_down.json"
        | "rib_40000_2_up.json"
        | "rib_40000_2_down.json"
        // | "rib_50000_2_up.json"
        | "rib_50000_2_down.json"
        | "fuck" => false,
        _ => true,
    }
}
