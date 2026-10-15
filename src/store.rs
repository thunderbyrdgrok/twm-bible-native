//! Notes, highlights, and appearance, stored under the XDG state directory.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MARKS: &[&str] = &["cyan", "magenta", "amber", "mint", "red"];
pub const FONT_MIN: i32 = 15;
pub const FONT_MAX: i32 = 40;
pub const GAP_MIN: i32 = 0;
pub const GAP_MAX: i32 = 28;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub links: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportRec {
    pub id: String,
    pub label: String,
    pub file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme_mode: String,
    #[serde(default = "default_font")]
    pub font_size: i32,
    #[serde(default = "default_enabled")]
    pub enabled: Vec<String>,
    #[serde(default = "default_active")]
    pub active: String,
    #[serde(default = "default_mark")]
    pub mark_color: String,
    #[serde(default = "default_book")]
    pub last_book: String,
    #[serde(default = "default_chapter")]
    pub last_chapter: i32,
    #[serde(default)]
    pub last_verse: i32,
    #[serde(default)]
    pub notes: BTreeMap<String, Note>,
    #[serde(default)]
    pub highlights: BTreeMap<String, String>,
    #[serde(default)]
    pub imports: Vec<ImportRec>,
    #[serde(default)]
    pub bookmarks: Vec<String>,
    #[serde(default = "default_gap")]
    pub verse_gap: i32,
    #[serde(default)]
    pub compare_on: bool,
    #[serde(default = "default_compare")]
    pub compare_id: String,
    #[serde(default)]
    pub intro_seen: bool,
    #[serde(default)]
    pub intro_on_startup: bool,
}

fn default_theme() -> String {
    "system".to_string()
}
fn default_font() -> i32 {
    21
}
fn default_enabled() -> Vec<String> {
    vec!["kjv".to_string()]
}
fn default_active() -> String {
    "kjv".to_string()
}
fn default_mark() -> String {
    "amber".to_string()
}
fn default_gap() -> i32 {
    8
}
fn default_compare() -> String {
    "asv".to_string()
}
fn default_book() -> String {
    "gen".to_string()
}
fn default_chapter() -> i32 {
    1
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_mode: default_theme(),
            font_size: default_font(),
            enabled: default_enabled(),
            active: default_active(),
            mark_color: default_mark(),
            last_book: default_book(),
            last_chapter: default_chapter(),
            last_verse: 0,
            notes: BTreeMap::new(),
            highlights: BTreeMap::new(),
            imports: Vec::new(),
            bookmarks: Vec::new(),
            verse_gap: default_gap(),
            compare_on: false,
            compare_id: default_compare(),
            intro_seen: false,
            intro_on_startup: false,
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        if !matches!(self.theme_mode.as_str(), "light" | "dark" | "system" | "sepia" | "amber" | "forest" | "contrast") {
            self.theme_mode = default_theme();
        }
        self.font_size = self.font_size.clamp(FONT_MIN, FONT_MAX);
        self.verse_gap = self.verse_gap.clamp(GAP_MIN, GAP_MAX);
        if !MARKS.contains(&self.mark_color.as_str()) {
            self.mark_color = default_mark();
        }
        if self.last_book.is_empty() {
            self.last_book = default_book();
        }
        if self.last_chapter < 1 {
            self.last_chapter = 1;
        }
        if self.last_verse < 0 {
            self.last_verse = 0;
        }
        if !self.enabled.iter().any(|id| id == "kjv") {
            self.enabled.insert(0, "kjv".to_string());
        }
        let mut seen = Vec::new();
        self.enabled.retain(|id| {
            if id.is_empty() || seen.contains(id) {
                return false;
            }
            seen.push(id.clone());
            true
        });
        if !self.enabled.iter().any(|id| id == &self.active) {
            self.active = "kjv".to_string();
        }
        self.notes.retain(|_, note| {
            note.text = note.text.trim().to_string();
            !note.text.is_empty() || !note.links.is_empty()
        });
        self.highlights.retain(|_, color| !color.is_empty());
        if self.compare_id.is_empty() {
            self.compare_id = default_compare();
        }
        let mut seen = Vec::new();
        self.bookmarks.retain(|key| {
            if key.split(':').count() != 3 || seen.contains(key) {
                return false;
            }
            seen.push(key.clone());
            true
        });
    }
}

pub fn data_dir() -> Option<PathBuf> {
    if let Ok(path) = env::var("TWM_BIBLE_DATA") {
        let path = PathBuf::from(path);
        if path.join("kjv.json").exists() {
            return Some(path);
        }
    }
    let mut candidates = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")];
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("data"));
            candidates.push(dir.join("../data"));
            candidates.push(dir.join("../../data"));
            // Linux packages and AppImage: <prefix>/bin -> <prefix>/share/twm-bible-native/data
            candidates.push(dir.join("../share/twm-bible-native/data"));
            // macOS app bundle: Contents/MacOS -> Contents/Resources/data
            candidates.push(dir.join("../Resources/data"));
        }
    }
    candidates.push(PathBuf::from("/usr/share/twm-bible-native/data"));
    candidates
        .into_iter()
        .find(|path| path.join("kjv.json").exists())
}

pub fn state_dir() -> PathBuf {
    if let Ok(path) = env::var("XDG_STATE_HOME") {
        if !path.is_empty() {
            return PathBuf::from(path).join("twm-bible-slint");
        }
    }
    #[cfg(windows)]
    if let Ok(path) = env::var("LOCALAPPDATA").or_else(|_| env::var("APPDATA")) {
        if !path.is_empty() {
            return PathBuf::from(path).join("twm-bible-slint");
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    #[cfg(target_os = "macos")]
    return PathBuf::from(home).join("Library/Application Support/twm-bible-slint");
    #[cfg(not(target_os = "macos"))]
    PathBuf::from(home).join(".local/state/twm-bible-slint")
}

pub fn load(path: &Path) -> Settings {
    let mut settings = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default();
    settings.sanitize();
    settings
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let data = serde_json::to_vec_pretty(settings).map_err(|err| err.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data).map_err(|err| err.to_string())?;
    fs::rename(&tmp, path).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_sanitize() {
        let dir = env::temp_dir().join(format!("twm-bible-slint-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut settings = Settings::default();
        settings.theme_mode = "nope".into();
        settings.font_size = 99;
        settings.enabled.clear();
        settings.notes.insert(
            "john:3:16".into(),
            Note {
                text: "  loved  ".into(),
                links: vec!["gen:1:1".into()],
            },
        );
        settings.sanitize();
        assert_eq!(settings.theme_mode, "system");
        assert_eq!(settings.font_size, FONT_MAX);
        settings.verse_gap = 99;
        settings.theme_mode = "sepia".into();
        settings.sanitize();
        assert_eq!(settings.verse_gap, GAP_MAX);
        assert_eq!(settings.theme_mode, "sepia");
        settings.theme_mode = "nope".into();
        settings.sanitize();
        assert_eq!(settings.enabled, vec!["kjv".to_string()]);
        assert_eq!(settings.notes["john:3:16"].text, "loved");
        save(&path, &settings).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.notes["john:3:16"].links, vec!["gen:1:1".to_string()]);
        assert!(!loaded.intro_seen);
        let mut seen = loaded.clone();
        seen.intro_seen = true;
        seen.intro_on_startup = true;
        save(&path, &seen).unwrap();
        let again = load(&path);
        assert!(again.intro_seen && again.intro_on_startup);
        let _ = fs::remove_dir_all(&dir);
    }
}
