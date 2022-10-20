use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_as_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_millis() as u64
}
