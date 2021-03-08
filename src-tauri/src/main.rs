#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::mpsc;

use tlc::view::{handle::init, request::Request};

fn main() {
    let (tx, rx) = mpsc::sync_channel(2);
    init(rx);

    tauri::AppBuilder::new()
        .invoke_handler(move |webview, arg| {
            let req: Request = serde_json::from_str(arg).unwrap();
            println!("{}: {:?}", req.cmd, req.body);
            let _ = tx.try_send((webview.as_mut(), req));

            Ok(())
        })
        .build()
        .run();
}
