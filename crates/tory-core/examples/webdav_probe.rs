//! Prueft eine WebDAV-Anbindung und sagt, woran es liegt.
//!
//! Gedacht fuer den Fall, der sich sonst nur als rotes Banner auf dem Telefon
//! zeigt: die Adresse stimmt nicht, der Server will etwas anderes, oder die
//! Antwort sieht anders aus als erwartet. Hier laeuft derselbe Code wie in der
//! App, nur mit allem, was er unterwegs sieht, auf der Konsole.
//!
//! ```text
//! cargo run -p tory-core --example webdav_probe -- \
//!     https://cloud.example.de/remote.php/dav/files/jakob/Vault jakob PASSWORT
//! ```

use tory_core::config::{Cadence, ObsidianSource, SourceCommon, VaultAccess};
use tory_core::connectors::{Connector, SyncContext};
use tory_core::secrets::SecretStore;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (url, user, pass) = match (args.next(), args.next(), args.next()) {
        (Some(u), Some(n), Some(p)) => (u, n, p),
        _ => {
            eprintln!("Aufruf: webdav_probe <URL> <Benutzer> <Passwort>");
            std::process::exit(2);
        }
    };

    let dir = std::env::temp_dir().join(format!("tory-probe-{}", std::process::id()));
    let mut secrets = SecretStore::open(&dir)?;
    secrets.set("probe", &pass)?;
    let http = tory_core::http::client()?;

    let config = ObsidianSource {
        common: SourceCommon::new("probe", "Probe", Cadence::minutes(30)),
        vault_name: "Probe".into(),
        access: VaultAccess::Webdav {
            base_url: url.clone(),
            username: user,
            password_key: "probe".into(),
        },
        include_folders: Vec::new(),
        exclude_folders: vec![".obsidian".into(), ".trash".into()],
        read_tasks: true,
        pinned_tags: vec!["merker".into()],
        scan_limit: 50,
    };

    println!("Vault:  {url}");
    let ctx = SyncContext {
        http: &http,
        secrets: &secrets,
        now: chrono::Utc::now(),
        tz_offset_minutes: 0,
    };

    let connector = tory_core::connectors::obsidian::ObsidianConnector::new(config);
    match connector.fetch(&ctx).await {
        Ok(ernte) => {
            println!("\nGelesen: {}", ernte.overview.metric);
            for zeile in &ernte.overview.lines {
                println!("  · {}", zeile.text);
            }
            println!("\n{} Signale:", ernte.signals.len());
            for s in ernte.signals.iter().take(20) {
                println!("  · {}  [{}]", s.title, s.subtitle.as_deref().unwrap_or(""));
            }
            if ernte.signals.is_empty() {
                println!("  (keine — liegen im Vault Aufgaben mit Datum?)");
            }
        }
        Err(err) => {
            println!("\nFehlgeschlagen.");
            println!("  Meldung: {err}");
            println!("  Einordnung: {:?}", err.as_fault());
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(1);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
