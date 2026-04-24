#![allow(dead_code)]

mod audio;
mod commands;
mod transport;

use commands::AppState;
use tauri::Manager;
use tracing::info;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_devices,
            commands::start_interpretation,
            commands::stop_interpretation,
            commands::start_mic_bridge,
            commands::stop_mic_bridge,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                info!("App exiting, cleaning up audio capture...");
                let state = app.state::<AppState>();
                // Send stop signal to clean up SCStream before exit
                if let Ok(mut guard) = state.stop_tx.try_lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.try_send(());
                    }
                }
                if let Ok(mut guard) = state.mic_bridge_stop_tx.try_lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.try_send(());
                    }
                }
                // Give a moment for cleanup to complete
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
}
