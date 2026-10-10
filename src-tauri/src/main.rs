#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use signal_core::runtime::{Handle, Request};
use tauri::Manager;

#[tauri::command]
async fn signal_request(
    request: Request,
    state: tauri::State<'_, Handle>,
) -> Result<serde_json::Value, String> {
    state.call(request).await
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let handle = tauri::async_runtime::block_on(Handle::start(dir.join("signal.sqlite3")))?;
            app.manage(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                if let Some(handle) = window.try_state::<Handle>() {
                    // Flush the recorder before the native process exits normally.
                    let _ = tauri::async_runtime::block_on(handle.call(Request::Shutdown));
                }
            }
        })
        .invoke_handler(tauri::generate_handler![signal_request])
        .run(tauri::generate_context!())
        .expect("SIGNAL desktop failed to start");
}
