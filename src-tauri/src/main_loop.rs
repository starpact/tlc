use crossbeam::channel::Receiver;
use rusqlite::Connection;
use tracing::info;

use crate::{request::Request, state::GlobalState};

pub fn main_loop(db: Connection, request_receiver: Receiver<Request>) {
    let mut global_state = GlobalState::new(db);
    while global_state.handle(&request_receiver).is_ok() {}
    info!("exit main loop");
}
