use anyhow::{bail, Result};
use ndarray::ArcArray2;

use crate::{
    daq::{DaqData, DaqMeta, Interpolator},
    setting::SettingStorage,
};

use super::GlobalState;

impl<S: SettingStorage> GlobalState<S> {
    pub fn on_event_read_daq(&mut self, daq_meta: DaqMeta, daq_raw: ArcArray2<f64>) -> Result<()> {
        if self.setting_storage.daq_path()? != daq_meta.path {
            bail!("daq path changed");
        }

        self.daq_data = Some(DaqData::new(daq_meta, daq_raw));
        self.reconcile();

        Ok(())
    }

    pub fn on_event_interp(&mut self, interpolator: Interpolator) -> Result<()> {
        if &self.setting_storage.interp_meta()? != interpolator.meta() {
            bail!("interp meta changed, abort this result");
        }
        self.daq_data_mut()?.set_interpolator(interpolator)
    }

    fn reconcile(&mut self) {
        todo!()
    }
}
