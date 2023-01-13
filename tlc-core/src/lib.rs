#![feature(test)]
#![feature(array_windows)]
#![feature(assert_matches)]

mod daq;
mod main_loop;
mod post_processing;
pub mod request;
mod setting;
mod solve;
mod state;
mod util;
mod video;

pub use daq::{DaqMeta, InterpMethod, Thermocouple};
pub use main_loop::run;
pub use setting::StartIndex;
pub use solve::{IterationMethod, PhysicalParam};
pub use util::progress_bar::Progress;
pub use video::{FilterMethod, VideoMeta};
