#![allow(dead_code)]

use std::sync::{Condvar, Mutex};

struct Pg {
    mu: Mutex<usize>,
    cond_var: Condvar,
}

impl Pg {
    fn new() -> Pg {
        Pg {
            mu: Mutex::new(0),
            cond_var: Condvar::new(),
        }
    }
}
