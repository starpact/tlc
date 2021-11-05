mod command;
mod config;
mod data;
mod handler;

use tracing::{error, Level};

use command::*;

use crate::handler::TLCHandler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_max_level(Level::DEBUG)
        .init();

    let handler = TLCHandler::new().await;

    tauri::Builder::default()
        .manage(handler)
        .invoke_handler(tauri::generate_handler![
            load_config,
            get_save_info,
            set_video_path,
            get_frame,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| error!("uncaught error: {}", e));
}
