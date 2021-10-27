use tokio::sync::{mpsc, oneshot};

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<(String, oneshot::Sender<usize>)>(3);

    tokio::spawn(async move {
        let mut i = 0;
        while let Some((msg, tx)) = rx.recv().await {
            println!("{}", msg);
            tx.send(i).unwrap();
            i += 1;
        }
    });

    tauri::Builder::default()
        .manage(tx)
        .invoke_handler(tauri::generate_handler![my_custom_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[derive(serde::Serialize)]
struct Response {
    val: usize,
}

async fn some_other_function() -> Option<String> {
    Some("response".to_owned())
}

#[tauri::command]
async fn my_custom_command(
    window: tauri::Window,
    state: tauri::State<'_, mpsc::Sender<(String, oneshot::Sender<usize>)>>,
) -> Result<Response, String> {
    println!("Called from {}", window.label());
    let (tx, rx) = oneshot::channel();
    match some_other_function().await {
        Some(message) => {
            state.send((message, tx)).await.unwrap();
            Ok(Response {
                val: rx.await.unwrap(),
            })
        }
        None => Err("No result".to_owned()),
    }
}
