#![cfg_attr(test, feature(test, array_windows))]
#![allow(clippy::too_many_arguments)]

mod daq;
mod db;
mod postproc;
mod solve;
mod state;
mod util;
mod video;

#[cfg(test)]
mod tests;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::bail;
pub use daq::{InterpMethod, Thermocouple};
use db::Database;
use ndarray::{ArcArray2, Array2};
use serde::Serialize;
pub use solve::{IterMethod, PhysicalParam};
pub use video::FilterMethod;

pub fn init() {
    video::init();
    util::log::init();
}

#[derive(Debug, Serialize)]
pub struct NuData {
    pub nu2: ArcArray2<f64>,
    pub nu_nan_mean: f64,
}

#[derive(Default)]
pub struct State {
    inner: Arc<Mutex<Database>>,
}

impl State {
    pub fn get_name(&self) -> anyhow::Result<String> {
        Ok(self.inner.lock().unwrap().name()?.to_owned())
    }

    pub fn set_name(&self, name: String) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_name(name)
    }

    pub fn get_save_root_dir(&self) -> anyhow::Result<PathBuf> {
        Ok(self.inner.lock().unwrap().save_root_dir()?.to_owned())
    }

    pub fn set_save_root_dir(&self, save_root_dir: PathBuf) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_save_root_dir(save_root_dir)
    }

    pub fn get_video_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.inner.lock().unwrap().video_path()?.to_owned())
    }

    pub fn set_video_path(&self, video_path: PathBuf) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_video_path(video_path)
    }

    pub fn get_video_nframes(&self) -> anyhow::Result<usize> {
        self.inner.lock().unwrap().video_nframes()
    }

    pub fn get_video_frame_rate(&self) -> anyhow::Result<usize> {
        self.inner.lock().unwrap().video_frame_rate()
    }

    pub fn get_video_shape(&self) -> anyhow::Result<(u32, u32)> {
        self.inner.lock().unwrap().video_shape()
    }

    pub async fn decode_frame_base64(&self, frame_index: usize) -> anyhow::Result<String> {
        let video_data = self.inner.lock().unwrap().video_data()?;
        let nframes = video_data.nframes();
        if frame_index >= nframes {
            bail!("frame_index({frame_index}) exceeds nframes({nframes})");
        }
        video_data.decode_frame_base64(frame_index).await
    }

    pub fn get_daq_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.inner.lock().unwrap().daq_path()?.to_owned())
    }

    pub fn set_daq_path(&self, daq_path: PathBuf) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_daq_path(daq_path)
    }

    pub fn get_daq_data(&self) -> anyhow::Result<ArcArray2<f64>> {
        self.inner.lock().unwrap().get_daq_data()
    }

    pub fn synchronize_video_and_daq(
        &self,
        start_frame: usize,
        start_row: usize,
    ) -> anyhow::Result<()> {
        self.inner
            .lock()
            .unwrap()
            .synchronize_video_and_daq(start_frame, start_row)
    }

    pub fn get_start_frame(&self) -> anyhow::Result<usize> {
        self.inner.lock().unwrap().start_frame()
    }

    pub fn set_start_frame(&self, start_frame: usize) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_start_frame(start_frame)
    }

    pub fn get_start_row(&self) -> anyhow::Result<usize> {
        self.inner.lock().unwrap().start_row()
    }

    pub fn set_start_row(&self, start_frame: usize) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_start_row(start_frame)
    }

    pub fn get_area(&self) -> anyhow::Result<(u32, u32, u32, u32)> {
        self.inner.lock().unwrap().area()
    }

    pub fn set_area(&self, area: (u32, u32, u32, u32)) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_area(area)
    }

    pub fn get_filter_method(&self) -> anyhow::Result<FilterMethod> {
        self.inner.lock().unwrap().filter_method()
    }

    pub fn set_filter_method(&self, filter_method: FilterMethod) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_filter_method(filter_method)
    }

    pub fn filter_point(&self, point: (usize, usize)) -> anyhow::Result<Vec<u8>> {
        // TODO: async
        self.inner.lock().unwrap().filter_point(point)
    }

    pub fn get_thermocouples(&self) -> anyhow::Result<Box<[Thermocouple]>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .thermocouples()?
            .to_owned()
            .into_boxed_slice())
    }

    pub fn set_thermocouples(&self, thermocouples: Box<[Thermocouple]>) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_thermocouples(thermocouples)
    }

    pub fn get_interp_method(&self) -> anyhow::Result<InterpMethod> {
        self.inner.lock().unwrap().interp_method()
    }

    pub fn set_interp_method(&self, interp_method: InterpMethod) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_interp_method(interp_method)
    }

    pub fn interp_frame(&self, frame_index: usize) -> anyhow::Result<Array2<f64>> {
        // TODO: asnyc
        self.inner.lock().unwrap().interp_frame(frame_index)
    }

    pub fn get_iter_method(&self) -> anyhow::Result<IterMethod> {
        self.inner.lock().unwrap().iter_method()
    }

    pub fn set_iter_method(&self, iter_method: IterMethod) -> anyhow::Result<()> {
        self.inner.lock().unwrap().set_iter_method(iter_method)
    }

    pub fn get_physical_param(&self) -> anyhow::Result<PhysicalParam> {
        self.inner.lock().unwrap().physical_param()
    }

    pub fn set_physical_param(&self, physical_param: PhysicalParam) -> anyhow::Result<()> {
        self.inner
            .lock()
            .unwrap()
            .set_physical_param(physical_param)
    }

    pub fn get_nu_data(&self) -> anyhow::Result<NuData> {
        let nu2 = self.inner.lock().unwrap().nu2()?;
        let nu_nan_mean = postproc::nan_mean(nu2.view());
        Ok(NuData { nu2, nu_nan_mean })
    }

    pub fn get_nu_plot(&self, trunc: Option<(f64, f64)>) -> anyhow::Result<String> {
        self.inner.lock().unwrap().nu_plot(trunc)
    }

    pub fn save_data(&self) -> anyhow::Result<()> {
        self.inner.lock().unwrap().save_data()
    }
}
