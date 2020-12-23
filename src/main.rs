use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

const CFG_DIR: &str = "D:\\research\\EXP\\exp_20201206\\config";

fn main() -> Result<(), Box<dyn Error>> {
    let mut bm = BTreeMap::new();

    let cfg_dir = Path::new(CFG_DIR);
    for f in cfg_dir.read_dir()? {
        let file_name = f?.file_name();
        if filter(file_name.to_str().ok_or("")?) {
            let p = cfg_dir.to_owned().join(&file_name);
            println!("\n================\n{:?}...", p);
            let nu = tlc::cal(p)?;
            bm.insert(file_name, format!("{:.2}", nu));
        }
    }
    println!("{:#?}", bm);

    Ok(())
}

fn filter(file_name: &str) -> bool {
    match file_name {
        // "imp_40000_1_up.json" | "imp_40000_2_up.json" => true,
        "imp_50000_2_up.json" => true,
        _ => false,
    }
}
