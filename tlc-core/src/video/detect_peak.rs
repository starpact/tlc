use anyhow::{bail, Result};
use dwt::{transform, wavelet::Wavelet, Operation};
use median::Filter;
use ndarray::{parallel::prelude::*, prelude::*, ArcArray2};
use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq)]
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

impl Eq for FilterMethod {}

impl std::hash::Hash for FilterMethod {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            FilterMethod::No => state.write_u8(0),
            FilterMethod::Median { window_size } => {
                state.write_u8(1);
                window_size.hash(state)
            }
            FilterMethod::Wavelet { threshold_ratio } => {
                state.write_u8(2);
                threshold_ratio.to_bits().hash(state);
            }
        }
    }
}

#[instrument(skip(green2))]
pub fn filter_detect_peak(green2: ArcArray2<u8>, filter_method: FilterMethod) -> Vec<usize> {
    use FilterMethod::*;
    match filter_method {
        No => apply(green2, |green1| {
            green1
                .into_iter()
                .enumerate()
                .max_by_key(|(_, &g)| g)
                .unwrap()
                .0
        }),
        Median { window_size } => apply(green2, move |green1| {
            let mut filter = Filter::new(window_size);
            green1
                .into_iter()
                .enumerate()
                .max_by_key(|(_, &g)| filter.consume(g))
                .unwrap()
                .0
        }),
        Wavelet { threshold_ratio } => {
            let wavelet = db8_wavelet();
            apply(green2, move |green1| {
                wavelet_transform(green1, &wavelet, threshold_ratio)
                    .into_iter()
                    .enumerate()
                    .max_by_key(|&(_, g)| g as u8)
                    .unwrap()
                    .0
            })
        }
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
        FilterMethod::Wavelet { threshold_ratio } => {
            filter_wavelet(green1, &db8_wavelet(), threshold_ratio)
        }
    };
    Ok(temp_history)
}

fn apply<F>(green2: ArcArray2<u8>, f: F) -> Vec<usize>
where
    F: Fn(ArrayView1<u8>) -> usize + Send + Sync,
{
    green2.axis_iter(Axis(1)).into_par_iter().map(f).collect()
}

fn filter_median(green1: ArrayView1<u8>, window_size: usize) -> Vec<u8> {
    let mut filter = Filter::new(window_size);
    green1.into_iter().map(|&g| filter.consume(g)).collect()
}

fn filter_wavelet(green1: ArrayView1<u8>, wavelet: &Wavelet<f64>, threshold_ratio: f64) -> Vec<u8> {
    wavelet_transform(green1, wavelet, threshold_ratio)
        .into_iter()
        .map(|x| x as u8)
        .collect()
}

/// Refer to [pywavelets](https://pywavelets.readthedocs.io/en/latest/ref).
fn wavelet_transform(
    green1: ArrayView1<u8>,
    wavelet: &Wavelet<f64>,
    threshold_ratio: f64,
) -> Vec<f64> {
    let data_len = green1.len();

    let max_level = ((data_len / (wavelet.length - 1)) as f64).log2() as usize;
    let level_2 = 1 << max_level;
    let filter_len = data_len / level_2 * level_2;
    let mut green1f: Vec<_> = green1.iter().take(filter_len).map(|v| *v as f64).collect();

    // Decomposition.
    transform(
        &mut green1f[..filter_len],
        Operation::Forward,
        wavelet,
        max_level,
    );

    let mut start = filter_len / (1 << max_level);
    for _ in 0..max_level {
        let end = start * 2;
        let m = green1f[start..end].iter().fold(0.0, |m, &v| f64::max(m, v));
        let threshold = m * threshold_ratio;
        for v in &mut green1f[start..end] {
            *v = v.signum() * f64::max(v.abs() - threshold, 0.0);
        }
        start = end;
    }

    // Reconstruction.
    transform(
        &mut green1f[..filter_len],
        Operation::Inverse,
        wavelet,
        max_level,
    );
    green1f
}

/// Refer to [Daubechies 8](http://wavelets.pybytes.com/wavelet/db8)。
/// Horizontal flip.
fn db8_wavelet() -> Wavelet<f64> {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        util::log,
        video::{
            read::read_video,
            tests::{video_meta_real, VIDEO_PATH_REAL},
            DecoderManager,
        },
    };

    use super::*;

    #[ignore]
    #[test]
    fn test_detect() {
        log::init();
        let (_, parameters, packets) = read_video(VIDEO_PATH_REAL).unwrap();
        let decode_manager = DecoderManager::new(parameters, 10, 20);
        let green2 = decode_manager
            .decode_all(
                Arc::new(packets),
                10,
                video_meta_real().nframes - 10,
                (10, 10, 800, 1000),
            )
            .unwrap()
            .into_shared();

        // filter_detect_peak(green2.clone(), FilterMethod::No);
        // filter_detect_peak(green2.clone(), FilterMethod::Median { window_size: 10 });
        let x = filter_detect_peak(
            green2,
            FilterMethod::Wavelet {
                threshold_ratio: 0.8,
            },
        );
        println!("{}", x[..10000].iter().sum::<usize>());
    }
}
