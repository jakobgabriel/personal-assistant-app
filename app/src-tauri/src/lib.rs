//! Die Tauri-Schale.
//!
//! Hier steht so wenig wie moeglich: Fenster aufbauen, [`tory_core::engine::Engine`]
//! oeffnen, Befehle durchreichen. Alles, was eine Entscheidung trifft, liegt in
//! `tory-core` — und ist damit ohne Android-SDK und ohne WebView testbar.

mod commands;

use std::sync::Arc;

use tauri::Manager;
use tory_core::engine::Engine;

/// Zustand, den jeder Befehl bekommt.
pub struct AppState {
    pub engine: Arc<Engine>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                // Was im Netz passiert, ist beim Einrichten die haeufigste Frage.
                .level_for("tory_core", log::LevelFilter::Debug)
                .build(),
        )
        .setup(|app| {
            // Auf Android ist das das private Verzeichnis der App, auf dem
            // Desktop `~/.local/share/de.tory.app` bzw. das Aequivalent.
            let data_dir = app.path().app_data_dir()?;
            let engine = Engine::open(&data_dir)?;

            // Ein Deep Link kann die App starten oder eine laufende erreichen.
            // Beides landet hier.
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    let urls: Vec<String> = event.urls().iter().map(|u| u.to_string()).collect();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        commands::handle_deep_links(handle, urls).await;
                    });
                });
            }

            app.manage(AppState { engine });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::validate_config,
            commands::get_dashboard,
            commands::get_signals,
            commands::sync_now,
            commands::run_action,
            commands::snooze_signal,
            commands::dismiss_signal,
            commands::set_secret,
            commands::list_secret_names,
            commands::set_timezone_offset,
            commands::begin_gmail_auth,
            commands::finish_gmail_auth,
            commands::daily_brief,
            commands::app_info,
        ])
        .run(tauri::generate_context!())
        .expect("Tory konnte nicht starten");
}
