//! Die Befehle, die die Oberflaeche aufrufen darf.
//!
//! Jeder ist eine Zeile Weiterleitung an die Engine plus die Uebersetzung des
//! Fehlers in einen Text, den die Oberflaeche anzeigen kann. Bewusst keine
//! Logik: was hier entschieden wuerde, waere nicht mehr testbar.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tory_core::ai::Completion;
use tory_core::config::Config;
use tory_core::engine::{Dashboard, SyncReport};
use tory_core::model::{Action, Signal};

use crate::AppState;

/// Tauri braucht `Result<_, String>`; `tory_core::Error` traegt die Meldung
/// bereits auf Deutsch.
type CmdResult<T> = Result<T, String>;

fn fail(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> CmdResult<Config> {
    Ok(state.engine.config().await)
}

/// Speichert und gibt die Probleme zurueck. Eine leere Liste heisst: uebernommen.
#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: Config) -> CmdResult<Vec<String>> {
    state.engine.save_config(config).await.map_err(fail)
}

/// Prueft, ohne zu speichern — fuer die Rueckmeldung waehrend des Tippens.
#[tauri::command]
pub async fn validate_config(config: Config) -> CmdResult<Vec<String>> {
    Ok(config.validate())
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> CmdResult<Dashboard> {
    state.engine.dashboard().await.map_err(fail)
}

#[tauri::command]
pub async fn get_signals(state: State<'_, AppState>, source: String) -> CmdResult<Vec<Signal>> {
    state.engine.signals_of(&source).await.map_err(fail)
}

/// `force = true` ist der Knopf "jetzt aktualisieren"; `false` fragt nur, was
/// nach Takt und Backoff faellig ist.
#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>, force: bool) -> CmdResult<SyncReport> {
    state.engine.sync(force).await.map_err(fail)
}

/// Fuehrt die Aktion eines Signals aus. Liefert die Engine eine URL zurueck,
/// uebernimmt das Betriebssystem — nur hier, nicht im Webview.
#[tauri::command]
pub async fn run_action(
    app: AppHandle,
    state: State<'_, AppState>,
    action: Action,
) -> CmdResult<()> {
    if let Some(url) = state.engine.act(&action).await.map_err(fail)? {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(url, None::<&str>).map_err(fail)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn snooze_signal(state: State<'_, AppState>, key: String, hours: i64) -> CmdResult<()> {
    state.engine.snooze(&key, hours).await.map_err(fail)
}

#[tauri::command]
pub async fn dismiss_signal(state: State<'_, AppState>, key: String) -> CmdResult<()> {
    state.engine.dismiss(&key).await.map_err(fail)
}

/// Legt ein Geheimnis ab. Der Wert geht in eine Richtung: von der Eingabe in den
/// verschluesselten Store, nie zurueck in die Oberflaeche.
#[tauri::command]
pub async fn set_secret(state: State<'_, AppState>, name: String, value: String) -> CmdResult<()> {
    state.engine.set_secret(&name, &value).await.map_err(fail)
}

/// Nur die Namen — damit die Einstellungen "hinterlegt" anzeigen koennen.
#[tauri::command]
pub async fn list_secret_names(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    Ok(state.engine.secret_names().await)
}

/// Die Oberflaeche kennt die Zeitzone des Geraets, der Kern nicht.
#[tauri::command]
pub async fn set_timezone_offset(state: State<'_, AppState>, minutes: i32) -> CmdResult<()> {
    state.engine.set_tz_offset(minutes).await;
    Ok(())
}

/// Startet die Gmail-Anmeldung und oeffnet Google im Systembrowser.
///
/// Ausdruecklich nicht im Webview: ein eingebettetes Anmeldefenster ist das
/// Muster, vor dem Google warnt, und es kaeme ohne den PKCE-Rueckkanal aus.
#[tauri::command]
pub async fn begin_gmail_auth(
    app: AppHandle,
    state: State<'_, AppState>,
    instance: String,
) -> CmdResult<()> {
    let url = state.engine.begin_gmail_auth(&instance).await.map_err(fail)?;
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(fail)
}

/// Falls der Rueckkanal nicht als Deep Link ankommt (etwa weil der Nutzer die
/// URL von Hand kopiert), nimmt dieser Befehl sie entgegen.
#[tauri::command]
pub async fn finish_gmail_auth(
    state: State<'_, AppState>,
    redirect_url: String,
) -> CmdResult<String> {
    state.engine.finish_gmail_auth(&redirect_url).await.map_err(fail)
}

#[tauri::command]
pub async fn daily_brief(state: State<'_, AppState>) -> CmdResult<Completion> {
    state.engine.brief().await.map_err(fail)
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    /// Wo Konfiguration, Datenbank und Geheimnisse liegen — die haeufigste
    /// Frage beim Einrichten.
    pub data_dir: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.to_string(),
        data_dir: app
            .path()
            .app_data_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unbekannt".into()),
    })
}

/// Wird vom Deep-Link-Plugin gerufen — die App kann dabei gerade erst starten.
pub async fn handle_deep_links(app: AppHandle, urls: Vec<String>) {
    let state = app.state::<AppState>();
    for url in urls {
        if !url.contains("oauth2") {
            continue;
        }
        match state.engine.finish_gmail_auth(&url).await {
            Ok(instance) => {
                log::info!("Gmail-Konto '{instance}' angemeldet");
                let _ = app.emit("gmail-auth", instance);
            }
            Err(err) => {
                log::warn!("Gmail-Anmeldung fehlgeschlagen: {err}");
                let _ = app.emit("gmail-auth-error", err.to_string());
            }
        }
    }
}
