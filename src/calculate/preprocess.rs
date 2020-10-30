use ndarray::parallel::prelude::*;
use ndarray::prelude::*;

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
                })
        })
        .map(|x| (x.0 + x.1) >> 1)
        .collect_into_vec(&mut peak_frames);

    Array1::from(peak_frames)
}

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
    tc_pos: &Vec<(i32, i32)>,
    ul_pos: (usize, usize),
    interp_method: InterpMethod,
    region_shape: (usize, usize),
) -> (Array2<f64>, Array1<usize>) {
    let (cal_h, cal_w) = region_shape;
    let mut query_index = Array1::zeros(cal_h * cal_w);
    let mut iter = query_index.iter_mut();
    let mut interp_1d_t: Array2<f64>;
    let tc_pos_1d: Vec<i32> = match interp_method {
        InterpMethod::Horizontal => {
            (0..cal_h).for_each(|_| (0..cal_w).for_each(|x| *iter.next().unwrap() = x));
            interp_1d_t = Array2::zeros((t2d.nrows(), cal_w));
            tc_pos.iter().map(|pos| pos.1 - ul_pos.1 as i32).collect()
        }
        InterpMethod::Vertical => {
            (0..cal_h).for_each(|y| (0..cal_w).for_each(|_| *iter.next().unwrap() = y));
            interp_1d_t = Array2::zeros((t2d.nrows(), cal_h));
            tc_pos.iter().map(|pos| pos.0 - ul_pos.0 as i32).collect()
        }
        _ => panic!("only horizontal or vertical for interpolation 1D"),
    };
    let len_of_interp_dimension = interp_1d_t.ncols() as i32;
    par_azip!((row0 in t2d.axis_iter(Axis(0)), mut row1 in interp_1d_t.axis_iter_mut(Axis(0))) {
        let mut iter = row1.iter_mut();
        let mut curr = 0;
        let (mut left, mut right) = (tc_pos_1d[curr], tc_pos_1d[curr + 1]);
        for pos in 0..len_of_interp_dimension {
            if pos == right && curr + 2 < tc_pos_1d.len() {
                curr += 1;
                left = tc_pos_1d[curr];
                right = tc_pos_1d[curr + 1];
            }
            *iter.next().unwrap() = (row0[curr] * (right - pos) as f64
                + row0[curr + 1] * (pos - left) as f64) / (right -left) as f64;
        }
    });
    (interp_1d_t, query_index)
}
