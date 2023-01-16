use std::path::PathBuf;

use crate::util::log;

#[test]
fn test_read_daq() {
    log::init();

    let mut db = crate::Database::default();
    assert!(db.get_daq_path().is_none());
    let daq_path = PathBuf::from("./testdata/imp_20000_1.lvm");
    db.set_daq_path(daq_path.clone());
    assert_eq!(db.get_daq_path().unwrap(), daq_path);

    println!("first");
    db.get_daq_data().unwrap();

    println!("no update");
    db.get_daq_data().unwrap();

    println!("set same");
    db.set_daq_path(PathBuf::from("./testdata/imp_20000_1.lvm"));
    db.get_daq_data().unwrap();

    println!("set different");
    db.set_daq_path(PathBuf::from("./testdata/imp_20000_1.xlsx"));
    db.get_daq_data().unwrap();
}
