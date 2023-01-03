use std::path::Path;

use crossbeam::channel::Receiver;
use rusqlite::Connection;
use tracing::info;

use crate::{request::Request, setting::new_db, state::GlobalState};

pub fn run<P: AsRef<Path>>(db_path: P, request_receiver: Receiver<Request>) {
    tlc_util::log::init();
    tlc_video::init();
    main_loop(new_db(db_path), request_receiver);
}

pub(crate) fn main_loop(db: Connection, request_receiver: Receiver<Request>) {
    let mut global_state = GlobalState::new(db);
    while global_state.handle(&request_receiver).is_ok() {}
    info!("exit main loop");
}
