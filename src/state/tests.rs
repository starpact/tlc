use std::path::PathBuf;

use super::*;
use crate::{
    daq::{InterpMethod, Thermocouple},
    solve::{IterMethod, PhysicalParam},
    util,
    video::{tests::*, FilterMethod},
};

#[ignore]
#[tokio::test]
async fn test_whole_process_step_by_step() {
    util::log::init();
    let mut db = Database::default();

    // db.set_name("test_case_1".to_owned()).unwrap();
    std::fs::create_dir_all("/tmp/tlc").unwrap();
    db.set_save_root_dir(PathBuf::from("/tmp/tlc")).unwrap();

    let video_path = PathBuf::from(VIDEO_PATH_REAL);
    db.set_video_path(video_path.clone()).unwrap();
    assert_eq!(db.video_path().unwrap(), &video_path);
    assert_eq!(db.video_shape().unwrap(), (1024, 1280));
    assert_eq!(db.video_frame_rate().unwrap(), 25);
    assert_eq!(db.video_nframes().unwrap(), 2444);

    let daq_path = PathBuf::from("/home/yhj/Downloads/EXP/imp/daq/imp_20000_1.lvm");
    db.set_daq_path(daq_path.clone()).unwrap();
    assert_eq!(db.daq_path().unwrap(), daq_path);
    assert_eq!(db.daq_data().unwrap().dim(), (2589, 10));

    db.set_start_frame(81).unwrap_err();
    db.set_start_row(151).unwrap_err();
    db.synchronize_video_and_daq(81, 150).unwrap();
    assert_eq!(db.start_frame().unwrap(), 81);
    assert_eq!(db.start_row().unwrap(), 150);
    db.set_start_frame(80).unwrap();
    assert_eq!(db.start_frame().unwrap(), 80);
    assert_eq!(db.start_row().unwrap(), 149);
    db.set_start_row(150).unwrap();
    assert_eq!(db.start_frame().unwrap(), 81);
    assert_eq!(db.start_row().unwrap(), 150);

    db.set_area((660, 20, 340, 1248)).unwrap();
    assert_eq!(db.area().unwrap(), (660, 20, 340, 1248));

    db.set_filter_method(_filter_method()).unwrap();
    assert_eq!(db.filter_method().unwrap(), _filter_method());
    db.filter_point((100, 100)).unwrap();
    db.filter_point((300, 500)).unwrap();

    db.set_thermocouples(_thermocouples()).unwrap();
    assert_eq!(db.thermocouples().unwrap(), &*_thermocouples());

    db.set_interp_method(InterpMethod::Horizontal).unwrap();
    assert_eq!(db.interp_method().unwrap(), InterpMethod::Horizontal);

    db.set_physical_param(_physical_param()).unwrap();
    assert_eq!(db.physical_param().unwrap(), _physical_param());

    db.set_iter_method(_iteration_method()).unwrap();
    assert_eq!(db.iter_method().unwrap(), _iteration_method());

    db.nu2().unwrap();
}

#[ignore]
#[test]
fn test_all_onetime_auto() {
    util::log::init();
    let mut db = Database::default();
    // db.set_name("test_case_2".to_owned()).unwrap();
    std::fs::create_dir_all("/tmp/tlc").unwrap();
    db.set_save_root_dir(PathBuf::from("/tmp/tlc")).unwrap();
    db.set_video_path(PathBuf::from(VIDEO_PATH_REAL)).unwrap();
    db.set_daq_path(PathBuf::from(
        "/home/yhj/Downloads/EXP/imp/daq/imp_20000_1.lvm",
    ))
    .unwrap();
    db.set_start_frame(81).unwrap_err();
    db.set_start_row(151).unwrap_err();
    db.synchronize_video_and_daq(81, 150).unwrap();
    db.set_start_frame(80).unwrap();
    db.set_start_row(150).unwrap();
    db.set_area((660, 20, 340, 1248)).unwrap();
    db.set_filter_method(_filter_method()).unwrap();
    db.set_thermocouples(_thermocouples()).unwrap();
    db.set_interp_method(InterpMethod::Horizontal).unwrap();
    db.set_physical_param(_physical_param()).unwrap();
    db.set_iter_method(_iteration_method()).unwrap();
    db.nu2().unwrap();
    db.nu2().unwrap();
    db.nu_plot(None).unwrap();
    db.save_data().unwrap();
}

fn _filter_method() -> FilterMethod {
    FilterMethod::Wavelet {
        threshold_ratio: 0.8,
    }
}

fn _thermocouples() -> Box<[Thermocouple]> {
    vec![
        Thermocouple {
            column_index: 1,
            position: (0, 166),
        },
        Thermocouple {
            column_index: 2,
            position: (0, 355),
        },
        Thermocouple {
            column_index: 3,
            position: (0, 543),
        },
        Thermocouple {
            column_index: 4,
            position: (0, 731),
        },
        Thermocouple {
            column_index: 1,
            position: (0, 922),
        },
        Thermocouple {
            column_index: 6,
            position: (0, 1116),
        },
    ]
    .into_boxed_slice()
}

fn _physical_param() -> PhysicalParam {
    PhysicalParam {
        gmax_temperature: 35.48,
        solid_thermal_conductivity: 0.19,
        solid_thermal_diffusivity: 1.091e-7,
        characteristic_length: 0.015,
        air_thermal_conductivity: 0.0276,
    }
}

fn _iteration_method() -> IterMethod {
    IterMethod::NewtonDown {
        h0: 50.0,
        max_iter_num: 10,
    }
}
