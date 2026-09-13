//! Nachrichten aus RSS-/Atom-Feeds, getrennt nach Regional, Welt und Wetter.
//!
//! Alle Feeds sind **eine** Quelleninstanz. Der Startscreen soll eine
//! Nachrichtenkarte zeigen, nicht zwanzig Feedkarten — und die Begrenzung auf
//! `headline_limit` wirkt nur, wenn sie ueber alle Feeds zusammen greift.
//!
//! Wetter ist bewusst auch nur ein Feed: eine Warnmeldung des DWD ist eine
//! Schlagzeile mit hoher Dringlichkeit, kein eigenes Modul. Ein echter
//! Wetter-Connector (Bright Sky) steht in `docs/roadmap-ki.md` unter "spaeter".

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use feed_rs::model::Feed as ParsedFeed;

use crate::config::{FeedTopic, FeedsSource};
use crate::error::{Error, Result};
use crate::model::{
    Action, Harvest, Overview, OverviewLine, Signal, SourceKind, SourceRef, TimeKind, Urgency,
};

use super::{shorten, Connector, SyncContext};

pub struct FeedsConnector {
    source: SourceRef,
    config: FeedsSource,
}

impl FeedsConnector {
    pub fn new(config: FeedsSource) -> Self {
        let source = SourceRef::new(SourceKind::Feeds, &config.common.instance, &config.common.label);
        Self { source, config }
    }
}

#[async_trait]
impl Connector for FeedsConnector {
    fn source(&self) -> &SourceRef {
        &self.source
    }

    async fn fetch(&self, ctx: &SyncContext<'_>) -> Result<Harvest> {
        let mut items = Vec::new();
        let mut reached = 0usize;
        let mut last_error: Option<Error> = None;

        for feed in self.config.feeds.iter().filter(|f| f.enabled) {
            match fetch_one(ctx, &feed.url).await {
                Ok(parsed) => {
                    reached += 1;
                    items.extend(entries_of(&parsed, feed.label.as_str(), feed.topic));
                }
                Err(err) => {
                    // Ein toter Feed darf die anderen neunzehn nicht mitnehmen.
                    log::warn!("Feed {} nicht erreichbar: {err}", feed.url);
                    last_error = Some(err);
                }
            }
        }

        let active = self.config.feeds.iter().filter(|f| f.enabled).count();
        if reached == 0 && active > 0 {
            return Err(last_error.unwrap_or_else(|| Error::other("Kein Feed erreichbar")));
        }

        Ok(map_items(&self.source, &self.config, items, ctx.now, reached, active))
    }
}

async fn fetch_one(ctx: &SyncContext<'_>, url: &str) -> Result<ParsedFeed> {
    let response = super::super::http::expect_ok(ctx.http.get(url).send().await?).await?;
    let bytes = response.bytes().await?;
    feed_rs::parser::parse(&bytes[..]).map_err(|e| Error::Xml(format!("{url}: {e}")))
}

/// Eine Schlagzeile, aus dem Feedformat herausgeloest.
#[derive(Debug, Clone)]
pub struct Headline {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub link: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub feed_label: String,
    pub topic: FeedTopic,
}

/// Zieht die Eintraege aus einem geparsten Feed. Ohne Netz testbar.
pub fn entries_of(parsed: &ParsedFeed, feed_label: &str, topic: FeedTopic) -> Vec<Headline> {
    parsed
        .entries
        .iter()
        .filter_map(|entry| {
            let title = entry.title.as_ref().map(|t| clean_text(&t.content))?;
            if title.is_empty() {
                return None;
            }
            let link = entry
                .links
                .iter()
                .find(|l| l.rel.as_deref() != Some("enclosure"))
                .or_else(|| entry.links.first())
                .map(|l| l.href.clone());
            Some(Headline {
                id: if entry.id.is_empty() {
                    // Ohne guid: Titel als Id, damit derselbe Beitrag beim
                    // naechsten Sync nicht als neu gilt.
                    format!("t:{}", short_hash(&title))
                } else {
                    entry.id.clone()
                },
                title,
                summary: entry
                    .summary
                    .as_ref()
                    .map(|s| clean_text(&s.content))
                    .filter(|s| !s.is_empty())
                    .map(|s| shorten(&s, 180)),
                link,
                published: entry.published.or(entry.updated),
                feed_label: feed_label.to_string(),
                topic,
            })
        })
        .collect()
}

/// Schlagzeilen -> Signale und Karte. Reine Funktion, ohne Netz.
pub fn map_items(
    source: &SourceRef,
    config: &FeedsSource,
    mut items: Vec<Headline>,
    now: DateTime<Utc>,
    reached: usize,
    active: usize,
) -> Harvest {
    let cutoff = now - Duration::hours(config.max_age_hours.max(1));
    items.retain(|h| h.published.map(|p| p >= cutoff).unwrap_or(true));
    // Neueste zuerst; Eintraege ohne Datum hinten.
    items.sort_by(|a, b| b.published.cmp(&a.published));

    // Dieselbe Meldung aus zwei Feeds: einmal zeigen.
    let mut seen_titles = HashSet::new();
    items.retain(|h| seen_titles.insert(normalise_title(&h.title)));

    let weather_count = items.iter().filter(|h| h.topic == FeedTopic::Weather).count();
    let total = items.len();

    let signals: Vec<Signal> = items
        .iter()
        .take(config.headline_limit)
        .map(|h| {
            let urgency = match h.topic {
                // Eine Wetterwarnung ist eine Warnung, keine Schlagzeile.
                FeedTopic::Weather => Urgency::High,
                _ => Urgency::Info,
            };
            Signal {
                subtitle: Some(format!("{} · {}", h.topic.label(), h.feed_label)),
                excerpt: h.summary.clone(),
                at: h.published,
                time_kind: h.published.map(|_| TimeKind::Since),
                urgency,
                badge: Some(h.feed_label.clone()),
                action: h.link.clone().map(|url| Action::OpenUrl { url }),
                dedup_key: Some(format!("news:{}", normalise_title(&h.title))),
                ..Signal::new(source.clone(), format!("feed:{}", short_hash(&h.id)), h.title.clone())
            }
        })
        .collect();

    let mut lines: Vec<OverviewLine> = Vec::new();
    for topic in [FeedTopic::Weather, FeedTopic::Local, FeedTopic::Global] {
        let n = items.iter().filter(|h| h.topic == topic).count();
        if n == 0 {
            continue;
        }
        lines.push(OverviewLine {
            text: format!("{}: {}", topic.label(), n),
            // Bewusst ohne Notiz: die neueste Schlagzeile steht als Signal
            // darunter in voller Laenge. Auf der Kachel bliebe von ihr nur ein
            // abgeschnittener Rest, der nichts sagt.
            note: None,
            urgency: Some(if topic == FeedTopic::Weather { Urgency::High } else { Urgency::Info }),
        });
    }

    let note = if reached < active {
        Some(format!("{} von {active} Feeds erreichbar", reached))
    } else if weather_count > 0 {
        Some(format!("{weather_count} Wettermeldung(en)"))
    } else {
        None
    };

    Harvest {
        overview: Overview {
            source: source.clone(),
            metric: total.to_string(),
            metric_raw: Some(total as f64),
            caption: "Schlagzeilen".into(),
            note,
            lines,
            progress: None,
        },
        source: source.clone(),
        signals,
    }
}

/// HTML-Reste und Entities aus Feedtexten entfernen. Viele Feeds liefern
/// Markup im `description`-Feld, auch wenn sie `text/plain` behaupten.
fn clean_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut depth = 0usize;
    for ch in raw.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    // Die fuenf vordefinierten Entities plus die beiden, die in Feeds
    // regelmaessig vorkommen. In einem Durchlauf, damit ein Text mit vielen
    // Entities nicht siebenmal kopiert wird.
    const ENTITIES: [(&str, &str); 7] = [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
    ];
    let mut decoded = String::with_capacity(out.len());
    let mut rest = out.as_str();
    'outer: while let Some(pos) = rest.find('&') {
        decoded.push_str(&rest[..pos]);
        for (entity, replacement) in ENTITIES {
            if rest[pos..].starts_with(entity) {
                decoded.push_str(replacement);
                rest = &rest[pos + entity.len()..];
                continue 'outer;
            }
        }
        // Unbekanntes `&` bleibt stehen — besser ein `&` zu viel als ein
        // verschluckter Textteil.
        decoded.push('&');
        rest = &rest[pos + 1..];
    }
    decoded.push_str(rest);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fuer die Entdopplung: Kleinschreibung, ohne Satzzeichen.
fn normalise_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Kurzer, stabiler Schluessel. Feed-Guids sind manchmal ganze URLs.
fn short_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cadence, Feed, SourceCommon};

    fn quelle() -> SourceRef {
        SourceRef::new(SourceKind::Feeds, "nachrichten", "Nachrichten")
    }

    fn konfig() -> FeedsSource {
        FeedsSource {
            common: SourceCommon::new("nachrichten", "Nachrichten", Cadence::minutes(30)),
            feeds: vec![Feed {
                url: "https://example.de/rss".into(),
                label: "Beispiel".into(),
                topic: FeedTopic::Global,
                enabled: true,
            }],
            headline_limit: 3,
            max_age_hours: 36,
        }
    }

    fn schlagzeile(id: &str, title: &str, topic: FeedTopic, alter_h: i64) -> Headline {
        Headline {
            id: id.into(),
            title: title.into(),
            summary: None,
            link: Some(format!("https://example.de/{id}")),
            published: Some(Utc::now() - Duration::hours(alter_h)),
            feed_label: "Beispiel".into(),
            topic,
        }
    }

    #[test]
    fn rss_wird_gelesen() {
        let xml = include_str!("../../tests/fixtures/feed_rss2.xml");
        let parsed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        let items = entries_of(&parsed, "Tagesschau", FeedTopic::Global);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Bundesrat beschliesst Haushalt");
        assert_eq!(items[0].link.as_deref(), Some("https://example.de/haushalt"));
        assert!(items[0].summary.as_deref().unwrap().starts_with("Der Bundesrat hat"));
        assert!(items[0].published.is_some());
    }

    #[test]
    fn html_wird_aus_der_zusammenfassung_entfernt() {
        let xml = include_str!("../../tests/fixtures/feed_rss2.xml");
        let parsed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        let items = entries_of(&parsed, "Tagesschau", FeedTopic::Global);
        let summary = items[1].summary.as_deref().unwrap();
        assert!(!summary.contains('<'), "Markup uebrig: {summary}");
        assert!(summary.contains("Sturmboeen"));
    }

    #[test]
    fn entities_werden_dekodiert_unbekanntes_bleibt_stehen() {
        assert_eq!(clean_text("Heise &amp; Co."), "Heise & Co.");
        assert_eq!(clean_text("5 &lt; 7 &gt; 3"), "5 < 7 > 3");
        assert_eq!(clean_text("&quot;Zitat&quot;"), "\"Zitat\"");
        assert_eq!(clean_text("Preis &euro; 20"), "Preis &euro; 20", "unbekannt bleibt");
        assert_eq!(clean_text("Meier &amp;&amp; Sohn"), "Meier && Sohn");
        assert_eq!(clean_text("<p>Text</p>"), "Text");
    }

    #[test]
    fn atom_wird_gelesen() {
        let xml = include_str!("../../tests/fixtures/feed_atom.xml");
        let parsed = feed_rs::parser::parse(xml.as_bytes()).unwrap();
        let items = entries_of(&parsed, "Heise", FeedTopic::Global);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Neue Rust-Version");
        assert_eq!(items[0].link.as_deref(), Some("https://example.de/rust"));
    }

    #[test]
    fn wetter_ist_dringlicher_als_nachrichten() {
        let harvest = map_items(
            &quelle(),
            &konfig(),
            vec![
                schlagzeile("a", "Irgendwas passiert", FeedTopic::Global, 1),
                schlagzeile("b", "Unwetterwarnung Stufe 3", FeedTopic::Weather, 2),
            ],
            Utc::now(),
            1,
            1,
        );
        let wetter = harvest.signals.iter().find(|s| s.title.starts_with("Unwetter")).unwrap();
        let nachricht = harvest.signals.iter().find(|s| s.title.starts_with("Irgendwas")).unwrap();
        assert_eq!(wetter.urgency, Urgency::High);
        assert_eq!(nachricht.urgency, Urgency::Info);
    }

    #[test]
    fn zu_alte_meldungen_fallen_heraus() {
        let harvest = map_items(
            &quelle(),
            &konfig(),
            vec![
                schlagzeile("frisch", "Heute", FeedTopic::Global, 2),
                schlagzeile("alt", "Vorletzte Woche", FeedTopic::Global, 200),
            ],
            Utc::now(),
            1,
            1,
        );
        assert_eq!(harvest.signals.len(), 1);
        assert_eq!(harvest.signals[0].title, "Heute");
    }

    #[test]
    fn begrenzt_auf_headline_limit_aber_zaehlt_alles() {
        let items: Vec<Headline> = (0..9)
            .map(|i| schlagzeile(&format!("id{i}"), &format!("Meldung {i}"), FeedTopic::Global, i))
            .collect();
        let harvest = map_items(&quelle(), &konfig(), items, Utc::now(), 1, 1);
        assert_eq!(harvest.signals.len(), 3, "headline_limit greift");
        assert_eq!(harvest.overview.metric, "9", "die Karte kennt die Gesamtzahl");
    }

    #[test]
    fn gleiche_meldung_aus_zwei_feeds_erscheint_einmal() {
        let mut a = schlagzeile("a", "Der gleiche Satz!", FeedTopic::Global, 1);
        let mut b = schlagzeile("b", "der gleiche satz", FeedTopic::Local, 1);
        a.feed_label = "Feed A".into();
        b.feed_label = "Feed B".into();
        let harvest = map_items(&quelle(), &konfig(), vec![a, b], Utc::now(), 2, 2);
        assert_eq!(harvest.signals.len(), 1);
    }

    #[test]
    fn karte_meldet_unerreichbare_feeds() {
        let harvest = map_items(
            &quelle(),
            &konfig(),
            vec![schlagzeile("a", "Eine Meldung", FeedTopic::Global, 1)],
            Utc::now(),
            2,
            5,
        );
        assert_eq!(harvest.overview.note.as_deref(), Some("2 von 5 Feeds erreichbar"));
    }

    #[test]
    fn neueste_zuerst() {
        let harvest = map_items(
            &quelle(),
            &konfig(),
            vec![
                schlagzeile("alt", "Gestern", FeedTopic::Global, 20),
                schlagzeile("neu", "Gerade", FeedTopic::Global, 1),
            ],
            Utc::now(),
            1,
            1,
        );
        assert_eq!(harvest.signals[0].title, "Gerade");
    }
}
