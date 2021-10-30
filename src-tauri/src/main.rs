mod command;
mod server;

use tokio::sync::{mpsc, oneshot};
use tracing::{error, Level};

use command::*;

const CHANNEL_BUFFER_SIZE: usize = 3;

/// Core calculation data are placed in different coroutine from main's and
/// [mpsc & oneshot](https://docs.rs/tokio/1.13.0/tokio/sync/index.html) is used to handle asynchronously.
/// In this way we can avoid visiting calculation data from another coroutine so `Send`, `Sync` and `'static`
/// constraints are not needed.
///
/// Actually spsc is enough here but tokio does not provide spsc and I didn't find any async spsc that is
/// actively maintained. The slight extra overhead can be ignored.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    let (tx, rx) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(CHANNEL_BUFFER_SIZE);
    tokio::spawn(server::serve(rx));

    tauri::Builder::default()
        .manage(tx)
        .invoke_handler(tauri::generate_handler![get_save_info])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("uncaught error: {}", e));
}
