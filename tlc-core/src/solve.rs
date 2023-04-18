#[cfg(test)]
mod tests;

use std::f64::{consts::PI, NAN};

use libm::erfc;
use ndarray::{ArcArray2, Array2};
use packed_simd::{f64x4, Simd};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    daq::{Interpolator, InterpolatorId},
    util::impl_eq_always_false,
    video::{GmaxFrameIndexesId, VideoDataId},
};

/// All fields not NAN.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct PhysicalParam {
    pub gmax_temperature: f64,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,
}

impl Eq for PhysicalParam {}

impl std::hash::Hash for PhysicalParam {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.gmax_temperature.to_bits().hash(state);
        self.solid_thermal_conductivity.to_bits().hash(state);
        self.solid_thermal_diffusivity.to_bits().hash(state);
        self.characteristic_length.to_bits().hash(state);
        self.air_thermal_conductivity.to_bits().hash(state);
    }
}

#[salsa::interned]
pub(crate) struct PhysicalParamId {
    pub physical_param: PhysicalParam,
}

/// All fields not NAN.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum IterMethod {
    NewtonTangent { h0: f64, max_iter_num: usize },
    NewtonDown { h0: f64, max_iter_num: usize },
}

impl Eq for IterMethod {}

impl std::hash::Hash for IterMethod {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            IterMethod::NewtonTangent { h0, max_iter_num } => {
                state.write_u8(0);
                h0.to_bits().hash(state);
                max_iter_num.hash(state);
            }
            IterMethod::NewtonDown { h0, max_iter_num } => {
                state.write_u8(1);
                h0.to_bits().hash(state);
                max_iter_num.hash(state);
            }
        }
    }
}

#[salsa::interned]
pub(crate) struct IterMethodId {
    pub iter_method: IterMethod,
}

#[derive(Debug, Clone)]
pub struct Nu2(pub ArcArray2<f64>);

impl_eq_always_false!(Nu2);

#[salsa::tracked]
pub(crate) struct Nu2Id {
    pub nu2: Nu2,
}

#[salsa::tracked]
pub(crate) fn solve_nu(
    db: &dyn crate::Db,
    video_data_id: VideoDataId,
    gmax_frame_indexes_id: GmaxFrameIndexesId,
    interpolator_id: InterpolatorId,
    physical_param_id: PhysicalParamId,
    iteration_method_id: IterMethodId,
) -> Nu2Id {
    let frame_rate = video_data_id.frame_rate(db);
    let gmax_frame_indexes = gmax_frame_indexes_id.gmax_frame_indexes(db);
    let interpolator = interpolator_id.interpolater(db);
    let physical_param = physical_param_id.physical_param(db);
    let iteration_method = iteration_method_id.iter_method(db);

    let nu2 = solve(
        &gmax_frame_indexes,
        interpolator,
        physical_param,
        iteration_method,
        frame_rate,
    )
    .into_shared();
    Nu2Id::new(db, Nu2(nu2))
}

#[derive(Clone, Copy)]
struct PointData<'a> {
    gmax_frame_index: usize,
    temperatures: &'a [f64],
}

impl PointData<'_> {
    fn heat_transfer_equation(self, h: f64, dt: f64, k: f64, a: f64, tw: f64) -> (f64, f64) {
        let gmax_frame_index = self.gmax_frame_index;
        let temps = self.temperatures;

        // We use the average of first 4 values to calculate the initial temperature.
        const FIRST_FEW_TO_CAL_T0: usize = 4;
        let t0 = temps[..FIRST_FEW_TO_CAL_T0].iter().sum::<f64>() / FIRST_FEW_TO_CAL_T0 as f64;
        let (mut sum, mut diff_sum) = (f64x4::default(), f64x4::default());

        let mut frame_index = 0;
        while frame_index + f64x4::lanes() < gmax_frame_index {
            let delta_temp = unsafe {
                f64x4::from_slice_unaligned_unchecked(&temps[frame_index + 1..])
                    - f64x4::from_slice_unaligned_unchecked(&temps[frame_index..])
            };
            let at = a
                * dt
                * f64x4::new(
                    (gmax_frame_index - frame_index - 1) as f64,
                    (gmax_frame_index - frame_index - 2) as f64,
                    (gmax_frame_index - frame_index - 3) as f64,
                    (gmax_frame_index - frame_index - 4) as f64,
                );
            let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc_simd(h / k * at.sqrt());
            let step = (1.0 - exp_erfc) * delta_temp;
            let diff_step = -delta_temp
                * (2.0 * at.sqrt() / k / PI.sqrt() - (2.0 * at * h * exp_erfc) / k.powf(2.0));

            frame_index += f64x4::lanes();
            sum += step;
            diff_sum += diff_step;
        }

        let (mut sum, mut diff_sum) = (sum.sum(), diff_sum.sum());

        while frame_index < gmax_frame_index {
            let delta_temp =
                unsafe { temps.get_unchecked(frame_index + 1) - temps.get_unchecked(frame_index) };
            let at = a * dt * (gmax_frame_index - frame_index - 1) as f64;
            let exp_erfc = (h.powf(2.0) / k.powf(2.0) * at).exp() * erfc(h / k * at.sqrt());
            let step = (1.0 - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2.0 * at.sqrt() / k / PI.sqrt() - (2.0 * at * h * exp_erfc) / k.powf(2.0));

            frame_index += 1;
            sum += step;
            diff_sum += d_step;
        }

        (tw - t0 - sum, diff_sum)
    }
}

// Scalar version erfc from libm is much faster than SIMD version from sleef.
// See bench.
fn erfc_simd(arr: Simd<[f64; 4]>) -> Simd<[f64; 4]> {
    unsafe {
        f64x4::new(
            erfc(arr.extract_unchecked(0)),
            erfc(arr.extract_unchecked(1)),
            erfc(arr.extract_unchecked(2)),
            erfc(arr.extract_unchecked(3)),
        )
    }
}

fn newtow_tangent<EQ>(equation: EQ, h0: f64, max_iter_num: usize) -> impl Fn(PointData) -> f64
where
    EQ: Fn(PointData, f64) -> (f64, f64),
{
    move |point_data| {
        let mut h = h0;
        for _ in 0..max_iter_num {
            let (f, df) = equation(point_data, h);
            let next_h = h - f / df;
            if next_h.abs() > 10000. {
                return NAN;
            }
            if (next_h - h).abs() < 1e-3 {
                return next_h;
            }
            h = next_h;
        }
        h
    }
}

fn newtow_down<EQ>(equation: EQ, h0: f64, max_iter_num: usize) -> impl Fn(PointData) -> f64
where
    EQ: Fn(PointData, f64) -> (f64, f64),
{
    move |point_data| {
        let mut h = h0;
        let (mut f, mut df) = equation(point_data, h);
        for _ in 0..max_iter_num {
            let mut lambda = 1.0;
            loop {
                let next_h = h - lambda * f / df;
                if (next_h - h).abs() < 1e-3 {
                    return next_h;
                }
                let (next_f, next_df) = equation(point_data, next_h);
                if next_f.abs() < f.abs() {
                    h = next_h;
                    f = next_f;
                    df = next_df;
                    break;
                }
                lambda /= 2.0;
                if lambda < 1e-3 {
                    return NAN;
                }
            }
            if h.abs() > 10000.0 {
                return NAN;
            }
        }
        h
    }
}

#[instrument(skip(gmax_frame_indexes, interpolator))]
fn solve(
    gmax_frame_indexes: &[usize],
    interpolator: Interpolator,
    physical_param: PhysicalParam,
    iteration_method: IterMethod,
    frame_rate: usize,
) -> Array2<f64> {
    let dt = 1.0 / frame_rate as f64;
    let shape = interpolator.shape();
    let shape = (shape.0 as usize, shape.1 as usize);

    let PhysicalParam {
        gmax_temperature: tw,
        solid_thermal_conductivity: k,
        solid_thermal_diffusivity: a,
        characteristic_length,
        air_thermal_conductivity,
    } = physical_param;

    let equation =
        move |point_data: PointData, h| point_data.heat_transfer_equation(h, dt, k, a, tw);

    let h1 = match iteration_method {
        IterMethod::NewtonTangent { h0, max_iter_num } => {
            let f = newtow_tangent(equation, h0, max_iter_num);
            solve_core(f, gmax_frame_indexes, interpolator)
        }
        IterMethod::NewtonDown { h0, max_iter_num } => {
            let f = newtow_down(equation, h0, max_iter_num);
            solve_core(f, gmax_frame_indexes, interpolator)
        }
    };
    Array2::from_shape_vec(shape, h1).unwrap() * characteristic_length / air_thermal_conductivity
}

fn solve_core<F>(
    solve_single_point: F,
    gmax_frame_indexes: &[usize],
    interpolator: Interpolator,
) -> Vec<f64>
where
    F: Fn(PointData) -> f64 + Send + Sync + 'static,
{
    const FIRST_FEW_TO_CAL_T0: usize = 4;
    gmax_frame_indexes
        .par_iter()
        .enumerate()
        .map(|(point_index, &gmax_frame_index)| {
            if gmax_frame_index <= FIRST_FEW_TO_CAL_T0 {
                return NAN;
            }
            let temperatures = interpolator.interp_point(point_index);
            let temperatures = temperatures.as_slice().unwrap();
            let point_data = PointData {
                gmax_frame_index,
                temperatures,
            };
            solve_single_point(point_data)
        })
        .collect()
}
