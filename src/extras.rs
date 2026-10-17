//! Navigation history, bookmarks, and other small pure helpers.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub book: String,
    pub chapter: usize,
    pub verse: Option<usize>,
}

/// Back stack for jumps made through links, search hits, and the passage box.
#[derive(Default)]
pub struct History {
    stack: Vec<Place>,
}

pub const HISTORY_MAX: usize = 50;

impl History {
    pub fn push(&mut self, place: Place) {
        if self.stack.last() == Some(&place) {
            return;
        }
        self.stack.push(place);
        if self.stack.len() > HISTORY_MAX {
            self.stack.remove(0);
        }
    }

    pub fn pop(&mut self) -> Option<Place> {
        self.stack.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

/// Adds the key when absent, removes it when present. True when it is now a bookmark.
pub fn toggle_bookmark(list: &mut Vec<String>, key: &str) -> bool {
    if let Some(pos) = list.iter().position(|item| item == key) {
        list.remove(pos);
        false
    } else {
        list.push(key.to_string());
        true
    }
}

use std::collections::BTreeMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::store::{Note, Settings, MARKS};

use crate::bible::{self, Book, Hit, Search};
use crate::flow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Old,
    New,
    Book(String),
}

pub fn is_new_testament(book_id: &str) -> bool {
    let first = bible::CANON.iter().position(|book| book.id == "matt");
    let at = bible::CANON.iter().position(|book| book.id == book_id);
    matches!((first, at), (Some(first), Some(at)) if at >= first)
}

fn in_scope(scope: &Scope, book_id: &str) -> bool {
    match scope {
        Scope::All => true,
        Scope::Old => !is_new_testament(book_id),
        Scope::New => is_new_testament(book_id),
        Scope::Book(id) => id == book_id,
    }
}

/// Splits a query into terms. "Quoted text" is one exact phrase; other words stand alone.
pub fn parse_terms(query: &str) -> Vec<String> {
    let query = query.replace(['\u{2019}', '\u{2018}'], "'").replace(['\u{201c}', '\u{201d}'], "\"");
    let mut terms = Vec::new();
    for (index, part) in query.split('"').enumerate() {
        let part = part.to_lowercase();
        if index % 2 == 1 {
            let phrase = part.split_whitespace().collect::<Vec<_>>().join(" ");
            if !phrase.is_empty() {
                terms.push(phrase);
            }
        } else {
            terms.extend(part.split_whitespace().map(str::to_string));
        }
    }
    terms
}

fn term_regex(term: &str) -> Regex {
    let pattern = term.split_whitespace().map(regex::escape).collect::<Vec<_>>().join(r"\s+");
    Regex::new(&format!("(?i){pattern}")).expect("term pattern")
}

/// Verses that contain every term, limited to the scope.
pub fn search(books: &[Book], query: &str, scope: &Scope, limit: usize) -> Search {
    let terms = parse_terms(query);
    let needle = terms.join(" ");
    if needle.chars().filter(|c| !c.is_whitespace()).count() < 2 {
        return Search { query: needle, total: 0, hits: Vec::new(), too_short: true, truncated: false };
    }
    let regexes: Vec<Regex> = terms.iter().map(|term| term_regex(term)).collect();
    let max = if limit == 0 { 500 } else { limit };
    let mut hits = Vec::new();
    let mut total = 0;
    for book in books.iter().filter(|book| in_scope(scope, &book.id)) {
        for (ci, chapter) in book.chapters.iter().enumerate() {
            for (vi, text) in chapter.iter().enumerate() {
                let source = text.replace(['\u{2019}', '\u{2018}'], "'");
                let mut first: Option<(usize, usize)> = None;
                let mut all = true;
                for re in &regexes {
                    match re.find(&source) {
                        Some(found) => {
                            if first.map(|(start, _)| found.start() < start).unwrap_or(true) {
                                first = Some((found.start(), found.end() - found.start()));
                            }
                        }
                        None => {
                            all = false;
                            break;
                        }
                    }
                }
                let Some((start, length)) = first.filter(|_| all) else { continue };
                total += 1;
                if hits.len() >= max {
                    continue;
                }
                hits.push(Hit {
                    book_id: book.id.clone(),
                    name: book.name.clone(),
                    chapter: ci + 1,
                    verse: vi + 1,
                    text: source,
                    start,
                    length,
                });
            }
        }
    }
    Search { query: needle, total, truncated: total > hits.len(), hits, too_short: false }
}

/// A snippet wrapped to lines of (text, highlighted) segments. A word that holds a match is
/// highlighted whole, and neighbouring words of the same kind share one segment.
pub fn highlight_lines(snippet: &str, terms: &[String], width: f32, size: f32) -> Vec<Vec<(String, bool)>> {
    let regexes: Vec<Regex> = terms.iter().map(|term| term_regex(term)).collect();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for re in &regexes {
        ranges.extend(re.find_iter(snippet).map(|found| (found.start(), found.end())));
    }
    let base = snippet.as_ptr() as usize;
    let flags: Vec<bool> = snippet
        .split_whitespace()
        .map(|word| {
            let start = word.as_ptr() as usize - base;
            let end = start + word.len();
            ranges.iter().any(|&(a, b)| a < end && b > start)
        })
        .collect();
    let mut index = 0;
    flow::wrap(snippet, width, size)
        .into_iter()
        .map(|line| {
            let mut segments: Vec<(String, bool)> = Vec::new();
            for chunk in line {
                let flag = flags.get(index).copied().unwrap_or(false);
                index += 1;
                match segments.last_mut() {
                    Some((text, last)) if *last == flag => {
                        text.push(' ');
                        text.push_str(&chunk);
                    }
                    _ => segments.push((chunk, flag)),
                }
            }
            segments
        })
        .collect()
}

pub const BACKUP_FORMAT: &str = "twm-bible-backup";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Backup {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub exported: String,
    #[serde(default)]
    pub notes: BTreeMap<String, Note>,
    #[serde(default)]
    pub highlights: BTreeMap<String, String>,
    #[serde(default)]
    pub bookmarks: Vec<String>,
    /// Optional Outreach sections (private details). Absent in older backups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contacts: Option<Vec<crate::outreach::ContactBackup>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<crate::outreach::Event>>,
}

pub fn make_backup(settings: &Settings, exported: &str) -> Backup {
    Backup {
        format: BACKUP_FORMAT.to_string(),
        version: 1,
        exported: exported.to_string(),
        notes: settings.notes.clone(),
        highlights: settings.highlights.clone(),
        bookmarks: settings.bookmarks.clone(),
        contacts: None,
        events: None,
    }
}

fn valid_key(key: &str) -> bool {
    bible::split_key(key).is_some()
}

/// Reads a backup file and drops anything that is not a verse key or a known colour.
pub fn parse_backup(text: &str) -> Result<Backup, String> {
    let mut backup: Backup =
        serde_json::from_str(text).map_err(|_| "That file is not a TWM Bible backup.".to_string())?;
    if backup.format != BACKUP_FORMAT {
        return Err("That file is not a TWM Bible backup.".into());
    }
    backup.notes.retain(|key, _| valid_key(key));
    for note in backup.notes.values_mut() {
        note.links.retain(|key| valid_key(key));
    }
    backup.highlights.retain(|key, color| valid_key(key) && MARKS.contains(&color.as_str()));
    backup.bookmarks.retain(|key| valid_key(key));
    Ok(backup)
}

/// Replace swaps the three collections. Merge keeps what is here and adds what is missing.
/// A note present on both sides keeps both texts and the union of its links.
pub fn apply_backup(settings: &mut Settings, backup: &Backup, replace: bool) {
    if replace {
        settings.notes = backup.notes.clone();
        settings.highlights = backup.highlights.clone();
        settings.bookmarks = backup.bookmarks.clone();
    } else {
        for (key, incoming) in &backup.notes {
            match settings.notes.get_mut(key) {
                None => {
                    settings.notes.insert(key.clone(), incoming.clone());
                }
                Some(mine) => {
                    if !incoming.text.is_empty() && !mine.text.contains(&incoming.text) {
                        if mine.text.is_empty() {
                            mine.text = incoming.text.clone();
                        } else {
                            mine.text = format!("{}\n---\n{}", mine.text, incoming.text);
                        }
                    }
                    for link in &incoming.links {
                        if !mine.links.contains(link) {
                            mine.links.push(link.clone());
                        }
                    }
                }
            }
        }
        for (key, color) in &backup.highlights {
            settings.highlights.entry(key.clone()).or_insert_with(|| color.clone());
        }
        for key in &backup.bookmarks {
            if !settings.bookmarks.contains(key) {
                settings.bookmarks.push(key.clone());
            }
        }
    }
    settings.sanitize();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str, links: &[&str]) -> Note {
        Note { text: text.into(), links: links.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn backup_roundtrip_and_merge() {
        let mut mine = Settings::default();
        mine.notes.insert("john:3:16".into(), note("mine", &["gen:1:1"]));
        mine.highlights.insert("john:3:16".into(), "amber".into());
        mine.bookmarks.push("john:3:16".into());
        let backup = make_backup(&mine, "2026-10-14");
        let text = serde_json::to_string(&backup).unwrap();
        let parsed = parse_backup(&text).unwrap();
        assert_eq!(parsed.notes.len(), 1);
        assert!(parse_backup("{\"notes\":{}}").is_err());
        assert!(parse_backup("nope").is_err());

        let mut other = Settings::default();
        other.notes.insert("john:3:16".into(), note("theirs", &["rom:8:28"]));
        other.highlights.insert("john:3:16".into(), "red".into());
        other.highlights.insert("ps:23:1".into(), "mint".into());
        apply_backup(&mut other, &parsed, false);
        assert_eq!(other.notes["john:3:16"].text, "theirs\n---\nmine");
        assert_eq!(other.notes["john:3:16"].links.len(), 2);
        assert_eq!(other.highlights["john:3:16"], "red");
        assert_eq!(other.highlights.len(), 2);
        assert_eq!(other.bookmarks, vec!["john:3:16".to_string()]);

        apply_backup(&mut other, &parsed, true);
        assert_eq!(other.notes["john:3:16"].text, "mine");
        assert_eq!(other.highlights.len(), 1);
    }

    #[test]
    fn backup_drops_bad_keys() {
        let text = r#"{"format":"twm-bible-backup","version":1,"notes":{"bad":{"text":"x"}},"highlights":{"john:3:16":"purple"},"bookmarks":["nope","john:3:16"]}"#;
        let backup = parse_backup(text).unwrap();
        assert!(backup.notes.is_empty() && backup.highlights.is_empty());
        assert_eq!(backup.bookmarks, vec!["john:3:16".to_string()]);
    }

    fn books() -> Vec<Book> {
        vec![
            Book {
                id: "gen".into(),
                abbrev: "Gen".into(),
                name: "Genesis".into(),
                chapters: vec![vec!["In the beginning God created the heaven".into(), "And God said, Let there be light".into()]],
            },
            Book {
                id: "john".into(),
                abbrev: "Jn".into(),
                name: "John".into(),
                chapters: vec![vec!["God so loved the world, that he gave his only begotten Son".into()]],
            },
        ]
    }

    #[test]
    fn terms_and_phrases() {
        assert_eq!(parse_terms("\"only begotten\" son"), vec!["only begotten", "son"]);
        assert_eq!(parse_terms("Light  God"), vec!["light", "god"]);
    }

    #[test]
    fn phrase_scope_and_all_words() {
        let books = books();
        assert_eq!(search(&books, "\"begotten only\"", &Scope::All, 10).total, 0);
        assert_eq!(search(&books, "\"only begotten\"", &Scope::All, 10).total, 1);
        assert_eq!(search(&books, "light god", &Scope::All, 10).total, 1);
        assert_eq!(search(&books, "god", &Scope::All, 10).total, 3);
        assert_eq!(search(&books, "god", &Scope::Old, 10).total, 2);
        assert_eq!(search(&books, "god", &Scope::New, 10).total, 1);
        assert_eq!(search(&books, "god", &Scope::Book("john".into()), 10).total, 1);
        assert!(search(&books, "g", &Scope::All, 10).too_short);
    }

    #[test]
    fn highlights_matches() {
        let lines = highlight_lines("God so loved the world", &["loved".to_string()], 2000.0, 15.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            vec![("God so".to_string(), false), ("loved".to_string(), true), ("the world".to_string(), false)]
        );
    }

    fn place(chapter: usize) -> Place {
        Place { book: "gen".into(), chapter, verse: None }
    }

    #[test]
    fn history_push_pop() {
        let mut history = History::default();
        assert!(history.is_empty());
        history.push(place(1));
        history.push(place(1));
        history.push(place(2));
        assert_eq!(history.pop(), Some(place(2)));
        assert_eq!(history.pop(), Some(place(1)));
        assert_eq!(history.pop(), None);
        for n in 0..80 {
            history.push(place(n + 1));
        }
        assert_eq!(history.stack.len(), HISTORY_MAX);
    }

    #[test]
    fn bookmark_toggle() {
        let mut list = Vec::new();
        assert!(toggle_bookmark(&mut list, "john:3:16"));
        assert!(!toggle_bookmark(&mut list, "john:3:16"));
        assert!(list.is_empty());
    }

    #[test]
    fn old_backups_import_and_new_sections_roundtrip() {
        // A backup written before Outreach existed has no contact or event sections.
        let old = r#"{"format":"twm-bible-backup","version":1,"exported":"2026-01-01","notes":{},"highlights":{},"bookmarks":["john:3:16"]}"#;
        let parsed = parse_backup(old).unwrap();
        assert!(parsed.contacts.is_none() && parsed.events.is_none());
        assert_eq!(parsed.bookmarks.len(), 1);
        // The sections are omitted from the file when empty, so old apps see an identical file.
        let plain = serde_json::to_string(&make_backup(&Settings::default(), "d")).unwrap();
        assert!(!plain.contains("contacts") && !plain.contains("events"));
        // With sections, fake people survive a trip through the file.
        let mut backup = make_backup(&Settings::default(), "d");
        backup.contacts = Some(vec![crate::outreach::ContactBackup {
            contact: crate::outreach::Contact { id: 4, name: "Test Person".into(), in_pool: true, ..Default::default() },
            photo_b64: Some("/9j/".into()),
        }]);
        backup.events = Some(vec![crate::outreach::Event { id: 2, title: "T".into(), date: "2026-10-20".into(), ..Default::default() }]);
        let again = parse_backup(&serde_json::to_string(&backup).unwrap()).unwrap();
        assert_eq!(again.contacts.unwrap()[0].contact.name, "Test Person");
        assert_eq!(again.events.unwrap()[0].date, "2026-10-20");
    }
}
