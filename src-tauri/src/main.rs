#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::mpsc;

use tlc::view::{cmd::Cmd, handle::init};

fn main() {
    let (tx, rx) = mpsc::sync_channel(1);
    init(rx);

    tauri::AppBuilder::new()
        .invoke_handler(move |webview, arg| {
            let cmd: Cmd = serde_json::from_str(arg).map_err(|err| err.to_string())?;
            let _ = tx.try_send((webview.as_mut(), cmd));

            Ok(())
        })
        .build()
        .run();
}
