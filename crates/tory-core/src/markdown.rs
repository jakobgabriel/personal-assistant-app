//! Genau so viel Markdown-Verstaendnis, wie ein Vault-Signal braucht.
//!
//! Kein vollstaendiger Parser: Tory liest YAML-Frontmatter oberflaechlich,
//! offene Checkboxen und die Datumsangaben, die in Obsidian ueblich sind — das
//! Emoji-Format des Tasks-Plugins (`📅 2026-09-12`) und die Dataview-Form
//! (`due:: 2026-09-12`). Alles Weitere gehoert in Obsidian, nicht hierher.

use chrono::NaiveDate;

/// Eine offene Checkbox aus einer Notiz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultTask {
    /// Zeile ohne `- [ ] ` und ohne die erkannten Datumsmarken.
    pub text: String,
    pub due: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    /// `#tag` aus der Zeile.
    pub tags: Vec<String>,
    /// Zeilennummer, 1-basiert — macht die Aufgabe innerhalb der Notiz eindeutig.
    pub line: usize,
    /// `!`-Prioritaet des Tasks-Plugins: `⏫`/`🔼` heben, `🔽` senken.
    pub high_priority: bool,
}

/// Was in einer Notiz steht, soweit Tory es braucht.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Note {
    /// `title:` aus dem Frontmatter, sonst die erste `#`-Zeile.
    pub title: Option<String>,
    /// Flache Frontmatter-Werte. Listen werden zu kommagetrennten Strings.
    pub front_matter: Vec<(String, String)>,
    /// Tags aus Frontmatter (`tags:`) und Text (`#tag`), ohne `#`.
    pub tags: Vec<String>,
    pub open_tasks: Vec<VaultTask>,
    /// Erste Zeilen Fliesstext fuer die Vorschau.
    pub excerpt: Option<String>,
}

/// Liest eine Notiz. Robust gegen fehlendes Frontmatter und CRLF.
pub fn parse_note(raw: &str) -> Note {
    let raw = raw.replace("\r\n", "\n");
    let (front, body, body_offset) = split_front_matter(&raw);
    let mut note = Note::default();

    for (key, value) in parse_front_matter(front) {
        if key == "title" && note.title.is_none() {
            note.title = Some(value.clone());
        }
        if key == "tags" || key == "tag" {
            note.tags.extend(
                value
                    .split(',')
                    .map(|t| t.trim().trim_start_matches('#').to_string())
                    .filter(|t| !t.is_empty()),
            );
        }
        note.front_matter.push((key, value));
    }

    let mut excerpt_lines: Vec<String> = Vec::new();
    let mut in_code = false;
    for (idx, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if note.title.is_none() {
            if let Some(rest) = trimmed.strip_prefix("# ") {
                note.title = Some(rest.trim().to_string());
                continue;
            }
        }
        if let Some(task) = parse_task_line(trimmed, body_offset + idx + 1) {
            note.tags.extend(task.tags.iter().cloned());
            note.open_tasks.push(task);
            continue;
        }
        if is_prose(trimmed) && excerpt_lines.len() < 3 {
            excerpt_lines.push(trimmed.to_string());
        }
        note.tags.extend(inline_tags(trimmed));
    }

    if !excerpt_lines.is_empty() {
        note.excerpt = Some(excerpt_lines.join(" "));
    }
    note.tags.sort();
    note.tags.dedup();
    note
}

/// Trennt `---`-Frontmatter ab. Gibt Frontmatter, Rest und die Zeilenzahl
/// zurueck, die der Rest im Original versetzt ist.
fn split_front_matter(raw: &str) -> (&str, &str, usize) {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return ("", raw, 0);
    };
    match rest.find("\n---") {
        Some(end) => {
            let front = &rest[..end];
            // Ueber die Schlusszeile hinweg; danach kann ein \n folgen oder nicht.
            let after = rest[end + 4..].strip_prefix('\n').unwrap_or(&rest[end + 4..]);
            let offset = front.lines().count() + 2;
            (front, after, offset)
        }
        // Unabgeschlossenes Frontmatter: als Text behandeln, nicht verschlucken.
        None => ("", raw, 0),
    }
}

/// Liest `schluessel: wert`-Zeilen. Bewusst kein YAML-Parser: Vaults enthalten
/// Frontmatter, das kein YAML-Parser mag, und Tory braucht nur flache Werte.
fn parse_front_matter(front: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut pending_list: Option<String> = None;
    for line in front.lines() {
        let trimmed = line.trim_end();
        if let Some(item) = trimmed.trim_start().strip_prefix("- ") {
            // Listenform:  tags:\n  - projekt\n  - offen
            if let Some(key) = &pending_list {
                if let Some(slot) = out.iter_mut().find(|(k, _)| k == key) {
                    if slot.1.is_empty() {
                        slot.1 = item.trim().to_string();
                    } else {
                        slot.1.push_str(", ");
                        slot.1.push_str(item.trim());
                    }
                }
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        let value = value.trim_start_matches('[').trim_end_matches(']').trim();
        out.push((key.clone(), value.to_string()));
        pending_list = if value.is_empty() { Some(key) } else { None };
    }
    out
}

/// Erkennt `- [ ] …`, `* [ ] …` und `1. [ ] …`. Abgehaktes (`[x]`, `[-]`)
/// liefert `None` — Tory zeigt nur Offenes.
fn parse_task_line(line: &str, line_no: usize) -> Option<VaultTask> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
        .or_else(|| {
            let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
            (!digits.is_empty())
                .then(|| line[digits.len()..].strip_prefix(". "))
                .flatten()
        })?;
    let rest = rest.strip_prefix("[ ]")?.trim_start();

    let mut task = VaultTask {
        text: String::new(),
        due: None,
        scheduled: None,
        tags: inline_tags(rest),
        line: line_no,
        high_priority: rest.contains('\u{23EB}') || rest.contains('\u{1F53C}'),
    };

    let mut text = rest.to_string();
    for (marker, slot) in [
        ("\u{1F4C5}", 0u8), // 📅 due
        ("due::", 0),
        ("\u{23F3}", 1), // ⏳ scheduled
        ("\u{1F6EB}", 1), // 🛫 start, praktisch wie scheduled
        ("scheduled::", 1),
    ] {
        if let Some((date, cleaned)) = take_date_after(&text, marker) {
            if slot == 0 {
                task.due = task.due.or(Some(date));
            } else {
                task.scheduled = task.scheduled.or(Some(date));
            }
            text = cleaned;
        }
    }

    // Prioritaets- und Wiederholungszeichen des Tasks-Plugins: hoch, hoeher,
    // niedrig, wiederkehrend. Sie sind Metadaten, kein Aufgabentext.
    let text = text.replace(['\u{23EB}', '\u{1F53C}', '\u{1F53D}', '\u{1F501}'], "");
    // Inline-Tags stehen bereits in `tags`. Bleiben sie zusaetzlich im Text,
    // ist der Titel unnoetig laut — und, schlimmer, dieselbe Aufgabe aus
    // Obsidian und Mindwtr bekommt zwei verschiedene Entdopplungsschluessel
    // und erscheint zweimal auf dem Startscreen.
    task.text = text
        .split_whitespace()
        .filter(|wort| !ist_tag(wort))
        .collect::<Vec<_>>()
        .join(" ");
    (!task.text.is_empty()).then_some(task)
}

/// Findet `marker JJJJ-MM-TT` und gibt Datum plus Text ohne diesen Teil zurueck.
fn take_date_after(text: &str, marker: &str) -> Option<(NaiveDate, String)> {
    let pos = text.find(marker)?;
    let after = &text[pos + marker.len()..];
    let candidate: String = after.trim_start().chars().take(10).collect();
    let date = NaiveDate::parse_from_str(&candidate, "%Y-%m-%d").ok()?;
    let consumed = after.len() - after.trim_start().len() + 10;
    let mut cleaned = String::with_capacity(text.len());
    cleaned.push_str(&text[..pos]);
    cleaned.push(' ');
    cleaned.push_str(&after[consumed..]);
    Some((date, cleaned))
}

/// `#tag` im Text. Nicht in Links, nicht `#` am Zeilenanfang (Ueberschrift).
fn inline_tags(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in line.split_whitespace() {
        let Some(tag) = token.strip_prefix('#') else { continue };
        let tag: String = tag
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
            .collect();
        if tag.len() >= 2 && tag.chars().next().is_some_and(|c| !c.is_ascii_digit()) {
            out.push(tag);
        }
    }
    out
}

/// Ob ein Wort ein Tag ist, das [`inline_tags`] bereits eingesammelt hat.
/// Dieselbe Regel, damit Text und Tagliste nicht auseinanderlaufen.
fn ist_tag(wort: &str) -> bool {
    wort.strip_prefix('#').is_some_and(|rest| {
        let tag: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
            .collect();
        // Genau die Bedingung aus `inline_tags`, und der Rest muss leer sein:
        // "#42" ist kein Tag, "#tag." ebenso wenig ein sauberes Wort.
        tag.len() >= 2
            && tag.chars().next().is_some_and(|c| !c.is_ascii_digit())
            && tag.len() == rest.len()
    })
}

/// Zeilen, die als Vorschautext taugen — keine Listen, Tabellen, Ueberschriften.
fn is_prose(line: &str) -> bool {
    !line.is_empty()
        && !line.starts_with('#')
        && !line.starts_with('-')
        && !line.starts_with('*')
        && !line.starts_with('|')
        && !line.starts_with('>')
        && !line.starts_with("![")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liest_frontmatter_titel_und_tags() {
        let note = parse_note(
            "---\ntitle: Projekt Nord\ntags: [arbeit, offen]\nstatus: laeuft\n---\n\nEin Satz Kontext.\n",
        );
        assert_eq!(note.title.as_deref(), Some("Projekt Nord"));
        assert_eq!(note.tags, vec!["arbeit", "offen"]);
        assert_eq!(note.excerpt.as_deref(), Some("Ein Satz Kontext."));
        assert!(note.front_matter.iter().any(|(k, v)| k == "status" && v == "laeuft"));
    }

    #[test]
    fn liest_frontmatter_als_liste() {
        let note = parse_note("---\ntags:\n  - projekt\n  - nord\n---\n# Titel\n");
        assert_eq!(note.tags, vec!["nord", "projekt"]);
        assert_eq!(note.title.as_deref(), Some("Titel"));
    }

    #[test]
    fn ueberschrift_wird_titel_ohne_frontmatter() {
        let note = parse_note("# Wochenrueckblick\n\nText.\n");
        assert_eq!(note.title.as_deref(), Some("Wochenrueckblick"));
    }

    #[test]
    fn findet_offene_aufgaben_mit_emoji_datum() {
        let note = parse_note("- [ ] Steuer einreichen \u{1F4C5} 2026-09-30 #finanzen\n- [x] Erledigt \u{1F4C5} 2026-09-01\n");
        assert_eq!(note.open_tasks.len(), 1, "abgehakte Aufgaben gehoeren nicht dazu");
        let task = &note.open_tasks[0];
        assert_eq!(task.text, "Steuer einreichen", "Tag gehoert in tags, nicht in den Titel");
        assert_eq!(task.due, Some(NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()));
        assert_eq!(task.tags, vec!["finanzen"]);
        assert_eq!(task.line, 1);
    }

    #[test]
    fn tags_verlassen_den_text_aber_nicht_die_tagliste() {
        let note = parse_note("- [ ] Angebot Dachdecker gegenlesen #handwerk #haus\n");
        let task = &note.open_tasks[0];
        // Genau dieser Text bildet den Entdopplungsschluessel. Steht der Tag
        // noch drin, erscheint dieselbe Aufgabe aus Mindwtr ein zweites Mal.
        assert_eq!(task.text, "Angebot Dachdecker gegenlesen");
        assert_eq!(task.tags, vec!["handwerk", "haus"]);
    }

    #[test]
    fn ziffernfolgen_sind_keine_tags_und_bleiben_stehen() {
        let note = parse_note("- [ ] Ticket #42 nachfassen #arbeit\n");
        assert_eq!(note.open_tasks[0].text, "Ticket #42 nachfassen");
        assert_eq!(note.open_tasks[0].tags, vec!["arbeit"]);
    }

    #[test]
    fn versteht_dataview_datum_und_prioritaet() {
        let note = parse_note("* [ ] Angebot pruefen due:: 2026-10-05 \u{23EB}\n");
        let task = &note.open_tasks[0];
        assert_eq!(task.due, Some(NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()));
        assert!(task.high_priority);
        assert_eq!(task.text, "Angebot pruefen");
    }

    #[test]
    fn zeilennummer_zaehlt_frontmatter_mit() {
        let note = parse_note("---\ntitle: X\n---\nText\n- [ ] Aufgabe\n");
        assert_eq!(note.open_tasks[0].line, 5);
    }

    #[test]
    fn code_bloecke_liefern_keine_aufgaben() {
        let note = parse_note("```md\n- [ ] Beispiel aus der Doku\n```\n- [ ] Echt\n");
        assert_eq!(note.open_tasks.len(), 1);
        assert_eq!(note.open_tasks[0].text, "Echt");
    }

    #[test]
    fn unabgeschlossenes_frontmatter_verschluckt_nichts() {
        let note = parse_note("---\ntitle: kaputt\n- [ ] trotzdem sichtbar\n");
        assert_eq!(note.open_tasks.len(), 1);
    }

    #[test]
    fn scheduled_und_due_getrennt() {
        let note = parse_note("- [ ] Termin \u{23F3} 2026-09-20 \u{1F4C5} 2026-09-25\n");
        let t = &note.open_tasks[0];
        assert_eq!(t.scheduled, Some(NaiveDate::from_ymd_opt(2026, 9, 20).unwrap()));
        assert_eq!(t.due, Some(NaiveDate::from_ymd_opt(2026, 9, 25).unwrap()));
        assert_eq!(t.text, "Termin");
    }
}
