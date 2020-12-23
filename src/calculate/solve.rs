use ndarray::prelude::*;
use ndarray::Zip;
use std::error::Error;
use std::f64::{consts::PI, NAN};

/// there is no erfc() in std, so use erfc() from libc
mod cmath {
    use libc::c_double;
    extern "C" {
        pub fn erfc(x: c_double) -> c_double;
    }
}

fn erfc(x: f64) -> f64 {
    unsafe { cmath::erfc(x) }
}

/// *struct that stores necessary information for solving the equation*
struct PointData<'a> {
    solid_thermal_conductivity: f64,
    solid_thermal_diffusivity: f64,
    dt: f64,
    peak_frame: usize,
    temps: ArrayView1<'a, f64>,
}

impl PointData<'_> {
    /// *semi-infinite plate heat transfer equation of each pixel*
    /// ### Return:
    /// equation and its derivative
    fn thermal_equation(&self, h: f64) -> (f64, f64) {
        let (k, a, dt, temps, peak_frame) = (
            self.solid_thermal_conductivity,
            self.solid_thermal_diffusivity,
            self.dt,
            self.temps,
            self.peak_frame,
        );

        let (sum, diff_sum) = (1..peak_frame).fold((0., 0.), |(f, df), i| {
            let delta_temp = unsafe { temps.uget(i) - temps.uget(i - 1) };
            let at = a * dt * (peak_frame - i) as f64;
            let exp_erfc = (h.powf(2.) / k.powf(2.) * at).exp() * erfc(h / k * at.sqrt());
            let step = (1. - exp_erfc) * delta_temp;
            let d_step = -delta_temp
                * (2. * at.sqrt() / k / PI.sqrt() - (2. * at * h * exp_erfc) / k.powf(2.));

            (f + step, df + d_step)
        });

        let t0 = self.temps.slice(s![..4]).mean().unwrap();

        (t0 + sum, diff_sum)
    }
}

trait PointSolver {
    fn helper(&self, h: f64) -> (f64, f64);

    fn newton_tangent(&self, h0: f64, max_iter_num: usize) -> f64 {
        let mut h = h0;
        for _ in 0..max_iter_num {
            let (f, df) = self.helper(h);
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

    fn newton_down(&self, h0: f64, max_iter_num: usize) -> f64 {
        let mut h = h0;
        let (mut f, mut df) = self.helper(h);

        for _ in 0..max_iter_num {
            let mut lambda = 1.;
            loop {
                let next_h = h - lambda * f / df;
                let (next_f, next_df) = self.helper(next_h);
                if next_f.abs() < f.abs() {
                    if (next_h - h).abs() < 1e-3 {
                        return next_h;
                    }
                    h = next_h;
                    f = next_f;
                    df = next_df;
                    break;
                }
                lambda /= 2.;
                if lambda < 1e-3 {
                    return NAN;
                }
            }
            if h > 10000. {
                return NAN;
            }
        }

        h
    }
}

struct SinglePointSolver<'a> {
    data: PointData<'a>,
    h0: f64,
    peak_temp: f64,
    max_iter_num: usize,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
}

impl PointSolver for SinglePointSolver<'_> {
    fn helper(&self, h: f64) -> (f64, f64) {
        let (sum, diff_sum) = self.data.thermal_equation(h);
        (self.peak_temp - sum, diff_sum)
    }
}

impl SinglePointSolver<'_> {
    fn solve_nu(&self) -> f64 {
        self.newton_tangent(self.h0, self.max_iter_num) * self.characteristic_length
            / self.air_thermal_conductivity
    }
}

struct DoublePointSolver<'a> {
    data1: PointData<'a>,
    data2: PointData<'a>,
    h0: f64,
    max_iter_num: usize,
    characteristic_length: f64,
    air_thermal_conductivity: f64,
}

impl PointSolver for DoublePointSolver<'_> {
    fn helper(&self, h: f64) -> (f64, f64) {
        let (sum1, diff_sum1) = self.data1.thermal_equation(h);
        let (sum2, diff_sum2) = self.data2.thermal_equation(h);
        (sum2 - sum1, diff_sum1 - diff_sum2)
    }
}

impl DoublePointSolver<'_> {
    fn solve_nu(&self) -> f64 {
        self.newton_down(self.h0, self.max_iter_num) * self.characteristic_length
            / self.air_thermal_conductivity
    }
}

pub struct CaseData {
    pub peak_frames: Array1<usize>,
    pub interp_temps: Array2<f64>,
    pub query_index: Array1<usize>,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,
    pub dt: f64,
    pub peak_temp: f64,
    pub h0: f64,
    pub max_iter_num: usize,
}

impl CaseData {
    pub fn solve(&self) -> Array1<f64> {
        let mut nus = Array1::zeros(self.query_index.len());

        Zip::from(&self.peak_frames)
            .and(&self.query_index)
            .and(&mut nus)
            .par_apply(|&peak_frame, &index, nu| {
                let data = PointData {
                    solid_thermal_conductivity: self.solid_thermal_conductivity,
                    solid_thermal_diffusivity: self.solid_thermal_diffusivity,
                    dt: self.dt,
                    peak_frame,
                    temps: self.interp_temps.column(index),
                };
                let single_pointer_solver = SinglePointSolver {
                    data,
                    h0: self.h0,
                    peak_temp: self.peak_temp,
                    max_iter_num: self.max_iter_num,
                    characteristic_length: self.characteristic_length,
                    air_thermal_conductivity: self.air_thermal_conductivity,
                };
                *nu = single_pointer_solver.solve_nu();
            });

        nus
    }
}

pub struct DoubleCaseData {
    pub peak_frames1: Array1<usize>,
    pub peak_frames2: Array1<usize>,
    pub interp_temps1: Array2<f64>,
    pub interp_temps2: Array2<f64>,
    pub query_index: Array1<usize>,
    pub solid_thermal_conductivity: f64,
    pub solid_thermal_diffusivity: f64,
    pub characteristic_length: f64,
    pub air_thermal_conductivity: f64,
    pub dt: f64,
    pub h0: f64,
    pub max_iter_num: usize,
}

impl DoubleCaseData {
    pub fn new(case_data1: CaseData, case_data2: CaseData) -> Result<Self, Box<dyn Error>> {
        if case_data1.peak_temp != case_data2.peak_temp {
            return Err("two case should have same peak temperature")?;
        }

        let solid_thermal_conductivity =
            if case_data1.solid_thermal_conductivity == case_data2.solid_thermal_conductivity {
                case_data1.solid_thermal_conductivity
            } else {
                return Err("two case should have same peak solid thermal conductivity")?;
            };

        let solid_thermal_diffusivity =
            if case_data1.solid_thermal_diffusivity == case_data2.solid_thermal_diffusivity {
                case_data1.solid_thermal_diffusivity
            } else {
                return Err("two case should have same peak solid thermal diffusivity")?;
            };

        let characteristic_length =
            if case_data1.characteristic_length == case_data2.characteristic_length {
                case_data1.characteristic_length
            } else {
                return Err("two case should have same peak characteristic length")?;
            };

        let air_thermal_conductivity =
            if case_data1.air_thermal_conductivity == case_data2.air_thermal_conductivity {
                case_data1.air_thermal_conductivity
            } else {
                return Err("two case should have same peak air thermal conductivity")?;
            };

        let dt = if case_data1.dt == case_data2.dt {
            case_data1.dt
        } else {
            return Err("two case should have same peak dt")?;
        };

        let h0 = if case_data1.h0 == case_data2.h0 {
            case_data1.h0
        } else {
            return Err("two case should have same peak h0")?;
        };

        let max_iter_num = if case_data1.max_iter_num == case_data2.max_iter_num {
            case_data1.max_iter_num
        } else {
            return Err("two case should have same peak max iter num")?;
        };

        Ok(Self {
            peak_frames1: case_data1.peak_frames,
            peak_frames2: case_data2.peak_frames,
            interp_temps1: case_data1.interp_temps,
            interp_temps2: case_data2.interp_temps,
            query_index: case_data1.query_index,
            solid_thermal_conductivity,
            solid_thermal_diffusivity,
            characteristic_length,
            air_thermal_conductivity,
            dt,
            h0,
            max_iter_num,
        })
    }

    pub fn solve(&self) -> Array1<f64> {
        let mut nus = Array1::zeros(self.query_index.len());

        Zip::from(&mut nus)
            .and(&self.query_index)
            .and(&self.peak_frames1)
            .and(&self.peak_frames2)
            .par_apply(|nu, &index, &peak_frame1, &peak_frame2| {
                let data1 = PointData {
                    solid_thermal_conductivity: self.solid_thermal_conductivity,
                    solid_thermal_diffusivity: self.solid_thermal_diffusivity,
                    dt: self.dt,
                    peak_frame: peak_frame1,
                    temps: self.interp_temps1.column(index),
                };
                let data2 = PointData {
                    solid_thermal_conductivity: self.solid_thermal_conductivity,
                    solid_thermal_diffusivity: self.solid_thermal_diffusivity,
                    dt: self.dt,
                    peak_frame: peak_frame2,
                    temps: self.interp_temps2.column(index),
                };
                let double_point_solver = DoublePointSolver {
                    data1,
                    data2,
                    h0: self.h0,
                    max_iter_num: self.max_iter_num,
                    characteristic_length: self.characteristic_length,
                    air_thermal_conductivity: self.air_thermal_conductivity,
                };

                *nu = double_point_solver.solve_nu();
            });

        nus
    }
}
