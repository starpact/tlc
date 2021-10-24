use tauri::async_runtime;
fn main() {
    tauri::Builder::default()
        .manage(Count(Default::default()))
        .invoke_handler(tauri::generate_handler![my_custom_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

struct Count(async_runtime::RwLock<usize>);

#[derive(serde::Serialize)]
struct Response {
    message: String,
    val: usize,
}

async fn some_other_function() -> Option<String> {
    Some("response".to_owned())
}

#[tauri::command]
async fn my_custom_command(
    window: tauri::Window,
    number: usize,
    count: tauri::State<'_, Count>,
) -> Result<Response, String> {
    println!("Called from {}", window.label());
    match some_other_function().await {
        Some(message) => {
            *count.0.write().await += number;
            Ok(Response {
                message,
                val: *count.0.read().await + number,
            })
        }
        None => Err("No result".to_owned()),
    }
}
