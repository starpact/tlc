mod command;
mod handler;

use tracing::{error, Level};

use crate::handler::TLCHandler;
use command::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    let tlc_handler = TLCHandler::new().await;

    tauri::Builder::default()
        .manage(tlc_handler)
        .invoke_handler(tauri::generate_handler![
            load_config,
            get_save_info,
            set_video_path,
            get_frame,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("uncaught error: {}", e));
}
