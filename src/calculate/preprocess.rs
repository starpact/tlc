use ndarray::parallel::prelude::*;
use ndarray::prelude::*;
use ndarray::Zip;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum FilterMethod {
    Median(usize),
    Wavelet,
}

use median::Filter;

/// filter the green history of each pixel along time axis
pub fn filtering(g2d: Array2<u8>, filter_method: FilterMethod) -> Array2<u8> {
    match filter_method {
        FilterMethod::Median(window_size) => {
            let mut filtered_g2d = Array2::zeros(g2d.dim());
            Zip::from(g2d.axis_iter(Axis(1)))
                .and(filtered_g2d.axis_iter_mut(Axis(1)))
                .par_apply(|col_raw, mut col_filtered| {
                    let mut filter = Filter::new(window_size);
                    col_raw
                        .iter()
                        .zip(col_filtered.iter_mut())
                        .for_each(|(g_raw, g_filtered)| {
                            *g_filtered = filter.consume(*g_raw);
                        })
                });

            filtered_g2d
        }
        _ => unimplemented!("在做了"),
    }
}

/// *traverse along the timeline to detect the peak of green values and record that frame index*
/// ### Argument:
/// green values 2D matrix
/// ### Return:
/// frame indexes of maximal green values
pub fn detect_peak(g2d: Array2<u8>) -> Array1<usize> {
    let mut peak_frames = Vec::with_capacity(g2d.ncols());

    g2d.axis_iter(Axis(1))
        .into_par_iter()
        .map(|column| {
            let (first_max, last_max, _) =
                column
                    .iter()
                    .enumerate()
                    .fold((0, 0, 0), |(mi_l, mi_r, mg), (i, &g)| {
                        if g > mg {
                            (i, i, g)
                        } else if g == mg {
                            (mi_l, i, g)
                        } else {
                            (mi_l, mi_r, mg)
                        }
                    });
            (first_max + last_max) >> 1
        })
        .collect_into_vec(&mut peak_frames);

    Array1::from(peak_frames)
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum InterpMethod {
    Horizontal,
    Vertical,
    TwoDimension,
}

/// interpolation of temperature matrix
/// ### Argument:
/// 2D matrix of the delta temperatures between adjacent frames
///
/// positions of thermocouples
///
/// interpolation method
///
/// position of upper left corner
///
/// width of calculation region
///
/// ### Return:
/// 2D matrix of the interpolated temperatures and how a pixel query about its temperature from this matrix
pub fn interp(
    t2d: ArrayView2<f64>,
    tc_pos: &Vec<(i32, i32)>,
    interp_method: InterpMethod,
    upper_left_pos: (usize, usize),
    region_shape: (usize, usize),
) -> (Array2<f64>, Array1<usize>) {
    match interp_method {
        InterpMethod::Horizontal | InterpMethod::Vertical => {
            interp_1d(t2d, tc_pos, upper_left_pos, interp_method, region_shape)
        }
        InterpMethod::TwoDimension => unimplemented!("在做了"),
    }
}

/// *one dimension interpolation, along axis X or Y*
fn interp_1d(
    t2d: ArrayView2<f64>,
    thermocouple_pos: &Vec<(i32, i32)>,
    upper_left_pos: (usize, usize),
    interp_method: InterpMethod,
    region_shape: (usize, usize),
) -> (Array2<f64>, Array1<usize>) {
    let (cal_h, cal_w) = region_shape;

    let (len_of_interp_dimension, tc_pos_relative, query_index) = match interp_method {
        InterpMethod::Horizontal => (
            cal_w,
            thermocouple_pos
                .iter()
                .map(|tc_pos_raw| tc_pos_raw.1 - upper_left_pos.1 as i32)
                .collect::<Vec<_>>(),
            (0..cal_w).cycle().take(cal_h * cal_w).collect(),
        ),
        InterpMethod::Vertical => (
            cal_h,
            thermocouple_pos
                .iter()
                .map(|tc_pos_raw| tc_pos_raw.0 - upper_left_pos.0 as i32)
                .collect::<Vec<_>>(),
            (0..cal_h * cal_w).map(|x| x / cal_w).collect(),
        ),
        _ => panic!("only horizontal or vertical for one dimensional interpolation"),
    };

    let mut interp_temps = Array2::zeros((t2d.nrows(), len_of_interp_dimension));
    par_azip!((row0 in t2d.axis_iter(Axis(0)), mut row1 in interp_temps.axis_iter_mut(Axis(0))) {
        let mut iter = row1.iter_mut();
        let mut curr = 0;
        for pos in 0..len_of_interp_dimension as i32 {
            let (left_end, right_end) = (tc_pos_relative[curr], tc_pos_relative[curr + 1]);
            if pos == right_end && curr + 2 < tc_pos_relative.len() {
                curr += 1;
            }
            *iter.next().unwrap() = (row0[curr] * (right_end - pos) as f64
                + row0[curr + 1] * (pos - left_end) as f64) / (right_end -left_end) as f64;
        }
    });

    (interp_temps, query_index)
}
