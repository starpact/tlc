use std::hash::{Hash, Hasher};

use anyhow::{bail, Result};
use dwt::{transform, wavelet::Wavelet, Operation};
use median::Filter;
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::Green2Id;

use super::controller::ProgressBar;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub enum FilterMethod {
    #[default]
    No,
    Median {
        window_size: usize,
    },
    Wavelet {
        threshold_ratio: f64,
    },
}

impl PartialEq for FilterMethod {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FilterMethod::No, FilterMethod::No) => true,
            (
                FilterMethod::Median {
                    window_size: window_size1,
                },
                FilterMethod::Median {
                    window_size: window_size2,
                },
            ) => window_size1 == window_size2,
            (
                FilterMethod::Wavelet {
                    threshold_ratio: threshold_ratio1,
                },
                FilterMethod::Wavelet {
                    threshold_ratio: threshold_ratio2,
                },
            ) => threshold_ratio1 == threshold_ratio2,
            _ => false,
        }
    }
}

impl Hash for FilterMethod {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            FilterMethod::No => state.write_u8(0),
            FilterMethod::Median { window_size } => {
                state.write_u8(1);
                state.write_u64(*window_size as u64);
            }
            FilterMethod::Wavelet { threshold_ratio } => {
                state.write_u8(1);
                state.write_u64(threshold_ratio.to_bits());
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct GmaxId {
    pub green2_id: Green2Id,
    pub filter_method: FilterMethod,
}

impl GmaxId {
    pub fn eval_hash(&self) -> u64 {
        let mut hasher = ahash::AHasher::default();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

#[instrument(skip(green2, progress_bar), err)]
pub fn filter_detect_peak(
    green2: ArcArray2<u8>,
    filter_method: FilterMethod,
    progress_bar: ProgressBar,
) -> Result<Vec<usize>> {
    let total = green2.dim().1;
    progress_bar.start(total as u32)?;

    use FilterMethod::*;
    match filter_method {
        No => apply(green2, progress_bar, filter_detect_peak_no),
        Median { window_size } => apply(green2, progress_bar, move |g1| {
            filter_detect_peak_median(g1, window_size)
        }),
        Wavelet { threshold_ratio } => apply(green2, progress_bar, move |g1| {
            filter_detect_peak_wavelet(g1, threshold_ratio)
        }),
    }
}

#[instrument(skip(green2), err)]
pub fn filter_point(
    green2: ArcArray2<u8>,
    filter_method: FilterMethod,
    area: (u32, u32, u32, u32),
    (y, x): (usize, usize),
) -> Result<Vec<u8>> {
    let (h, w) = (area.2 as usize, area.3 as usize);
    if y >= h {
        bail!("y({y}) out of range({h})");
    }
    if x >= w {
        bail!("x({x}) out of range({w})");
    }
    let position = y * w + x;
    let green1 = green2.column(position);

    let temp_history = match filter_method {
        FilterMethod::No => green1.to_vec(),
        FilterMethod::Median { window_size } => filter_median(green1, window_size),
        FilterMethod::Wavelet { threshold_ratio } => filter_wavelet(green1, threshold_ratio),
    };

    Ok(temp_history)
}

fn apply<F>(green2: ArcArray2<u8>, progress_bar: ProgressBar, f: F) -> Result<Vec<usize>>
where
    F: Fn(ArrayView1<u8>) -> usize + Send + Sync,
{
    green2
        .axis_iter(Axis(1))
        .into_par_iter()
        .map(|green_history| {
            progress_bar.add(1)?;
            Ok(f(green_history))
        })
        .collect()
}

fn filter_detect_peak_no(green1: ArrayView1<u8>) -> usize {
    green1
        .into_iter()
        .enumerate()
        .max_by_key(|(_, &g)| g)
        .unwrap()
        .0
}

fn filter_detect_peak_median(green1: ArrayView1<u8>, window_size: usize) -> usize {
    let mut filter = Filter::new(window_size);
    green1
        .into_iter()
        .enumerate()
        .max_by_key(|(_, &g)| filter.consume(g))
        .unwrap()
        .0
}

fn filter_detect_peak_wavelet(green1: ArrayView1<u8>, threshold_ratio: f64) -> usize {
    wavelet(green1, threshold_ratio)
        .into_iter()
        .enumerate()
        .max_by_key(|&(_, g)| g as u8)
        .unwrap()
        .0
}

fn filter_median(green1: ArrayView1<u8>, window_size: usize) -> Vec<u8> {
    let mut filter = Filter::new(window_size);
    green1.into_iter().map(|&g| filter.consume(g)).collect()
}

fn filter_wavelet(green1: ArrayView1<u8>, threshold_ratio: f64) -> Vec<u8> {
    wavelet(green1, threshold_ratio)
        .into_iter()
        .map(|x| x as u8)
        .collect()
}

/// Refer to [pywavelets](https://pywavelets.readthedocs.io/en/latest/ref).
fn wavelet(green1: ArrayView1<u8>, threshold_ratio: f64) -> Vec<f64> {
    let data_len = green1.len();
    let wavelet = db8();

    let max_level = ((data_len / (wavelet.length - 1)) as f64).log2() as usize;
    let level_2 = 1 << max_level;
    let filter_len = data_len / level_2 * level_2;
    let mut green1f: Vec<_> = green1.iter().take(filter_len).map(|v| *v as f64).collect();

    // Decomposition.
    transform(
        &mut green1f[..filter_len],
        Operation::Forward,
        &wavelet,
        max_level,
    );

    let mut start = filter_len / (1 << max_level);
    for _ in 0..max_level {
        let end = start << 1;
        let m = green1f[start..end].iter().fold(0., |m, &v| f64::max(m, v));
        let threshold = m * threshold_ratio;
        for v in &mut green1f[start..end] {
            *v = v.signum() * f64::max(v.abs() - threshold, 0.);
        }
        start = end;
    }

    // Reconstruction.
    transform(
        &mut green1f[..filter_len],
        Operation::Inverse,
        &wavelet,
        max_level,
    );

    green1f
}

/// Refer to [Daubechies 8](http://wavelets.pybytes.com/wavelet/db8)。
/// Horizontal flip.
#[inline]
fn db8() -> Wavelet<f64> {
    #[rustfmt::skip]
    let lo = vec![
        -0.00011747678400228192, 0.0006754494059985568,
        -0.0003917403729959771,  -0.00487035299301066,
        0.008746094047015655,    0.013981027917015516,
        -0.04408825393106472,    -0.01736930100202211,
        0.128747426620186,       0.00047248457399797254,
        -0.2840155429624281,     -0.015829105256023893,
        0.5853546836548691,      0.6756307362980128,
        0.3128715909144659,      0.05441584224308161,
    ];
    #[rustfmt::skip]
    let hi = vec![
        -0.05441584224308161,    0.3128715909144659,
        -0.6756307362980128,     0.5853546836548691,
        0.015829105256023893,    -0.2840155429624281,
        -0.00047248457399797254, 0.128747426620186,
        0.01736930100202211,     -0.04408825393106472,
        -0.013981027917015516,   0.008746094047015655,
        0.00487035299301066,     -0.0003917403729959771,
        -0.0006754494059985568,  -0.00011747678400228192,
    ];

    Wavelet {
        length: lo.len(),
        offset: 0,
        dec_lo: lo.clone(),
        dec_hi: hi.clone(),
        rec_lo: lo,
        rec_hi: hi,
    }
}
