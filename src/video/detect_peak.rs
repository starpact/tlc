use std::sync::Arc;

use anyhow::bail;
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

#[instrument(skip(green2))]
pub fn filter_detect_peak(green2: ArcArray2<u8>, filter_method: FilterMethod) -> Arc<[usize]> {
    fn index_of_max<I, F>(v: I, f: F) -> usize
    where
        I: IntoIterator,
        F: FnMut(&(usize, I::Item)) -> u8,
    {
        v.into_iter().enumerate().max_by_key(f).unwrap().0
    }

    use FilterMethod::*;
    (match filter_method {
        No => apply(green2, |green1| index_of_max(green1, |(_, &g)| g)),
        Median { window_size } => apply(green2, move |green1| {
            let mut filter = Filter::new(window_size);
            index_of_max(green1, |(_, &g)| filter.consume(g))
        }),
        Wavelet { threshold_ratio } => apply(green2, move |green1| {
            let green1 = wavelet_transform(green1, &db8_wavelet(), threshold_ratio);
            index_of_max(&green1, |(_, &g)| g as u8)
        }),
    })
    .into()
}

#[instrument(skip(green2), err)]
pub fn filter_point(
    green2: ArcArray2<u8>,
    filter_method: FilterMethod,
    area: (u32, u32, u32, u32),
    (y, x): (u32, u32),
) -> anyhow::Result<Vec<u8>> {
    let (h, w) = (area.2, area.3);
    if y >= h {
        bail!("y({y}) out of range({h})");
    }
    if x >= w {
        bail!("x({x}) out of range({w})");
    }
    let position = y * w + x;
    let green1 = green2.column(position as usize);

    let green_history = match filter_method {
        FilterMethod::No => green1.to_vec(),
        FilterMethod::Median { window_size } => filter_median(green1, window_size),
        FilterMethod::Wavelet { threshold_ratio } => {
            filter_wavelet(green1, &db8_wavelet(), threshold_ratio)
        }
    };
    Ok(green_history)
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
    use super::*;
    use crate::{
        util::log,
        video::{
            read_video,
            tests::{video_meta_real, VIDEO_PATH_REAL},
        },
    };

    #[ignore]
    #[test]
    fn test_detect() {
        log::init();
        let video_data = read_video(VIDEO_PATH_REAL).unwrap();
        let green2 = video_data
            .decode_range_area(10, video_meta_real().nframes - 10, (10, 10, 800, 1000))
            .unwrap()
            .into_shared();

        filter_detect_peak(green2.clone(), FilterMethod::No);
        filter_detect_peak(green2.clone(), FilterMethod::Median { window_size: 10 });
        filter_detect_peak(
            green2,
            FilterMethod::Wavelet {
                threshold_ratio: 0.8,
            },
        );
    }
}
