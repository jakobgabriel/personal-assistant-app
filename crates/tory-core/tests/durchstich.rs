//! Durchstich: echte HTTP-Aufrufe gegen einen Testserver, durch alle Schichten.
//!
//! Die Modultests pruefen die Abbildung (Antwort -> Signal) ohne Netz. Was sie
//! nicht pruefen: ob die Anfrage richtig gebaut wird — Kopfzeilen, Query,
//! Pfade — und ob am anderen Ende ein Dashboard herauskommt.
//!
//! Deshalb hier ein winziger HTTP-Server aus der Standardbibliothek, der die
//! aufgezeichneten Antworten ausliefert und **mitschreibt, was angefragt
//! wurde**. Eine falsche Kopfzeile faellt so auf, ohne dass ein echter
//! Mindwtr-, NocoDB- oder Feed-Server laufen muesste.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use tory_core::config::{
    Cadence, Config, Feed, FeedTopic, FeedsSource, MindwtrSource, NocodbSource, NocodbTableMap,
    SourceCommon,
};
use tory_core::engine::Engine;

/// Was der Server gesehen hat: Pfad -> (Query, Kopfzeilen).
type Protokoll = Arc<Mutex<Vec<Anfrage>>>;

#[derive(Debug, Clone)]
struct Anfrage {
    methode: String,
    pfad: String,
    query: String,
    kopfzeilen: HashMap<String, String>,
}

struct Testserver {
    basis: String,
    protokoll: Protokoll,
}

impl Testserver {
    fn starten() -> Self {
        // Port 0 laesst das Betriebssystem einen freien waehlen — sonst
        // kollidieren parallele Testlaeufe.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let basis = format!("http://{}", listener.local_addr().unwrap());
        let protokoll: Protokoll = Arc::new(Mutex::new(Vec::new()));
        let mitschrift = protokoll.clone();

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mitschrift = mitschrift.clone();
                thread::spawn(move || bedienen(stream, mitschrift));
            }
        });

        Self { basis, protokoll }
    }

    fn anfragen(&self) -> Vec<Anfrage> {
        self.protokoll.lock().unwrap().clone()
    }

    fn fand(&self, pfad: &str) -> Option<Anfrage> {
        self.anfragen().into_iter().find(|a| a.pfad == pfad)
    }
}

fn bedienen(mut stream: TcpStream, protokoll: Protokoll) {
    let mut leser = BufReader::new(stream.try_clone().unwrap());
    let mut startzeile = String::new();
    if leser.read_line(&mut startzeile).is_err() || startzeile.is_empty() {
        return;
    }
    let mut teile = startzeile.split_whitespace();
    let methode = teile.next().unwrap_or("").to_string();
    let ziel = teile.next().unwrap_or("/").to_string();
    let (pfad, query) = match ziel.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (ziel.clone(), String::new()),
    };

    let mut kopfzeilen = HashMap::new();
    let mut laenge = 0usize;
    loop {
        let mut zeile = String::new();
        if leser.read_line(&mut zeile).is_err() || zeile.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = zeile.split_once(':') {
            let k = k.trim().to_lowercase();
            if k == "content-length" {
                laenge = v.trim().parse().unwrap_or(0);
            }
            kopfzeilen.insert(k, v.trim().to_string());
        }
    }
    // Koerper abraeumen, sonst blockiert der Client beim naechsten Aufruf.
    if laenge > 0 {
        let mut puffer = vec![0u8; laenge];
        let _ = leser.read_exact(&mut puffer);
    }

    protokoll.lock().unwrap().push(Anfrage {
        methode: methode.clone(),
        pfad: pfad.clone(),
        query: query.clone(),
        kopfzeilen,
    });

    let (typ, koerper) = antwort(&pfad, &query);
    let kopf = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {typ}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        koerper.len()
    );
    let _ = stream.write_all(kopf.as_bytes());
    let _ = stream.write_all(koerper.as_bytes());
    let _ = stream.flush();
}

fn antwort(pfad: &str, query: &str) -> (&'static str, String) {
    match pfad {
        "/v1/tasks" => {
            // Der Connector fragt pro Status einmal; nur `next` traegt Daten,
            // damit die Zaehlung im Test eindeutig bleibt.
            if query.contains("status=next") {
                ("application/json", include_str!("fixtures/mindwtr_tasks.json").to_string())
            } else {
                ("application/json", r#"{"tasks":[],"total":0}"#.to_string())
            }
        }
        "/v1/projects" => {
            ("application/json", include_str!("fixtures/mindwtr_projects.json").to_string())
        }
        "/api/v2/tables/mtbl_bewerbungen/records" => {
            ("application/json", include_str!("fixtures/nocodb_records.json").to_string())
        }
        "/rss" => ("application/rss+xml", include_str!("fixtures/feed_rss2.xml").to_string()),
        _ => ("application/json", "{}".to_string()),
    }
}

fn konfiguration(basis: &str) -> Config {
    Config {
        mindwtr: vec![MindwtrSource {
            common: SourceCommon::new("haupt", "Mindwtr", Cadence::minutes(15)),
            base_url: basis.to_string(),
            token_key: "mindwtr.haupt.token".into(),
            statuses: vec!["inbox".into(), "next".into()],
            include_undated: false,
            horizon_days: 7,
        }],
        nocodb: vec![NocodbSource {
            common: SourceCommon::new("haupt", "NocoDB", Cadence::minutes(60)),
            base_url: basis.to_string(),
            token_key: "nocodb.haupt.token".into(),
            tables: vec![NocodbTableMap {
                table_id: "mtbl_bewerbungen".into(),
                label: "Bewerbungen".into(),
                view_id: None,
                title_field: "Position".into(),
                subtitle_field: Some("Notiz".into()),
                date_field: Some("Frist".into()),
                status_field: Some("Status".into()),
                done_values: vec!["Abgelehnt".into()],
                filter: None,
                limit: 100,
            }],
        }],
        feeds: vec![FeedsSource {
            common: SourceCommon::new("nachrichten", "Nachrichten", Cadence::minutes(30)),
            feeds: vec![Feed {
                url: format!("{basis}/rss"),
                label: "Beispiel".into(),
                topic: FeedTopic::Global,
                enabled: true,
            }],
            headline_limit: 10,
            // Die Fixture traegt feste Daten aus dem September 2026; ohne einen
            // grosszuegigen Rahmen fielen sie irgendwann aus dem Hoechstalter.
            max_age_hours: 24 * 365 * 20,
        }],
        ..Config::default()
    }
}

fn verzeichnis(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tory-durchstich-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[tokio::test]
async fn drei_quellen_vom_server_bis_auf_den_startscreen() {
    let server = Testserver::starten();
    let dir = verzeichnis("voll");
    let engine = Engine::open(&dir).unwrap();

    engine.set_tz_offset(120).await;
    assert!(engine.save_config(konfiguration(&server.basis)).await.unwrap().is_empty());
    engine.set_secret("mindwtr.haupt.token", "token-m").await.unwrap();
    engine.set_secret("nocodb.haupt.token", "token-n").await.unwrap();

    let bericht = engine.sync(true).await.unwrap();
    assert!(bericht.failed.is_empty(), "Fehlversuche: {:?}", bericht.failed);
    assert_eq!(bericht.synced.len(), 3, "drei Quellen: {:?}", bericht.synced);

    let dash = engine.dashboard().await.unwrap();
    assert_eq!(dash.cards.len(), 3);
    assert!(dash.alerts.is_empty(), "kein Banner bei Erfolg: {:?}", dash.alerts);

    // Jede Karte hat eine Uebersicht und einen erfolgreichen Sync-Stand.
    for karte in &dash.cards {
        assert!(karte.overview.is_some(), "{} ohne Uebersicht", karte.source.id());
        assert!(karte.sync.last_ok.is_some(), "{} ohne last_ok", karte.source.id());
        assert!(karte.sync.fault.is_none());
    }

    // Signale aus allen drei Quellen sind da und quellenuebergreifend sortiert.
    let alle: Vec<_> = dash.top.iter().chain(dash.rest.iter()).collect();
    for erwartet in ["mindwtr:haupt", "nocodb:haupt", "feeds:nachrichten"] {
        assert!(alle.iter().any(|s| s.source.id() == erwartet), "{erwartet} fehlt");
    }
    let raenge: Vec<u8> = alle.iter().map(|s| s.urgency.rank()).collect();
    assert!(raenge.windows(2).all(|w| w[0] <= w[1]), "nicht sortiert: {raenge:?}");

    // Die Schlagzeile ist Info, die ueberfaellige Bewerbungsfrist kritisch —
    // also steht die Frist oben und nicht die Nachricht.
    assert!(!dash.top.is_empty());
    assert_ne!(dash.top[0].source.kind, tory_core::model::SourceKind::Feeds);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn anfragen_tragen_die_richtige_anmeldung() {
    let server = Testserver::starten();
    let dir = verzeichnis("auth");
    let engine = Engine::open(&dir).unwrap();
    engine.save_config(konfiguration(&server.basis)).await.unwrap();
    engine.set_secret("mindwtr.haupt.token", "token-m").await.unwrap();
    engine.set_secret("nocodb.haupt.token", "token-n").await.unwrap();
    engine.sync(true).await.unwrap();

    let tasks = server.fand("/v1/tasks").expect("Mindwtr wurde nicht gefragt");
    assert_eq!(tasks.methode, "GET");
    assert_eq!(tasks.kopfzeilen.get("authorization").map(String::as_str), Some("Bearer token-m"));
    assert!(tasks.query.contains("status="), "Status fehlt: {}", tasks.query);
    assert!(tasks.query.contains("limit="));

    let noco = server.fand("/api/v2/tables/mtbl_bewerbungen/records").expect("NocoDB nicht gefragt");
    assert_eq!(noco.kopfzeilen.get("xc-token").map(String::as_str), Some("token-n"));
    // Ohne Sortierung nach der Datumsspalte schneidet `limit` willkuerlich ab.
    assert!(noco.query.contains("sort=Frist"), "keine Sortierung: {}", noco.query);

    // Und: die Basis-URL wird nicht doppelt mit /v1 versehen.
    assert!(server.fand("/v1/v1/tasks").is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn fehlendes_token_meldet_konfigurationsfehler_und_verschont_die_anderen() {
    let server = Testserver::starten();
    let dir = verzeichnis("kein-token");
    let engine = Engine::open(&dir).unwrap();
    engine.save_config(konfiguration(&server.basis)).await.unwrap();
    // Nur NocoDB bekommt sein Token.
    engine.set_secret("nocodb.haupt.token", "token-n").await.unwrap();

    let bericht = engine.sync(true).await.unwrap();
    assert_eq!(bericht.failed.len(), 1, "nur Mindwtr faellt aus: {bericht:?}");
    assert_eq!(bericht.failed[0].source, "mindwtr:haupt");
    assert_eq!(bericht.synced.len(), 2, "die anderen laufen weiter");

    let dash = engine.dashboard().await.unwrap();
    let banner = dash.alerts.iter().find(|a| a.source.id() == "mindwtr:haupt").unwrap();
    assert!(banner.needs_action, "fehlendes Token heilt nicht durch Warten");

    // Ein zweiter Sync ohne `force` versucht es nicht sofort erneut.
    let zweiter = engine.sync(false).await.unwrap();
    assert!(zweiter.skipped.contains(&"mindwtr:haupt".to_string()), "{zweiter:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn unerreichbarer_server_laesst_die_alten_zahlen_stehen() {
    let server = Testserver::starten();
    let dir = verzeichnis("offline");
    let engine = Engine::open(&dir).unwrap();
    engine.save_config(konfiguration(&server.basis)).await.unwrap();
    engine.set_secret("nocodb.haupt.token", "token-n").await.unwrap();
    engine.sync(true).await.unwrap();

    let vorher = engine.dashboard().await.unwrap();
    let noco_vorher = vorher.cards.iter().find(|c| c.source.id() == "nocodb:haupt").unwrap();
    let zahl = noco_vorher.overview.as_ref().unwrap().metric.clone();
    assert_eq!(zahl, "3");

    // Jetzt auf einen Port zeigen, auf dem nichts horcht.
    let mut kaputt = konfiguration(&server.basis);
    kaputt.nocodb[0].base_url = "http://127.0.0.1:1".into();
    engine.save_config(kaputt).await.unwrap();
    let bericht = engine.sync(true).await.unwrap();
    assert!(bericht.failed.iter().any(|f| f.source == "nocodb:haupt"));

    // Die Karte zeigt weiter den letzten Stand — nur eben als alten.
    let nachher = engine.dashboard().await.unwrap();
    let noco = nachher.cards.iter().find(|c| c.source.id() == "nocodb:haupt").unwrap();
    assert_eq!(noco.overview.as_ref().unwrap().metric, zahl, "alte Zahlen bleiben lesbar");
    assert!(noco.sync.fault.is_some(), "aber als fehlerhaft gekennzeichnet");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn wegwischen_ueberlebt_den_naechsten_sync() {
    let server = Testserver::starten();
    let dir = verzeichnis("wegwischen");
    let engine = Engine::open(&dir).unwrap();
    engine.save_config(konfiguration(&server.basis)).await.unwrap();
    engine.set_secret("nocodb.haupt.token", "token-n").await.unwrap();
    engine.sync(true).await.unwrap();

    let signale = engine.signals_of("nocodb:haupt").await.unwrap();
    assert_eq!(signale.len(), 3);
    let key = signale[0].key();

    engine.dismiss(&key).await.unwrap();
    assert_eq!(engine.signals_of("nocodb:haupt").await.unwrap().len(), 2);

    // Dieselben Zeilen kommen beim naechsten Sync wieder — die Marke bleibt.
    engine.sync(true).await.unwrap();
    assert_eq!(engine.signals_of("nocodb:haupt").await.unwrap().len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}
