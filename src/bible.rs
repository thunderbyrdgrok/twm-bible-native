//! Canon, reference parsing, search, and the KJV glossary.
//! The behavior follows the web edition's `logic.js`.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug)]
pub struct CanonBook {
    pub id: &'static str,
    pub abbrev: &'static str,
    pub name: &'static str,
    pub chapters: usize,
    pub aliases: &'static [&'static str],
}

pub const CANON: &[CanonBook] = &[
    CanonBook { id: "gen", abbrev: "gn", name: "Genesis", chapters: 50, aliases: &["genesis", "gen", "ge"] },
    CanonBook { id: "exo", abbrev: "ex", name: "Exodus", chapters: 40, aliases: &["exodus", "exo", "ex"] },
    CanonBook { id: "lev", abbrev: "lv", name: "Leviticus", chapters: 27, aliases: &["leviticus", "lev", "lv"] },
    CanonBook { id: "num", abbrev: "nm", name: "Numbers", chapters: 36, aliases: &["numbers", "num", "nm"] },
    CanonBook { id: "deut", abbrev: "dt", name: "Deuteronomy", chapters: 34, aliases: &["deuteronomy", "deut", "dt"] },
    CanonBook { id: "josh", abbrev: "js", name: "Joshua", chapters: 24, aliases: &["joshua", "josh"] },
    CanonBook { id: "judg", abbrev: "jz", name: "Judges", chapters: 21, aliases: &["judges", "judg", "jdg"] },
    CanonBook { id: "ruth", abbrev: "rt", name: "Ruth", chapters: 4, aliases: &["ruth"] },
    CanonBook { id: "1sam", abbrev: "1sm", name: "1 Samuel", chapters: 31, aliases: &["1 samuel", "1samuel", "1 sam", "1sam"] },
    CanonBook { id: "2sam", abbrev: "2sm", name: "2 Samuel", chapters: 24, aliases: &["2 samuel", "2samuel", "2 sam", "2sam"] },
    CanonBook { id: "1kgs", abbrev: "1rs", name: "1 Kings", chapters: 22, aliases: &["1 kings", "1kings", "1 kgs", "1kgs"] },
    CanonBook { id: "2kgs", abbrev: "2rs", name: "2 Kings", chapters: 25, aliases: &["2 kings", "2kings", "2 kgs", "2kgs"] },
    CanonBook { id: "1chr", abbrev: "1cr", name: "1 Chronicles", chapters: 29, aliases: &["1 chronicles", "1chronicles", "1 chron", "1chr"] },
    CanonBook { id: "2chr", abbrev: "2cr", name: "2 Chronicles", chapters: 36, aliases: &["2 chronicles", "2chronicles", "2 chron", "2chr"] },
    CanonBook { id: "ezra", abbrev: "ed", name: "Ezra", chapters: 10, aliases: &["ezra"] },
    CanonBook { id: "neh", abbrev: "ne", name: "Nehemiah", chapters: 13, aliases: &["nehemiah", "neh"] },
    CanonBook { id: "esth", abbrev: "et", name: "Esther", chapters: 10, aliases: &["esther", "esth"] },
    CanonBook { id: "job", abbrev: "j\u{00f3}", name: "Job", chapters: 42, aliases: &["job"] },
    CanonBook { id: "ps", abbrev: "sl", name: "Psalms", chapters: 150, aliases: &["psalms", "psalm", "ps", "psa"] },
    CanonBook { id: "prov", abbrev: "pv", name: "Proverbs", chapters: 31, aliases: &["proverbs", "prov", "prv"] },
    CanonBook { id: "eccl", abbrev: "ec", name: "Ecclesiastes", chapters: 12, aliases: &["ecclesiastes", "eccl", "ecc"] },
    CanonBook { id: "song", abbrev: "ct", name: "Song of Solomon", chapters: 8, aliases: &["song of solomon", "song of songs", "canticles", "song"] },
    CanonBook { id: "isa", abbrev: "is", name: "Isaiah", chapters: 66, aliases: &["isaiah", "isa"] },
    CanonBook { id: "jer", abbrev: "jr", name: "Jeremiah", chapters: 52, aliases: &["jeremiah", "jer"] },
    CanonBook { id: "lam", abbrev: "lm", name: "Lamentations", chapters: 5, aliases: &["lamentations", "lam"] },
    CanonBook { id: "ezek", abbrev: "ez", name: "Ezekiel", chapters: 48, aliases: &["ezekiel", "ezek"] },
    CanonBook { id: "dan", abbrev: "dn", name: "Daniel", chapters: 12, aliases: &["daniel", "dan"] },
    CanonBook { id: "hos", abbrev: "os", name: "Hosea", chapters: 14, aliases: &["hosea", "hos"] },
    CanonBook { id: "joel", abbrev: "jl", name: "Joel", chapters: 3, aliases: &["joel"] },
    CanonBook { id: "amos", abbrev: "am", name: "Amos", chapters: 9, aliases: &["amos"] },
    CanonBook { id: "obad", abbrev: "ob", name: "Obadiah", chapters: 1, aliases: &["obadiah", "obad"] },
    CanonBook { id: "jonah", abbrev: "jn", name: "Jonah", chapters: 4, aliases: &["jonah"] },
    CanonBook { id: "mic", abbrev: "mq", name: "Micah", chapters: 7, aliases: &["micah", "mic"] },
    CanonBook { id: "nah", abbrev: "na", name: "Nahum", chapters: 3, aliases: &["nahum"] },
    CanonBook { id: "hab", abbrev: "hc", name: "Habakkuk", chapters: 3, aliases: &["habakkuk", "hab"] },
    CanonBook { id: "zeph", abbrev: "sf", name: "Zephaniah", chapters: 3, aliases: &["zephaniah", "zeph"] },
    CanonBook { id: "hag", abbrev: "ag", name: "Haggai", chapters: 2, aliases: &["haggai", "hag"] },
    CanonBook { id: "zech", abbrev: "zc", name: "Zechariah", chapters: 14, aliases: &["zechariah", "zech"] },
    CanonBook { id: "mal", abbrev: "ml", name: "Malachi", chapters: 4, aliases: &["malachi", "mal"] },
    CanonBook { id: "matt", abbrev: "mt", name: "Matthew", chapters: 28, aliases: &["matthew", "matt", "mt"] },
    CanonBook { id: "mark", abbrev: "mc", name: "Mark", chapters: 16, aliases: &["mark", "mk"] },
    CanonBook { id: "luke", abbrev: "lc", name: "Luke", chapters: 24, aliases: &["luke", "lk"] },
    CanonBook { id: "john", abbrev: "jo", name: "John", chapters: 21, aliases: &["john", "jn", "joh"] },
    CanonBook { id: "acts", abbrev: "atos", name: "Acts", chapters: 28, aliases: &["acts"] },
    CanonBook { id: "rom", abbrev: "rm", name: "Romans", chapters: 16, aliases: &["romans", "rom"] },
    CanonBook { id: "1cor", abbrev: "1co", name: "1 Corinthians", chapters: 16, aliases: &["1 corinthians", "1corinthians", "1 cor", "1cor"] },
    CanonBook { id: "2cor", abbrev: "2co", name: "2 Corinthians", chapters: 13, aliases: &["2 corinthians", "2corinthians", "2 cor", "2cor"] },
    CanonBook { id: "gal", abbrev: "gl", name: "Galatians", chapters: 6, aliases: &["galatians", "gal"] },
    CanonBook { id: "eph", abbrev: "ef", name: "Ephesians", chapters: 6, aliases: &["ephesians", "eph"] },
    CanonBook { id: "phil", abbrev: "fp", name: "Philippians", chapters: 4, aliases: &["philippians", "phil"] },
    CanonBook { id: "col", abbrev: "cl", name: "Colossians", chapters: 4, aliases: &["colossians", "col"] },
    CanonBook { id: "1thess", abbrev: "1ts", name: "1 Thessalonians", chapters: 5, aliases: &["1 thessalonians", "1thessalonians", "1 thess", "1thess"] },
    CanonBook { id: "2thess", abbrev: "2ts", name: "2 Thessalonians", chapters: 3, aliases: &["2 thessalonians", "2thessalonians", "2 thess", "2thess"] },
    CanonBook { id: "1tim", abbrev: "1tm", name: "1 Timothy", chapters: 6, aliases: &["1 timothy", "1timothy", "1 tim", "1tim"] },
    CanonBook { id: "2tim", abbrev: "2tm", name: "2 Timothy", chapters: 4, aliases: &["2 timothy", "2timothy", "2 tim", "2tim"] },
    CanonBook { id: "titus", abbrev: "tt", name: "Titus", chapters: 3, aliases: &["titus"] },
    CanonBook { id: "phlm", abbrev: "fm", name: "Philemon", chapters: 1, aliases: &["philemon", "phlm"] },
    CanonBook { id: "heb", abbrev: "hb", name: "Hebrews", chapters: 13, aliases: &["hebrews", "heb"] },
    CanonBook { id: "jas", abbrev: "tg", name: "James", chapters: 5, aliases: &["james", "jas"] },
    CanonBook { id: "1pet", abbrev: "1pe", name: "1 Peter", chapters: 5, aliases: &["1 peter", "1peter", "1 pet", "1pet"] },
    CanonBook { id: "2pet", abbrev: "2pe", name: "2 Peter", chapters: 3, aliases: &["2 peter", "2peter", "2 pet", "2pet"] },
    CanonBook { id: "1john", abbrev: "1jo", name: "1 John", chapters: 5, aliases: &["1 john", "1john", "1 jn", "1jn"] },
    CanonBook { id: "2john", abbrev: "2jo", name: "2 John", chapters: 1, aliases: &["2 john", "2john", "2 jn", "2jn"] },
    CanonBook { id: "3john", abbrev: "3jo", name: "3 John", chapters: 1, aliases: &["3 john", "3john", "3 jn", "3jn"] },
    CanonBook { id: "jude", abbrev: "jd", name: "Jude", chapters: 1, aliases: &["jude"] },
    CanonBook { id: "rev", abbrev: "ap", name: "Revelation", chapters: 22, aliases: &["revelation", "rev", "apocalypse"] },
];

pub fn section_label(id: &str) -> &'static str {
    match id {
        "gen" => "OLD TESTAMENT",
        "matt" => "NEW TESTAMENT",
        _ => "",
    }
}

pub fn book_by_id(id: &str) -> Option<&'static CanonBook> {
    CANON.iter().find(|book| book.id == id)
}

#[derive(Clone, Debug)]
pub struct Book {
    pub id: String,
    pub abbrev: String,
    pub name: String,
    pub chapters: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedRef {
    pub book_id: &'static str,
    pub chapter: Option<u32>,
    pub verse: Option<u32>,
}

struct Alias {
    alias: String,
    id: &'static str,
}

fn alias_table() -> &'static [Alias] {
    static TABLE: OnceLock<Vec<Alias>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut rows = Vec::new();
        for book in CANON {
            let mut names = BTreeSet::new();
            names.insert(book.name.to_lowercase());
            names.insert(book.id.to_string());
            for alias in book.aliases {
                names.insert((*alias).to_string());
            }
            for alias in names {
                rows.push(Alias { alias, id: book.id });
            }
        }
        rows.sort_by(|a, b| b.alias.len().cmp(&a.alias.len()));
        rows
    })
}

fn replace_leading(input: &str, words: &[&str], replacement: &str) -> String {
    for word in words {
        if let Some(rest) = input.strip_prefix(word) {
            if rest.starts_with(char::is_whitespace) {
                return format!("{replacement} {}", rest.trim_start());
            }
        }
    }
    input.to_string()
}

fn prep_reference(input: &str) -> String {
    let mut text = input.trim().to_lowercase().replace('.', "");
    text = replace_leading(&text, &["first", "1st"], "1");
    text = replace_leading(&text, &["second", "2nd"], "2");
    text = replace_leading(&text, &["third", "3rd"], "3");
    text = replace_leading(&text, &["iii"], "3");
    text = replace_leading(&text, &["ii"], "2");
    text = replace_leading(&text, &["i"], "1");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn take_number(input: &str) -> Option<(u32, &str)> {
    let end = input
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    let number = input[..end].parse().ok()?;
    Some((number, &input[end..]))
}

pub fn parse_reference(input: &str) -> Option<ParsedRef> {
    let mut text = prep_reference(input);
    if text.is_empty() {
        return None;
    }
    let mut found = None;
    for row in alias_table() {
        if !text.starts_with(&row.alias) {
            continue;
        }
        let next = text[row.alias.len()..].chars().next();
        if let Some(ch) = next {
            if !(ch.is_whitespace() || ch == ':' || ch.is_ascii_digit()) {
                continue;
            }
        }
        found = Some(row.id);
        text = text[row.alias.len()..].trim().to_string();
        break;
    }
    let book_id = found?;
    text = text
        .trim_start_matches(|ch: char| ch == ':' || ch.is_whitespace())
        .to_string();
    if text.is_empty() {
        return Some(ParsedRef {
            book_id,
            chapter: None,
            verse: None,
        });
    }
    let (chapter, rest) = take_number(&text)?;
    let verse = if let Some(after) = rest.strip_prefix(':') {
        take_number(after.trim_start()).map(|(number, _)| number)
    } else if rest.starts_with(char::is_whitespace) {
        take_number(rest.trim_start()).map(|(number, _)| number)
    } else {
        None
    };
    Some(ParsedRef {
        book_id,
        chapter: Some(chapter),
        verse,
    })
}

pub fn ref_key(book_id: &str, chapter: usize, verse: usize) -> String {
    format!("{book_id}:{chapter}:{verse}")
}

pub fn split_key(key: &str) -> Option<(&str, usize, usize)> {
    let mut parts = key.split(':');
    let book = parts.next()?;
    let chapter = parts.next()?.parse().ok()?;
    let verse = parts.next()?.parse().ok()?;
    if parts.next().is_some() || book.is_empty() {
        return None;
    }
    Some((book, chapter, verse))
}

pub fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in value.nfd() {
        if ('\u{0300}'..='\u{036f}').contains(&ch) {
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "book".to_string() } else { out }
}

fn verse_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn normalize(raw: &Value) -> Result<Vec<Book>, String> {
    let arr = if let Some(list) = raw.as_array() {
        list
    } else if let Some(list) = raw.get("books").and_then(Value::as_array) {
        list
    } else if let Some(list) = raw.get("Book").and_then(Value::as_array) {
        list
    } else {
        return Err("This file is not a Bible JSON array of books.".into());
    };
    if arr.is_empty() {
        return Err("This file has no books.".into());
    }
    let abbrevs: Vec<String> = arr
        .iter()
        .map(|book| {
            book.get("abbrev")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect();
    let canon_match = abbrevs.len() == CANON.len()
        && abbrevs
            .iter()
            .enumerate()
            .all(|(index, abbrev)| abbrev == CANON[index].abbrev);
    let mut books = Vec::with_capacity(arr.len());
    for (index, book) in arr.iter().enumerate() {
        let chapters_value = book
            .get("chapters")
            .and_then(Value::as_array)
            .ok_or_else(|| "A book is missing its chapters.".to_string())?;
        let mut chapters = Vec::with_capacity(chapters_value.len());
        for chapter in chapters_value {
            let verses = chapter
                .as_array()
                .ok_or_else(|| "A chapter is not a list of verses.".to_string())?;
            chapters.push(verses.iter().map(verse_string).collect());
        }
        if canon_match {
            books.push(Book {
                id: CANON[index].id.to_string(),
                abbrev: CANON[index].abbrev.to_string(),
                name: CANON[index].name.to_string(),
                chapters,
            });
        } else {
            let mut name = book
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| book.get("book").and_then(Value::as_str))
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                name = format!("Book {}", index + 1);
            }
            let mut abbrev = book
                .get("abbrev")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if abbrev.is_empty() {
                abbrev = format!("b{}", index + 1);
            }
            let id = slug(&format!("{abbrev}-{name}"));
            books.push(Book {
                id,
                abbrev,
                name,
                chapters,
            });
        }
    }
    Ok(books)
}

pub fn load_bible_file(path: &Path) -> Result<Vec<Book>, String> {
    let file = File::open(path).map_err(|_| format!("Could not open {}.", path.display()))?;
    let value: Value = serde_json::from_reader(BufReader::new(file))
        .map_err(|_| "That file is not valid JSON.".to_string())?;
    normalize(&value)
}

pub fn step_chapter(books: &[Book], book_id: &str, chapter: usize, delta: i32) -> Option<(String, usize)> {
    let index = books.iter().position(|book| book.id == book_id)?;
    let mut next_index = index as i32;
    let mut next_chapter = chapter as i32 + delta;
    if next_chapter < 1 {
        next_index -= 1;
        if next_index < 0 {
            return None;
        }
        next_chapter = books[next_index as usize].chapters.len() as i32;
    } else if next_chapter > books[index].chapters.len() as i32 {
        next_index += 1;
        if next_index >= books.len() as i32 {
            return None;
        }
        next_chapter = 1;
    }
    if next_chapter < 1 {
        return None;
    }
    Some((books[next_index as usize].id.clone(), next_chapter as usize))
}

pub fn compact_verses(nums: &[usize]) -> String {
    let mut sorted = nums.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.is_empty() {
        return String::new();
    }
    let mut ranges = Vec::new();
    let mut start = sorted[0];
    let mut prev = sorted[0];
    for number in sorted.iter().skip(1).copied() {
        if number == prev + 1 {
            prev = number;
            continue;
        }
        ranges.push(if start == prev {
            start.to_string()
        } else {
            format!("{start}-{prev}")
        });
        start = number;
        prev = number;
    }
    ranges.push(if start == prev {
        start.to_string()
    } else {
        format!("{start}-{prev}")
    });
    ranges.join(", ")
}

#[derive(Clone, Debug)]
pub struct PassageEntry {
    pub book_id: String,
    pub name: String,
    pub chapter: usize,
    pub verse: usize,
    pub text: String,
}

pub struct Passage {
    pub reference: String,
    pub text: String,
}

pub fn format_passage(entries: &[PassageEntry], version_label: &str) -> Passage {
    let mut groups: Vec<(String, String, Vec<(usize, Vec<usize>)>)> = Vec::new();
    for entry in entries {
        if groups.last().map(|(id, _, _)| id == &entry.book_id) != Some(true) {
            groups.push((
                entry.book_id.clone(),
                entry.name.clone(),
                vec![(entry.chapter, vec![entry.verse])],
            ));
            continue;
        }
        let parts = &mut groups.last_mut().unwrap().2;
        if parts.last().map(|(chapter, _)| *chapter == entry.chapter) == Some(true) {
            parts.last_mut().unwrap().1.push(entry.verse);
        } else {
            parts.push((entry.chapter, vec![entry.verse]));
        }
    }
    let reference_body = groups
        .iter()
        .map(|(_, name, parts)| {
            let bits = parts
                .iter()
                .map(|(chapter, verses)| format!("{chapter}:{}", compact_verses(verses)))
                .collect::<Vec<_>>()
                .join("; ");
            format!("{name} {bits}")
        })
        .collect::<Vec<_>>()
        .join("; ");
    let reference = if version_label.is_empty() {
        reference_body
    } else {
        format!("{reference_body} ({version_label})")
    };
    let body = entries
        .iter()
        .map(|entry| format!("{} {}", entry.verse, entry.text.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    Passage {
        text: format!("{body}\n\n{reference}"),
        reference,
    }
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub book_id: String,
    pub name: String,
    pub chapter: usize,
    pub verse: usize,
    pub text: String,
    pub start: usize,
    pub length: usize,
}

pub struct Search {
    pub query: String,
    pub total: usize,
    pub hits: Vec<Hit>,
    pub too_short: bool,
    pub truncated: bool,
}

pub fn search_verses(books: &[Book], query: &str, limit: usize) -> Search {
    let words: Vec<String> = query
        .trim()
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let needle = words.join(" ");
    let max = if limit == 0 { 500 } else { limit };
    if needle.len() < 2 {
        return Search {
            query: needle,
            total: 0,
            hits: Vec::new(),
            too_short: true,
            truncated: false,
        };
    }
    let pattern = words
        .iter()
        .map(|word| regex::escape(word))
        .collect::<Vec<_>>()
        .join(r"\s+");
    let re = Regex::new(&format!("(?i){pattern}")).expect("search pattern");
    let mut hits = Vec::new();
    let mut total = 0;
    for book in books {
        for (chapter_index, chapter) in book.chapters.iter().enumerate() {
            for (verse_index, text) in chapter.iter().enumerate() {
                let source = text.replace(['\u{2019}', '\u{2018}'], "'");
                let Some(found) = re.find(&source) else {
                    continue;
                };
                total += 1;
                if hits.len() >= max {
                    continue;
                }
                let start = found.start();
                let length = found.end() - start;
                hits.push(Hit {
                    book_id: book.id.clone(),
                    name: book.name.clone(),
                    chapter: chapter_index + 1,
                    verse: verse_index + 1,
                    text: source,
                    start,
                    length,
                });
            }
        }
    }
    Search {
        query: needle,
        total,
        truncated: total > hits.len(),
        hits,
        too_short: false,
    }
}

pub fn snippet(text: &str, start: usize, len: usize) -> String {
    if text.chars().count() <= 240 {
        return text.to_string();
    }
    let start = start.min(text.len());
    let end = (start + len).min(text.len());
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return text.chars().take(240).collect();
    }
    let start_char = text[..start].chars().count();
    let match_chars = text[start..end].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let from = start_char.saturating_sub(48);
    let to = (start_char + match_chars + 96).min(chars.len());
    let mut out: String = chars[from..to].iter().collect();
    if from > 0 {
        out.insert(0, '\u{2026}');
    }
    if to < chars.len() {
        out.push('\u{2026}');
    }
    out
}

pub fn clip(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 40 && !chars[end].is_whitespace() {
        end -= 1;
    }
    let mut out: String = chars[..end].iter().collect();
    out.push('\u{2026}');
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub word: bool,
    pub text: String,
}

fn word_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z]+(?:['\u{2019}-][A-Za-z]+)*").expect("word pattern"))
}

pub fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut last = 0;
    for found in word_regex().find_iter(text) {
        if found.start() > last {
            tokens.push(Token {
                word: false,
                text: text[last..found.start()].to_string(),
            });
        }
        tokens.push(Token {
            word: true,
            text: found.as_str().to_string(),
        });
        last = found.end();
    }
    if last < text.len() {
        tokens.push(Token {
            word: false,
            text: text[last..].to_string(),
        });
    }
    tokens
}

pub fn dictionary_word(surface: &str) -> String {
    let word = surface.replace(['\u{2019}', '\u{2018}'], "'");
    let trimmed = word
        .trim_matches(|ch: char| !matches!(ch, 'A'..='Z' | 'a'..='z' | '\''))
        .to_lowercase();
    let stripped = trimmed.strip_suffix("'s").unwrap_or(&trimmed);
    stripped.trim_end_matches('\'').to_string()
}

#[derive(Clone, Debug, Deserialize)]
struct RawGloss {
    #[serde(default)]
    archaic: String,
    #[serde(default)]
    modern: String,
    #[serde(default)]
    pos: String,
}

#[derive(Clone, Debug)]
pub struct Gloss {
    pub archaic: String,
    pub modern: String,
    pub pos: String,
}

pub fn load_glossary(path: &Path) -> Result<HashMap<String, Gloss>, String> {
    let file = File::open(path).map_err(|_| format!("Could not open {}.", path.display()))?;
    let raw: HashMap<String, RawGloss> = serde_json::from_reader(BufReader::new(file))
        .map_err(|_| "The glossary file is not valid JSON.".to_string())?;
    Ok(raw
        .into_iter()
        .map(|(key, entry)| {
            (
                key,
                Gloss {
                    archaic: entry.archaic,
                    modern: entry.modern,
                    pos: entry.pos,
                },
            )
        })
        .collect())
}

#[derive(Clone, Debug)]
pub struct Sense {
    pub word: String,
    pub surface: String,
    pub pos: String,
    pub archaic: String,
    pub modern: String,
    pub from_glossary: bool,
}

const ETH_SKIP: &[&str] = &[
    "teeth",
    "seth",
    "nazareth",
    "elizabeth",
    "elisabeth",
    "bethlehem",
    "shibboleth",
];

const PHRASES: &[&str] = &[
    "give up the ghost",
    "holy ghost",
    "filthy lucre",
    "by and by",
    "take thought",
    "help meet",
];

pub fn archaic_for(surface: &str, glossary: &HashMap<String, Gloss>) -> Sense {
    let word = dictionary_word(surface);
    let entry = glossary.get(&word);
    let letters: String = surface.chars().filter(|ch| ch.is_ascii_alphabetic()).collect();
    let all_caps = letters.len() > 1 && letters.chars().all(|ch| ch.is_ascii_uppercase());
    let mut archaic = entry
        .map(|item| item.archaic.clone())
        .filter(|text| !text.is_empty())
        .unwrap_or_default();
    if all_caps && (word == "lord" || word == "god") {
        let note = if word == "lord" {
            "In capitals, LORD stands for the divine name YHWH, which older English Bibles also render as Jehovah."
        } else {
            "In capitals, GOD often stands for the divine name YHWH when the verse already says Lord, as in \u{201c}Lord GOD.\u{201d}"
        };
        archaic = if archaic.is_empty() {
            note.to_string()
        } else {
            format!("{note} {archaic}")
        };
    }
    if archaic.is_empty()
        && word.ends_with("eth")
        && word.len() > 4
        && !ETH_SKIP.contains(&word.as_str())
        && !word.ends_with("ieth")
    {
        archaic = "The -eth ending is the older form for he, she, or it. Modern English uses -s, so \u{201c}goeth\u{201d} is \u{201c}goes.\u{201d}".into();
    }
    if archaic.is_empty() {
        archaic = format!(
            "No separate archaic meaning is listed for \u{201c}{word}\u{201d}. In the KJV it is read in its ordinary sense."
        );
    }
    Sense {
        word,
        surface: surface.to_string(),
        pos: entry.map(|item| item.pos.clone()).unwrap_or_default(),
        modern: entry.map(|item| item.modern.clone()).unwrap_or_default(),
        archaic,
        from_glossary: entry.is_some(),
    }
}

pub fn lookup_in_tokens(tokens: &[Token], index: usize, glossary: &HashMap<String, Gloss>) -> Sense {
    let surface = tokens
        .get(index)
        .filter(|token| token.word)
        .map(|token| token.text.as_str())
        .unwrap_or("");
    let word_indexes: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.word)
        .map(|(index, _)| index)
        .collect();
    let seq: Vec<String> = word_indexes
        .iter()
        .map(|index| dictionary_word(&tokens[*index].text))
        .collect();
    if let Some(at) = word_indexes.iter().position(|slot| *slot == index) {
        for phrase in PHRASES {
            let parts: Vec<&str> = phrase.split(' ').collect();
            if seq.len() < parts.len() {
                continue;
            }
            for start in 0..=seq.len() - parts.len() {
                let same = seq[start..start + parts.len()]
                    .iter()
                    .zip(parts.iter())
                    .all(|(left, right)| left == right);
                if !same || at < start || at >= start + parts.len() {
                    continue;
                }
                if let Some(entry) = glossary.get(*phrase) {
                    return Sense {
                        word: (*phrase).to_string(),
                        surface: surface.to_string(),
                        pos: entry.pos.clone(),
                        archaic: entry.archaic.clone(),
                        modern: entry.modern.clone(),
                        from_glossary: true,
                    };
                }
            }
        }
    }
    archaic_for(surface, glossary)
}

pub fn parse_link_list(text: &str) -> (Vec<String>, usize) {
    let mut keys = Vec::new();
    let mut skipped = 0;
    for part in text.split([',', ';', '\n']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match parse_reference(part) {
            Some(parsed) if parsed.chapter.is_some() && parsed.verse.is_some() => {
                keys.push(ref_key(
                    parsed.book_id,
                    parsed.chapter.unwrap() as usize,
                    parsed.verse.unwrap() as usize,
                ));
            }
            _ => skipped += 1,
        }
    }
    keys.sort();
    keys.dedup();
    (keys, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mini() -> Vec<Book> {
        vec![
            Book {
                id: "gen".into(),
                abbrev: "gn".into(),
                name: "Genesis".into(),
                chapters: vec![vec!["In the beginning.".into(), "And the earth.".into()], vec!["And God said.".into()]],
            },
            Book {
                id: "exo".into(),
                abbrev: "ex".into(),
                name: "Exodus".into(),
                chapters: vec![vec!["Now these are the names.".into()]],
            },
        ]
    }

    #[test]
    fn canon_shape() {
        assert_eq!(CANON.len(), 66);
        assert_eq!(CANON[0].abbrev, "gn");
        assert_eq!(CANON[17].abbrev, "j\u{00f3}");
        assert_eq!(CANON[42].id, "john");
        assert_eq!(CANON[65].id, "rev");
        assert_eq!(CANON[18].chapters, 150);
    }

    #[test]
    fn references() {
        assert_eq!(
            parse_reference("John 3:16"),
            Some(ParsedRef { book_id: "john", chapter: Some(3), verse: Some(16) })
        );
        assert_eq!(parse_reference("jn 3:16").unwrap().book_id, "john");
        assert_eq!(parse_reference("1 John 1:9").unwrap().book_id, "1john");
        assert_eq!(parse_reference("Song of Solomon 2:1").unwrap().book_id, "song");
        assert_eq!(
            parse_reference("Psalm 23"),
            Some(ParsedRef { book_id: "ps", chapter: Some(23), verse: None })
        );
        assert_eq!(parse_reference("Jude 1").unwrap().book_id, "jude");
        assert_eq!(parse_reference("Judges 2:1").unwrap().book_id, "judg");
        assert_eq!(parse_reference("Jonah 1:1").unwrap().book_id, "jonah");
        assert_eq!(parse_reference("I Samuel 1:1").unwrap().book_id, "1sam");
        assert_eq!(parse_reference("Genesis").unwrap().chapter, None);
        assert!(parse_reference("not a book").is_none());
    }

    #[test]
    fn passage_and_steps() {
        let books = mini();
        let passage = format_passage(
            &[
                PassageEntry { book_id: "gen".into(), name: "Genesis".into(), chapter: 1, verse: 1, text: "In the beginning.".into() },
                PassageEntry { book_id: "gen".into(), name: "Genesis".into(), chapter: 1, verse: 2, text: "And the earth.".into() },
                PassageEntry { book_id: "gen".into(), name: "Genesis".into(), chapter: 1, verse: 3, text: "And God said.".into() },
                PassageEntry { book_id: "gen".into(), name: "Genesis".into(), chapter: 1, verse: 5, text: "And God called.".into() },
            ],
            "KJV 1611",
        );
        assert_eq!(passage.reference, "Genesis 1:1-3, 5 (KJV 1611)");
        assert!(passage.text.starts_with("1 In the beginning."));
        assert_eq!(step_chapter(&books, "gen", 2, 1), Some(("exo".into(), 1)));
        assert_eq!(step_chapter(&books, "exo", 1, -1), Some(("gen".into(), 2)));
        assert_eq!(step_chapter(&books, "gen", 1, -1), None);
        assert_eq!(step_chapter(&books, "exo", 1, 1), None);
    }

    #[test]
    fn words_and_search() {
        let words: Vec<_> = tokenize("In the beginning, LORD's.")
            .into_iter()
            .filter(|token| token.word)
            .map(|token| token.text)
            .collect();
        assert_eq!(words, vec!["In", "the", "beginning", "LORD's"]);
        assert_eq!(dictionary_word("LORD's"), "lord");
        let books = mini();
        let found = search_verses(&books, "the earth", 500);
        assert_eq!(found.total, 1);
        assert_eq!(found.hits[0].verse, 2);
        assert!(search_verses(&books, "a", 500).too_short);
    }

    #[test]
    fn glossary_senses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/glossary.json");
        if !path.exists() {
            return;
        }
        let glossary = load_glossary(&path).unwrap();
        let lord = archaic_for("LORD", &glossary);
        assert!(lord.archaic.contains("YHWH"));
        let tokens = tokenize("Let your conversation be");
        let conversation = lookup_in_tokens(&tokens, 4, &glossary);
        assert!(conversation.archaic.to_lowercase().contains("conduct")
            || conversation.archaic.to_lowercase().contains("manner of life")
            || conversation.archaic.to_lowercase().contains("behavior"));
        assert!(conversation.modern.to_lowercase().contains("talk"));
        assert!(archaic_for("prevent", &glossary).archaic.to_lowercase().contains("before"));
        assert!(archaic_for("let", &glossary).archaic.to_lowercase().contains("hinder")
            || archaic_for("let", &glossary).archaic.to_lowercase().contains("restrain"));
        let phrase = lookup_in_tokens(&tokenize("by and by"), 0, &glossary);
        assert_eq!(phrase.word, "by and by");
        assert!(phrase.archaic.contains("Immediately"));
    }

    #[test]
    fn bundled_kjv() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/kjv.json");
        if !path.exists() {
            return;
        }
        let books = load_bible_file(&path).unwrap();
        assert_eq!(books.len(), 66);
        assert_eq!(books[0].name, "Genesis");
        assert_eq!(books[42].id, "john");
        assert_eq!(books[18].chapters.len(), 150);
        assert!(books[42].chapters[2][15].starts_with("For God so loved the world"));
        let found = search_verses(&books, "only begotten", 500);
        assert!(found.hits.iter().any(|hit| hit.book_id == "john" && hit.chapter == 3 && hit.verse == 16));
        assert_eq!(step_chapter(&books, "gen", 50, 1), Some(("exo".into(), 1)));
        assert_eq!(step_chapter(&books, "rev", 22, 1), None);
    }
}
