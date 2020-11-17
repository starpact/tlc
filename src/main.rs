// use std::fs::File;
// use std::io::{self, BufReader, Read, Write};
// use std::net::{TcpListener, TcpStream};
// use std::thread;

// fn main() -> io::Result<()> {
//     let listener = TcpListener::bind("127.0.0.1:2333")?;

//     for stream in listener.incoming() {
//         let stream = stream?;
//         thread::spawn(move || {
//             handle_connection(stream).unwrap_or(());
//         });
//     }

//     Ok(())
// }

// fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
//     let mut buf = [0; 1024];
//     stream.read(&mut buf)?;
//     println!("{}", String::from_utf8_lossy(&buf).lines().nth(0).unwrap());
//     stream.write("HTTP/1.1 200 OK\r\n\r\n".as_bytes())?;

//     let simple_get_head = b"GET / HTTP/1.1\r\n";
//     if buf.starts_with(simple_get_head) {
//         let file = File::open("./config/config_large.json")?;
//         let mut reader = BufReader::new(file);
//         let mut buf = Vec::new();
//         reader.read_to_end(&mut buf)?;
//         stream.write(&buf[..])?;
//     }

//     stream.flush()?;

//     Ok(())
// }

#[macro_use]
extern crate lazy_static;
use tlc::calculate::*;

const CONFIG_PATH: &str = "./config/config_large.json";

lazy_static! {
    static ref CONFIG_PARAS: io::ConfigParas = io::read_config(CONFIG_PATH).unwrap();
    static ref VIDEO_PATH: &'static str = CONFIG_PARAS.video_path.as_str();
    static ref EXCEL_PATH: &'static str = CONFIG_PARAS.excel_path.as_str();
    static ref START_FRAME: usize = CONFIG_PARAS.start_frame;
    static ref START_LINE: usize = CONFIG_PARAS.start_line;
    static ref FRAME_NUM: usize = CONFIG_PARAS.frame_num;
    static ref UPPER_LEFT_POS: (usize, usize) = CONFIG_PARAS.upper_left_pos;
    static ref REGION_SHAPE: (usize, usize) = CONFIG_PARAS.region_shape;
    static ref TEMP_COLUMN_NUM: &'static Vec<usize> = &CONFIG_PARAS.temp_column_num;
    static ref THERMOCOUPLE_POS: &'static Vec<(i32, i32)> = &CONFIG_PARAS.thermocouple_pos;
    static ref INTERP_METHOD: preprocess::InterpMethod = CONFIG_PARAS.interp_method;
    static ref FILTER_METHOD: preprocess::FilterMethod = CONFIG_PARAS.filter_method;
    static ref PEAK_TEMP: f64 = CONFIG_PARAS.peak_temp;
    static ref SOLID_THERMAL_CONDUCTIVITY: f64 = CONFIG_PARAS.solid_thermal_conductivity;
    static ref SOLID_THERMAL_DIFFUSIVITY: f64 = CONFIG_PARAS.solid_thermal_diffusivity;
    static ref H0: f64 = CONFIG_PARAS.h0;
    static ref MAX_ITER_NUM: usize = CONFIG_PARAS.max_iter_num;
}

fn main() {
    let video_record = (*START_FRAME, *FRAME_NUM, *VIDEO_PATH);
    let region_record = (*UPPER_LEFT_POS, *REGION_SHAPE);
    let v = io::read_video(video_record, region_record).unwrap().1;
    println!("{}", v);
}
