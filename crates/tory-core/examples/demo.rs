//! Beispieldaten zum Ausprobieren — ohne einen einzigen echten Dienst.
//!
//! Startet einen kleinen HTTP-Server, der sich wie Mindwtr, NocoDB und ein
//! Nachrichtenfeed verhaelt, legt daneben einen Obsidian-Vault aus echten
//! Markdown-Dateien an und schreibt eine Konfiguration, die auf beides zeigt.
//! Danach zeigt Tory einen vollen Startscreen, ohne dass irgendwo ein Server
//! laufen muss.
//!
//! ```text
//! cargo run -p tory-core --example demo -- /tmp/tory-demo 8799
//! XDG_DATA_HOME=/tmp/tory-demo ./target/debug/tory
//! ```
//!
//! Alle Datumsangaben entstehen relativ zu *heute*. Feste Daten in einer
//! Beispielmenge veralten, und ein Startscreen, auf dem alles ueberfaellig ist,
//! zeigt nicht, was er zeigen soll.
//!
//! Gmail fehlt bewusst: die API-Adresse steht fest im Connector, und sie fuer
//! Beispieldaten umleitbar zu machen hiesse, eine Hintertuer einzubauen, die im
//! fertigen Programm niemand braucht.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use chrono::{Duration, Local, Utc};
use tory_core::config::{
    Cadence, Config, DashboardConfig, Feed, FeedTopic, FeedsSource, MindwtrAccess, MindwtrSource,
    NocodbSource, NocodbTableMap, ObsidianSource, SourceCommon, VaultAccess,
};
use tory_core::secrets::SecretStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let wurzel: PathBuf = args.next().unwrap_or_else(|| "/tmp/tory-demo".into()).into();
    let port: u16 = args.next().unwrap_or_else(|| "8799".into()).parse()?;

    let daten = wurzel.join("de.tory.app");
    let vault = wurzel.join("vault-privat");
    std::fs::create_dir_all(&daten)?;

    vault_anlegen(&vault)?;
    let basis = format!("http://127.0.0.1:{port}");
    konfiguration_schreiben(&daten, &vault, &basis)?;

    println!("Beispieldaten liegen bereit.");
    println!("  Datenverzeichnis : {}", daten.display());
    println!("  Vault            : {}", vault.display());
    println!("  Dienste          : {basis}");
    println!();
    println!("Jetzt starten mit:");
    println!("  XDG_DATA_HOME={} ./target/debug/tory", wurzel.display());
    println!();

    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("Server laeuft auf {basis} — mit Strg-C beenden.");
    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || {
            if let Err(err) = bedienen(stream) {
                eprintln!("Anfrage fehlgeschlagen: {err}");
            }
        });
    }
    Ok(())
}

/// Ein kleiner Vault mit den Dingen, die Tory daraus liest: Aufgaben mit Frist
/// im Emoji- und im Dataview-Format, und eine angepinnte Notiz.
fn vault_anlegen(vault: &Path) -> std::io::Result<()> {
    let heute = Local::now().date_naive();
    let tag = |n: i64| (heute + Duration::days(n)).format("%Y-%m-%d").to_string();

    std::fs::create_dir_all(vault.join("Projekte"))?;
    std::fs::create_dir_all(vault.join("Merker"))?;
    std::fs::create_dir_all(vault.join(".obsidian"))?;
    // Liegt im ausgeschlossenen Ordner und darf deshalb nicht auftauchen.
    std::fs::write(vault.join(".obsidian/workspace.md"), "- [ ] interner Kram\n")?;

    std::fs::write(
        vault.join("Projekte/Haus.md"),
        format!(
            "---\ntitle: Haus\ntags: [projekt, wohnen]\n---\n\n\
             Sammelnotiz fuer alles rund ums Haus.\n\n\
             - [ ] Angebot Dachdecker gegenlesen \u{1F4C5} {} #handwerk\n\
             - [ ] Heizung entlueften \u{1F4C5} {}\n\
             - [x] Rauchmelder geprueft \u{1F4C5} {}\n\
             - [ ] Irgendwann die Garage sortieren\n",
            tag(0),
            tag(2),
            tag(-9)
        ),
    )?;

    std::fs::write(
        vault.join("Projekte/Steuer.md"),
        format!(
            "# Steuer 2025\n\nBelege liegen im Ordner Ablage.\n\n\
             - [ ] Fahrtkosten nachtragen due:: {} \u{23EB}\n\
             - [ ] Handwerkerrechnungen sortieren due:: {}\n",
            tag(-2),
            tag(5)
        ),
    )?;

    std::fs::write(
        vault.join("Merker/Umzug.md"),
        "---\ntitle: Umzug Checkliste\ntags: [merker, wohnen]\n---\n\n\
         Kartons bestellen, Nachsendeauftrag stellen, Strom ummelden.\n",
    )?;

    Ok(())
}

fn konfiguration_schreiben(daten: &Path, vault: &Path, basis: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config {
        dashboard: DashboardConfig { top_signals: 3, ..DashboardConfig::default() },
        obsidian: vec![ObsidianSource {
            common: SourceCommon { order: 0, ..SourceCommon::new("privat", "Vault Privat", Cadence::minutes(30)) },
            vault_name: "Privat".into(),
            access: VaultAccess::Local { path: vault.display().to_string() },
            include_folders: Vec::new(),
            exclude_folders: vec![".obsidian".into(), ".trash".into()],
            read_tasks: true,
            pinned_tags: vec!["merker".into()],
            scan_limit: 400,
        }],
        mindwtr: vec![MindwtrSource {
            common: SourceCommon { order: 1, ..SourceCommon::new("haupt", "Mindwtr", Cadence::minutes(15)) },
            access: MindwtrAccess::Cloud {
                base_url: basis.into(),
                token_key: "mindwtr.haupt.token".into(),
            },
            statuses: vec!["inbox".into(), "next".into(), "waiting".into()],
            include_undated: false,
            horizon_days: 7,
        }],
        nocodb: vec![NocodbSource {
            common: SourceCommon { order: 2, ..SourceCommon::new("haupt", "NocoDB", Cadence::minutes(60)) },
            base_url: basis.into(),
            token_key: "nocodb.haupt.token".into(),
            tables: vec![NocodbTableMap {
                table_id: "mtbl_bewerbungen".into(),
                label: "Bewerbungen".into(),
                view_id: None,
                title_field: "Position".into(),
                subtitle_field: Some("Notiz".into()),
                date_field: Some("Frist".into()),
                status_field: Some("Status".into()),
                done_values: vec!["Abgelehnt".into(), "Zurueckgezogen".into()],
                filter: None,
                limit: 100,
            }],
        }],
        feeds: vec![FeedsSource {
            common: SourceCommon { order: 3, ..SourceCommon::new("nachrichten", "Nachrichten", Cadence::minutes(30)) },
            feeds: vec![
                Feed { url: format!("{basis}/rss/welt"), label: "Tagesschau".into(), topic: FeedTopic::Global, enabled: true },
                Feed { url: format!("{basis}/rss/regional"), label: "Stadtnachrichten".into(), topic: FeedTopic::Local, enabled: true },
                Feed { url: format!("{basis}/rss/wetter"), label: "DWD".into(), topic: FeedTopic::Weather, enabled: true },
            ],
            headline_limit: 6,
            max_age_hours: 36,
        }],
        ..Config::default()
    };

    let probleme = config.validate();
    assert!(probleme.is_empty(), "Beispielkonfiguration ist fehlerhaft: {probleme:?}");
    std::fs::write(daten.join("config.json"), serde_json::to_string_pretty(&config)?)?;

    let mut secrets = SecretStore::open(&daten.join("secrets"))?;
    secrets.set("mindwtr.haupt.token", "beispiel-token")?;
    secrets.set("nocodb.haupt.token", "beispiel-token")?;
    Ok(())
}

// --- Der Server ------------------------------------------------------------

fn bedienen(mut stream: TcpStream) -> std::io::Result<()> {
    let mut leser = BufReader::new(stream.try_clone()?);
    let mut startzeile = String::new();
    leser.read_line(&mut startzeile)?;
    let ziel = startzeile.split_whitespace().nth(1).unwrap_or("/").to_string();
    let (pfad, query) = ziel.split_once('?').unwrap_or((ziel.as_str(), ""));
    let (pfad, query) = (pfad.to_string(), query.to_string());

    let mut laenge = 0usize;
    loop {
        let mut zeile = String::new();
        if leser.read_line(&mut zeile)? == 0 || zeile.trim().is_empty() {
            break;
        }
        if let Some(v) = zeile.to_lowercase().strip_prefix("content-length:") {
            laenge = v.trim().parse().unwrap_or(0);
        }
    }
    if laenge > 0 {
        let mut puffer = vec![0u8; laenge];
        leser.read_exact(&mut puffer)?;
    }

    let (typ, koerper) = antwort(&pfad, &query);
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {typ}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        koerper.len()
    )?;
    stream.write_all(koerper.as_bytes())?;
    stream.flush()
}

fn antwort(pfad: &str, query: &str) -> (&'static str, String) {
    match pfad {
        "/v1/tasks" if query.contains("status=next") => ("application/json", mindwtr_naechste()),
        "/v1/tasks" if query.contains("status=inbox") => ("application/json", mindwtr_eingang()),
        "/v1/tasks" => ("application/json", r#"{"tasks":[],"total":0}"#.into()),
        "/v1/projects" => ("application/json", mindwtr_projekte()),
        // Dieselben Daten als die Datei, die Mindwtrs WebDAV-Sync ablegt.
        "/dav/Mindwtr/data.json" => ("application/json", mindwtr_datei()),
        "/api/v2/tables/mtbl_bewerbungen/records" => ("application/json", nocodb_zeilen()),
        "/rss/welt" => ("application/rss+xml", feed_welt()),
        "/rss/regional" => ("application/rss+xml", feed_regional()),
        "/rss/wetter" => ("application/rss+xml", feed_wetter()),
        _ => ("application/json", "{}".into()),
    }
}

/// ISO-Datum relativ zu heute.
fn tag(versatz: i64) -> String {
    (Local::now().date_naive() + Duration::days(versatz)).format("%Y-%m-%d").to_string()
}

/// RFC-2822-Zeitstempel vor `stunden` Stunden — das Format, das RSS erwartet.
fn vor_stunden(stunden: i64) -> String {
    (Utc::now() - Duration::hours(stunden)).to_rfc2822()
}

fn mindwtr_naechste() -> String {
    format!(
        r#"{{"tasks":[
        {{"id":"t1","title":"Angebot Dachdecker gegenlesen","status":"next","priority":"high",
          "dueDate":"{}","projectId":"p-haus","tags":["handwerk"],"contexts":["@schreibtisch"],
          "description":"Zwei Angebote vergleichen, Termin bestaetigen.",
          "createdAt":"2026-08-01T10:00:00Z","updatedAt":"2026-09-01T10:00:00Z"}},
        {{"id":"t2","title":"Werkstatt anrufen wegen Inspektion","status":"next",
          "isFocusedToday":true,"tags":[],"contexts":["@telefon"],
          "createdAt":"2026-09-01T06:00:00Z","updatedAt":"2026-09-01T06:00:00Z"}},
        {{"id":"t3","title":"Zahnarzttermin verschieben","status":"next","dueDate":"{}",
          "projectId":"p-alltag","tags":[],"contexts":["@telefon"],
          "createdAt":"2026-09-01T10:00:00Z","updatedAt":"2026-09-01T10:00:00Z"}},
        {{"id":"t4","title":"Geschenk fuer Anna besorgen","status":"next","dueDate":"{}",
          "projectId":"p-alltag","tags":[],"contexts":[],
          "createdAt":"2026-09-01T10:00:00Z","updatedAt":"2026-09-01T10:00:00Z"}},
        {{"id":"t5","title":"Backup-Platte pruefen","status":"next","dueDate":"{}",
          "tags":[],"contexts":[],
          "createdAt":"2026-09-01T10:00:00Z","updatedAt":"2026-09-01T10:00:00Z"}}
        ],"total":31,"limit":200,"offset":0}}"#,
        tag(-1),
        tag(1),
        tag(3),
        tag(20)
    )
}

fn mindwtr_eingang() -> String {
    format!(
        r#"{{"tasks":[
        {{"id":"i1","title":"Strom ummelden","status":"inbox","dueDate":"{}","tags":[],"contexts":[],
          "createdAt":"2026-09-01T10:00:00Z","updatedAt":"2026-09-01T10:00:00Z"}},
        {{"id":"i2","title":"Idee: Regal fuer die Werkstatt","status":"inbox","tags":[],"contexts":[],
          "createdAt":"2026-09-01T10:00:00Z","updatedAt":"2026-09-01T10:00:00Z"}}
        ],"total":9,"limit":200,"offset":0}}"#,
        tag(2)
    )
}

fn mindwtr_projekte() -> String {
    // `r##"…"##`: die Farbwerte enthalten `"#`, was ein `r#"`-Literal beenden wuerde.
    r##"{"projects":[
    {"id":"p-haus","title":"Haus","status":"active","color":"#1B6B4C","order":1,"tagIds":[],
     "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"},
    {"id":"p-alltag","title":"Alltag","status":"active","color":"#3A4CA0","order":2,"tagIds":[],
     "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}
    ],"total":2,"limit":200,"offset":0}"##
        .into()
}

/// Die `data.json` des WebDAV-Syncs: dieselben Werte, nur ohne Umschlag.
///
/// Zusammengesetzt aus denselben Quellen wie die REST-Antworten — so zeigt das
/// Beispiel beide Wege mit identischem Inhalt, und ein Unterschied in der
/// Anzeige waere ein echter Fehler und kein Datenartefakt.
fn mindwtr_datei() -> String {
    let aufgaben = |roh: &str| -> Vec<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(roh)
            .ok()
            .and_then(|v| v.get("tasks").and_then(|t| t.as_array().cloned()))
            .unwrap_or_default()
    };
    let mut tasks = aufgaben(&mindwtr_naechste());
    tasks.extend(aufgaben(&mindwtr_eingang()));

    let projects = serde_json::from_str::<serde_json::Value>(&mindwtr_projekte())
        .ok()
        .and_then(|v| v.get("projects").and_then(|p| p.as_array().cloned()))
        .unwrap_or_default();

    serde_json::json!({
        "tasks": tasks,
        "projects": projects,
        "sections": [],
        "areas": [],
        "settings": {},
    })
    .to_string()
}

fn nocodb_zeilen() -> String {
    format!(
        r#"{{"list":[
        {{"Id":12,"Position":"Data Engineer (IIoT)","Firma":{{"Id":3,"title":"Beispiel GmbH"}},
          "Status":"Beworben","Frist":"{}","Notiz":"Nachfassen, seit drei Wochen keine Antwort."}},
        {{"Id":13,"Position":"IIoT Platform Engineer","Firma":{{"Id":4,"title":"Nordwerk AG"}},
          "Status":"Interview","Frist":"{}","Notiz":"Technisches Gespraech vorbereiten."}},
        {{"Id":14,"Position":"Lean Manager Produktion","Firma":{{"Id":5,"title":"Musterwerke AG"}},
          "Status":"Beworben","Frist":"{}","Notiz":null}},
        {{"Id":15,"Position":"Werkstudent Digitalisierung","Firma":{{"Id":6,"title":"Alt AG"}},
          "Status":"Abgelehnt","Frist":"{}","Notiz":"Absage per Mail."}}
        ],"pageInfo":{{"totalRows":4,"page":1,"pageSize":100,"isFirstPage":true,"isLastPage":true}}}}"#,
        tag(-3),
        tag(4),
        tag(32),
        tag(-40)
    )
}

fn rss(titel: &str, eintraege: &[(&str, &str, i64)]) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\"><channel>\n\
         <title>{titel}</title><link>https://example.de</link><description>Beispiel</description>\n"
    );
    for (i, (schlagzeile, text, stunden)) in eintraege.iter().enumerate() {
        xml.push_str(&format!(
            "<item><title>{schlagzeile}</title><link>https://example.de/{i}</link>\
             <guid isPermaLink=\"false\">urn:beispiel:{titel}:{i}</guid>\
             <pubDate>{}</pubDate><description><![CDATA[{text}]]></description></item>\n",
            vor_stunden(*stunden)
        ));
    }
    xml.push_str("</channel></rss>\n");
    xml
}

fn feed_welt() -> String {
    rss(
        "Tagesschau",
        &[
            ("Bundesrat beschliesst Haushalt", "<p>Der Bundesrat hat dem Haushalt zugestimmt.</p>", 3),
            ("Tarifrunde geht in die dritte Verhandlung", "Die Gewerkschaft fordert sieben Prozent.", 7),
            ("Neue Regeln fuer Heizungen ab Januar", "Was sich fuer Eigentuemer aendert.", 14),
        ],
    )
}

fn feed_regional() -> String {
    rss(
        "Stadtnachrichten",
        &[
            ("Sperrung der Nordbruecke ab Montag", "Umleitung ueber die Ringstrasse.", 5),
            ("Neuer Wochenmarkt am Hafen", "Ab Samstag, zunaechst probeweise.", 26),
        ],
    )
}

fn feed_wetter() -> String {
    rss(
        "DWD Warnlage",
        &[("Amtliche Warnung vor Sturmboeen", "Boeen bis 90 km/h, ab 16 Uhr, bis morgen frueh.", 1)],
    )
}
