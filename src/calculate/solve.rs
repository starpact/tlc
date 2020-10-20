mod cmath {
    use libc::c_double;
    extern "C" {
        pub fn erf(x: c_double) -> c_double;
        pub fn erfc(x: c_double) -> c_double;
    }
}

pub fn erf(x: f64) -> f64 {
    unsafe { cmath::erf(x) }
}

pub fn erfc(x: f64) -> f64 {
    unsafe { cmath::erfc(x) }
}
