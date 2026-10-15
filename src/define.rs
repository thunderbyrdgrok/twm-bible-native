//! Modern definitions. The KJV glossary is local. A modern sense is fetched
//! once from dictionaryapi.dev, then Datamuse, and kept in the state directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use serde_json::Value;

static CACHE_LOCK: Mutex<()> = Mutex::new(());

pub fn cached(path: &Path, key: &str) -> Option<String> {
    let _guard = CACHE_LOCK.lock().ok()?;
    let text = fs::read_to_string(path).ok()?;
    let map: BTreeMap<String, String> = serde_json::from_str(&text).ok()?;
    map.get(key).cloned()
}

fn write_cache(path: &Path, key: &str, value: &str) {
    let _guard = CACHE_LOCK.lock().ok();
    let mut map: BTreeMap<String, String> = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    map.insert(key.to_string(), value.to_string());
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&map) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, data).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn encode(key: &str) -> String {
    let mut out = String::new();
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn fetch(url: &str) -> Option<(u16, String)> {
    let output = crate::quiet(Command::new("curl").args([
            "-sS",
            "--max-time",
            "6",
            "-A",
            "TWM-Bible/1.0",
            "-H",
            "Accept: application/json",
            "-w",
            "\n%{http_code}",
            url,
        ]))
        .output()
        .ok()?;
    if !output.status.success() && output.stdout.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n')?;
    let code = code.trim().parse().unwrap_or(0);
    if code == 0 && !output.status.success() {
        return None;
    }
    Some((code, body.to_string()))
}

pub fn parse_dictionary(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let entry = value.as_array()?.first()?;
    let mut phonetic = entry
        .get("phonetic")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if phonetic.is_empty() {
        phonetic = entry
            .get("phonetics")
            .and_then(|item| item.as_array())
            .and_then(|list| {
                list.iter().find_map(|item| {
                    item.get("text")
                        .and_then(|text| text.as_str())
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                })
            })
            .unwrap_or_default();
    }
    let meanings = entry.get("meanings")?.as_array()?;
    let mut lines = Vec::new();
    if !phonetic.is_empty() {
        lines.push(phonetic);
    }
    let mut groups = 0;
    for meaning in meanings.iter().take(3) {
        let pos = meaning
            .get("partOfSpeech")
            .and_then(|item| item.as_str())
            .unwrap_or("");
        let defs = meaning.get("definitions")?.as_array()?;
        let texts: Vec<String> = defs
            .iter()
            .filter_map(|item| item.get("definition").and_then(|text| text.as_str()))
            .map(squeeze)
            .filter(|text| !text.is_empty())
            .take(2)
            .collect();
        if texts.is_empty() {
            continue;
        }
        groups += 1;
        let body = texts.join(" ");
        if pos.is_empty() {
            lines.push(body);
        } else {
            lines.push(format!("{pos} — {body}"));
        }
    }
    if groups == 0 {
        return None;
    }
    Some(lines.join("\n"))
}

pub fn parse_datamuse(body: &str, key: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let entry = value.as_array()?.first()?;
    let word = entry.get("word").and_then(|item| item.as_str()).unwrap_or("");
    if word.to_lowercase() != key.to_lowercase() {
        return None;
    }
    let labels = [
        ("n", "noun"),
        ("v", "verb"),
        ("adj", "adjective"),
        ("adv", "adverb"),
        ("u", "interjection"),
        ("pron", "pronoun"),
        ("prep", "preposition"),
        ("conj", "conjunction"),
    ];
    let mut order = Vec::new();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for item in entry.get("defs").and_then(|defs| defs.as_array())? {
        let raw = item.as_str().unwrap_or("");
        let (pos, text) = raw.split_once('\t').unwrap_or(("", raw));
        let text = squeeze(text);
        if text.is_empty() {
            continue;
        }
        let label = labels
            .iter()
            .find(|(code, _)| *code == pos)
            .map(|(_, name)| *name)
            .unwrap_or(if pos.is_empty() { "definition" } else { pos });
        if let Some((_, defs)) = groups.iter_mut().find(|(name, _)| name == label) {
            if defs.len() < 2 {
                defs.push(text);
            }
        } else if order.len() < 3 {
            order.push(label.to_string());
            groups.push((label.to_string(), vec![text]));
        }
    }
    if groups.is_empty() {
        return None;
    }
    Some(
        groups
            .iter()
            .map(|(pos, defs)| format!("{pos} — {}", defs.join(" ")))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn lookup_modern(cache: &Path, key: &str) -> String {
    let key = key.trim().to_lowercase();
    if key.is_empty() || key.len() > 40 {
        return "That is not a word this dictionary can look up.".into();
    }
    if let Some(hit) = cached(cache, &key) {
        return hit;
    }
    let dictionary = fetch(&format!(
        "https://api.dictionaryapi.dev/api/v2/entries/en/{}",
        encode(&key)
    ));
    if let Some((200, body)) = &dictionary {
        if let Some(text) = parse_dictionary(body) {
            write_cache(cache, &key, &text);
            return text;
        }
    }
    let datamuse = fetch(&format!(
        "https://api.datamuse.com/words?sp={}&md=d&max=1",
        encode(&key)
    ));
    if let Some((200, body)) = &datamuse {
        if let Some(text) = parse_datamuse(body, &key) {
            write_cache(cache, &key, &text);
            return text;
        }
    }
    if dictionary.is_none() && datamuse.is_none() {
        "The modern dictionary needs a network the first time this word is opened.".into()
    } else {
        "No modern definition was found.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_payload() {
        let body = r#"[{"word":"charity","phonetic":"/ˈtʃærɪti/","meanings":[{"partOfSpeech":"noun","definitions":[{"definition":"The voluntary giving of help."}]}]}]"#;
        let text = parse_dictionary(body).unwrap();
        assert!(text.contains("/ˈtʃærɪti/"));
        assert!(text.contains("noun — The voluntary giving of help."));
    }

    #[test]
    fn datamuse_payload() {
        let body = r#"[{"word":"charity","defs":["n\ta kindly and lenient attitude toward people"]}]"#;
        let text = parse_datamuse(body, "charity").unwrap();
        assert!(text.starts_with("noun — a kindly"));
        assert!(parse_datamuse(body, "love").is_none());
    }
}
