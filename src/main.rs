#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! Native TWM Bible window.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use twm_bible_native::bible::{self as texts, Book};
use twm_bible_native::store::{self, ImportRec, Settings, FONT_MAX, FONT_MIN, GAP_MAX, GAP_MIN};
use twm_bible_native::theme::{self, Scheme};
use twm_bible_native::define;
use twm_bible_native::export;
use twm_bible_native::extras::{self, History, Place};
use twm_bible_native::flow;

slint::include_modules!();

struct Bundled {
    id: &'static str,
    label: &'static str,
    file: &'static str,
}

const BUNDLED: &[Bundled] = &[
    Bundled { id: "kjv", label: "KJV 1611", file: "kjv.json" },
    Bundled { id: "asv", label: "ASV", file: "asv.json" },
    Bundled { id: "web", label: "WEB", file: "web.json" },
    Bundled { id: "ylt", label: "Young's Literal", file: "ylt.json" },
    Bundled { id: "darby", label: "Darby", file: "darby.json" },
    Bundled { id: "rv", label: "Revised Version", file: "rv.json" },
    Bundled { id: "geneva", label: "Geneva 1599", file: "geneva.json" },
    Bundled { id: "dra", label: "Douay-Rheims", file: "dra.json" },
];

struct Edition {
    id: String,
    label: String,
    builtin: bool,
    path: PathBuf,
    books: Option<Vec<Book>>,
}

struct ListedHit {
    book_id: String,
    chapter: usize,
    verse: usize,
    reference: String,
    snippet: String,
}

enum NoteSave {
    NoVerse,
    Unchanged,
    Saved,
    Cleared,
    Skipped(usize),
}

struct Core {
    settings: Settings,
    settings_path: PathBuf,
    data_dir: Option<PathBuf>,
    state_dir: PathBuf,
    glossary: HashMap<String, texts::Gloss>,
    editions: HashMap<String, Edition>,
    order: Vec<String>,
    book_id: String,
    chapter: usize,
    verse: Option<usize>,
    tokens: Vec<texts::Token>,
    last_hits: Vec<ListedHit>,
    search_summary: String,
    load_error: String,
    define_token: Arc<AtomicU64>,
    flash_message: String,
    flash_until: Option<Instant>,
    last_scheme: Scheme,
    text_width: f32,
    history: History,
    pending_backup: Option<extras::Backup>,
    hit_width: f32,
    terms: Vec<String>,
}

#[derive(Clone)]
struct Models {
    books: Rc<VecModel<BookRow>>,
    chapters: Rc<VecModel<ChapterRow>>,
    verses: Rc<VecModel<VerseRow>>,
    words: Rc<VecModel<WordChip>>,
    hits: Rc<VecModel<HitRow>>,
    versions: Rc<VecModel<VersionRow>>,
    live: Rc<VecModel<LiveRow>>,
    links: Rc<VecModel<LinkChip>>,
    bookmarks: Rc<VecModel<BookmarkRow>>,
    word_hits: Rc<VecModel<HitRow>>,
}

impl Core {
    fn open() -> Self {
        let state_dir = store::state_dir();
        let settings_path = state_dir.join("settings.json");
        let settings = store::load(&settings_path);
        let data_dir = store::data_dir();
        let glossary = data_dir
            .as_ref()
            .and_then(|dir| texts::load_glossary(&dir.join("glossary.json")).ok())
            .unwrap_or_default();
        let mut core = Self {
            settings,
            settings_path,
            data_dir,
            state_dir,
            glossary,
            editions: HashMap::new(),
            order: Vec::new(),
            book_id: "gen".into(),
            chapter: 1,
            verse: None,
            tokens: Vec::new(),
            last_hits: Vec::new(),
            search_summary: String::new(),
            load_error: String::new(),
            define_token: Arc::new(AtomicU64::new(0)),
            flash_message: String::new(),
            flash_until: None,
            last_scheme: Scheme::Dark,
            text_width: 600.0,
            history: History::default(),
            pending_backup: None,
            hit_width: 560.0,
            terms: Vec::new(),
        };
        if core.data_dir.is_none() {
            core.load_error = "The Bible text was not found. Expected data/kjv.json beside the app.".into();
        }
        core.mount_editions();
        if core.settings.compare_on {
            let id = core.settings.compare_id.clone();
            let _ = core.ensure(&id);
        }
        if let Err(err) = core.ensure_active() {
            if core.load_error.is_empty() {
                core.load_error = err;
            }
        } else {
            core.restore_place();
        }
        let _ = core.persist();
        core
    }

    fn mount_editions(&mut self) {
        self.editions.clear();
        for bundled in BUNDLED {
            let path = self
                .data_dir
                .as_ref()
                .map(|dir| dir.join(bundled.file))
                .unwrap_or_else(|| PathBuf::from(bundled.file));
            self.editions.insert(
                bundled.id.to_string(),
                Edition {
                    id: bundled.id.to_string(),
                    label: bundled.label.to_string(),
                    builtin: true,
                    path,
                    books: None,
                },
            );
        }
        let imports_dir = self.state_dir.join("imports");
        let imports = self.settings.imports.clone();
        let mut kept = Vec::new();
        for record in imports {
            let path = imports_dir.join(&record.file);
            if !path.exists() {
                continue;
            }
            self.editions.insert(
                record.id.clone(),
                Edition {
                    id: record.id.clone(),
                    label: record.label.clone(),
                    builtin: false,
                    path,
                    books: None,
                },
            );
            kept.push(record);
        }
        self.settings.imports = kept;
        self.rebuild_order();
        self.settings
            .enabled
            .retain(|id| id == "kjv" || self.editions.contains_key(id));
        self.settings.sanitize();
    }

    fn rebuild_order(&mut self) {
        self.order = BUNDLED.iter().map(|item| item.id.to_string()).collect::<Vec<_>>();
        for record in &self.settings.imports {
            if !self.order.iter().any(|id| id == &record.id) {
                self.order.push(record.id.clone());
            }
        }
    }

    fn ensure(&mut self, id: &str) -> Result<(), String> {
        let path = {
            let edition = self
                .editions
                .get(id)
                .ok_or_else(|| "That text is not installed.".to_string())?;
            if edition.books.is_some() {
                return Ok(());
            }
            edition.path.clone()
        };
        let books = texts::load_bible_file(&path)?;
        if let Some(edition) = self.editions.get_mut(id) {
            edition.books = Some(books);
        }
        Ok(())
    }

    fn ensure_active(&mut self) -> Result<(), String> {
        let id = self.settings.active.clone();
        self.ensure(&id)
    }

    fn books(&self) -> &[Book] {
        self.editions
            .get(&self.settings.active)
            .and_then(|edition| edition.books.as_deref())
            .unwrap_or(&[])
    }

    fn book(&self) -> Option<&Book> {
        let id = self.book_id.as_str();
        self.books().iter().find(|book| book.id == id)
    }

    fn book_name(&self) -> String {
        self.book().map(|book| book.name.clone()).unwrap_or_default()
    }

    fn name_of(&self, id: &str) -> String {
        if let Some(book) = self.books().iter().find(|book| book.id == id) {
            return book.name.clone();
        }
        texts::book_by_id(id)
            .map(|book| book.name.to_string())
            .unwrap_or_else(|| id.to_string())
    }

    fn version_label(&self) -> String {
        self.editions
            .get(&self.settings.active)
            .map(|edition| edition.label.clone())
            .unwrap_or_else(|| "KJV 1611".into())
    }

    fn chapter_verses(&self) -> &[String] {
        self.book()
            .and_then(|book| book.chapters.get(self.chapter.saturating_sub(1)))
            .map(|chapter| chapter.as_slice())
            .unwrap_or(&[])
    }

    fn place_title(&self) -> String {
        let name = self.book_name();
        if name.is_empty() {
            return "No text".into();
        }
        match self.verse {
            Some(verse) => format!("{name} {}:{verse}", self.chapter),
            None => format!("{name} {}", self.chapter),
        }
    }

    fn snippet(&self) -> String {
        let text = if let Some(verse) = self.verse {
            self.chapter_verses()
                .get(verse.saturating_sub(1))
                .cloned()
                .unwrap_or_default()
        } else {
            self.chapter_verses().first().cloned().unwrap_or_default()
        };
        texts::clip(&text, 160)
    }

    fn can_step(&self, delta: i32) -> bool {
        texts::step_chapter(self.books(), &self.book_id, self.chapter, delta).is_some()
    }

    fn remember(&mut self) {
        self.settings.last_book = self.book_id.clone();
        self.settings.last_chapter = self.chapter as i32;
        self.settings.last_verse = self.verse.unwrap_or(0) as i32;
    }

    fn restore_place(&mut self) {
        let book = self.settings.last_book.clone();
        let chapter = self.settings.last_chapter.max(1) as usize;
        let verse = if self.settings.last_verse > 0 {
            Some(self.settings.last_verse as usize)
        } else {
            None
        };
        if self.go_to(&book, chapter, verse).is_err() {
            let fallback = self
                .books()
                .first()
                .map(|book| book.id.clone())
                .unwrap_or_else(|| "gen".into());
            let _ = self.go_to(&fallback, 1, None);
        }
    }

    fn go_to(&mut self, book_id: &str, chapter: usize, verse: Option<usize>) -> Result<(), String> {
        let active = self.settings.active.clone();
        self.ensure(&active)?;
        let text = {
            let book = self
                .books()
                .iter()
                .find(|book| book.id == book_id)
                .ok_or_else(|| "That book is not in this text.".to_string())?;
            if chapter == 0 || chapter > book.chapters.len() {
                return Err(format!("{} has {} chapters.", book.name, book.chapters.len()));
            }
            let verses = &book.chapters[chapter - 1];
            if let Some(number) = verse {
                if number == 0 || number > verses.len() {
                    return Err(format!("{} {chapter} has {} verses.", book.name, verses.len()));
                }
                Some(verses[number - 1].clone())
            } else {
                None
            }
        };
        self.book_id = book_id.to_string();
        self.chapter = chapter;
        self.verse = verse;
        self.tokens = text.map(|line| texts::tokenize(&line)).unwrap_or_default();
        self.remember();
        Ok(())
    }

    fn step(&mut self, delta: i32) -> Result<(), String> {
        let next = texts::step_chapter(self.books(), &self.book_id, self.chapter, delta);
        match next {
            Some((id, chapter)) => self.go_to(&id, chapter, None),
            None if delta < 0 => Err("This is the first chapter.".into()),
            None => Err("This is the last chapter.".into()),
        }
    }

    fn pick_verse(&mut self, number: usize) {
        let text = {
            let verses = self.chapter_verses();
            if number == 0 || number > verses.len() {
                return;
            }
            verses[number - 1].clone()
        };
        self.verse = Some(number);
        self.tokens = texts::tokenize(&text);
        self.remember();
    }

    fn mark_verse(&mut self, number: usize) {
        self.pick_verse(number);
        let key = texts::ref_key(&self.book_id, self.chapter, number);
        if !self.settings.highlights.contains_key(&key) {
            let color = self.settings.mark_color.clone();
            self.settings.highlights.insert(key, color);
        }
    }

    fn compare_active(&self) -> bool {
        self.settings.compare_on
            && self
                .editions
                .get(&self.settings.compare_id)
                .map(|edition| edition.books.is_some())
                .unwrap_or(false)
    }

    /// Side by side when the pane is wide enough for two readable columns.
    fn side_by_side(&self) -> bool {
        self.compare_active() && self.text_width >= 640.0
    }

    fn col_width(&self) -> f32 {
        if self.side_by_side() {
            (self.text_width - 24.0) / 2.0
        } else {
            self.text_width
        }
    }

    fn compare_label(&self) -> String {
        self.editions
            .get(&self.settings.compare_id)
            .map(|edition| edition.label.clone())
            .unwrap_or_default()
    }

    fn compare_verses(&self) -> Vec<String> {
        if !self.compare_active() {
            return Vec::new();
        }
        self.editions
            .get(&self.settings.compare_id)
            .and_then(|edition| edition.books.as_ref())
            .and_then(|books| books.iter().find(|book| book.id == self.book_id))
            .and_then(|book| book.chapters.get(self.chapter.saturating_sub(1)))
            .cloned()
            .unwrap_or_default()
    }

    fn set_compare(&mut self, on: bool) -> Result<(), String> {
        if on {
            let id = self.settings.compare_id.clone();
            self.ensure(&id)?;
        }
        self.settings.compare_on = on;
        Ok(())
    }

    fn set_compare_version(&mut self, id: &str) -> Result<String, String> {
        if !self.editions.contains_key(id) {
            return Err("That text is not installed.".into());
        }
        self.ensure(id)?;
        self.settings.compare_id = id.to_string();
        self.settings.compare_on = true;
        Ok(self.compare_label())
    }

    fn current_place(&self) -> Place {
        Place { book: self.book_id.clone(), chapter: self.chapter, verse: self.verse }
    }

    /// Like go_to, but remembers where we were so BACK can return.
    fn go_to_tracked(&mut self, book_id: &str, chapter: usize, verse: Option<usize>) -> Result<(), String> {
        let before = self.current_place();
        self.go_to(book_id, chapter, verse)?;
        if self.current_place() != before {
            self.history.push(before);
        }
        Ok(())
    }

    fn go_back(&mut self) -> Result<(), String> {
        let place = self.history.pop().ok_or_else(|| "Nothing to go back to.".to_string())?;
        self.go_to(&place.book, place.chapter, place.verse)
    }

    fn verse_key(&self) -> Option<String> {
        self.verse.map(|verse| texts::ref_key(&self.book_id, self.chapter, verse))
    }

    fn toggle_bookmark_here(&mut self) -> Result<bool, String> {
        let key = self.verse_key().ok_or_else(|| "Select a verse to bookmark first.".to_string())?;
        Ok(extras::toggle_bookmark(&mut self.settings.bookmarks, &key))
    }

    fn verse_text_of(&self, key: &str) -> String {
        let Some((book, chapter, verse)) = texts::split_key(key) else {
            return String::new();
        };
        self.books()
            .iter()
            .find(|item| item.id == book)
            .and_then(|item| item.chapters.get(chapter.saturating_sub(1)))
            .and_then(|verses| verses.get(verse.saturating_sub(1)))
            .map(|text| texts::clip(text, 140))
            .unwrap_or_default()
    }

    fn set_mark_color(&mut self, color: &str) {
        if !store::MARKS.contains(&color) {
            return;
        }
        self.settings.mark_color = color.to_string();
        if let Some(verse) = self.verse {
            let key = texts::ref_key(&self.book_id, self.chapter, verse);
            self.settings.highlights.insert(key, color.to_string());
        }
    }

    fn clear_mark(&mut self) {
        let Some(verse) = self.verse else {
            return;
        };
        let key = texts::ref_key(&self.book_id, self.chapter, verse);
        self.settings.highlights.remove(&key);
    }

    fn store_note(&mut self, text: &str, links: &str) -> NoteSave {
        let Some(verse) = self.verse else {
            return NoteSave::NoVerse;
        };
        let key = texts::ref_key(&self.book_id, self.chapter, verse);
        let text = text.trim().to_string();
        let (parsed, skipped) = texts::parse_link_list(links);
        if text.is_empty() && parsed.is_empty() {
            if skipped > 0 {
                return NoteSave::Skipped(skipped);
            }
            return if self.settings.notes.remove(&key).is_some() {
                NoteSave::Cleared
            } else {
                NoteSave::Unchanged
            };
        }
        let note = store::Note { text, links: parsed };
        if skipped == 0 && self.settings.notes.get(&key) == Some(&note) {
            return NoteSave::Unchanged;
        }
        self.settings.notes.insert(key, note);
        if skipped > 0 {
            NoteSave::Skipped(skipped)
        } else {
            NoteSave::Saved
        }
    }

    fn open_query(&mut self, query: &str) -> Result<String, String> {
        let parsed = texts::parse_reference(query).ok_or_else(|| "No book by that name.".to_string())?;
        let chapter = parsed.chapter.unwrap_or(1) as usize;
        let verse = parsed.verse.map(|number| number as usize);
        self.go_to_tracked(parsed.book_id, chapter, verse)?;
        Ok(format!("Opened {}.", self.place_title()))
    }

    fn search_words(&mut self, query: &str, scope: &extras::Scope) -> String {
        let result = extras::search(self.books(), query, scope, 500);
        self.terms = extras::parse_terms(query);
        if result.too_short {
            self.last_hits.clear();
            let message = "Type at least two letters.".to_string();
            self.search_summary = message.clone();
            return message;
        }
        self.last_hits = result
            .hits
            .iter()
            .map(|hit| ListedHit {
                book_id: hit.book_id.clone(),
                chapter: hit.chapter,
                verse: hit.verse,
                reference: format!("{} {}:{}", hit.name, hit.chapter, hit.verse),
                snippet: texts::snippet(&hit.text, hit.start, hit.length),
            })
            .collect::<Vec<_>>();
        let message = if result.total == 0 {
            "No verses contain that.".to_string()
        } else if result.truncated {
            format!("500 of {} verses. The list stops at 500.", result.total)
        } else if result.total == 1 {
            "1 verse.".to_string()
        } else {
            format!("{} verses.", result.total)
        };
        self.search_summary = message.clone();
        message
    }

    fn set_version(&mut self, id: &str) -> Result<(), String> {
        if !self.settings.enabled.iter().any(|item| item == id) {
            return Err("Turn that text on first.".into());
        }
        let book = self.book_id.clone();
        let chapter = self.chapter;
        let verse = self.verse;
        self.settings.active = id.to_string();
        self.ensure(id)?;
        if self.go_to(&book, chapter, verse).is_err() {
            let fallback = self
                .books()
                .first()
                .map(|item| item.id.clone())
                .unwrap_or_else(|| "gen".into());
            let _ = self.go_to(&fallback, 1, None);
        }
        Ok(())
    }

    fn toggle_version(&mut self, id: &str) -> Result<String, String> {
        if id == "kjv" {
            return Err("KJV stays on.".into());
        }
        if !self.editions.contains_key(id) {
            return Err("That text is not installed.".into());
        }
        let label = self
            .editions
            .get(id)
            .map(|edition| edition.label.clone())
            .unwrap_or_else(|| id.to_string());
        if let Some(pos) = self.settings.enabled.iter().position(|item| item == id) {
            self.settings.enabled.remove(pos);
            if self.settings.active == id {
                self.settings.active = "kjv".into();
                self.ensure("kjv")?;
                let book = self.book_id.clone();
                let chapter = self.chapter;
                let verse = self.verse;
                if self.go_to(&book, chapter, verse).is_err() {
                    let _ = self.go_to("gen", 1, None);
                }
            }
            Ok(format!("{label} is off."))
        } else {
            self.settings.enabled.push(id.to_string());
            Ok(format!("{label} is on."))
        }
    }

    fn import_file(&mut self, source: &Path) -> Result<String, String> {
        let books = texts::load_bible_file(source)?;
        let stem = source
            .file_stem()
            .and_then(|text| text.to_str())
            .unwrap_or("import");
        let mut id = texts::slug(stem);
        if BUNDLED.iter().any(|item| item.id == id) {
            return Err("That name belongs to a built-in text. Rename the file and import it again.".into());
        }
        if id.is_empty() || id == "book" {
            id = format!("import-{}", self.settings.imports.len() + 1);
        }
        let label: String = stem
            .replace(['_', '-'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let label: String = if label.is_empty() {
            "Import".into()
        } else {
            label.chars().take(40).collect()
        };
        let file_name = format!("{id}.json");
        let dir = self.state_dir.join("imports");
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let dest = dir.join(&file_name);
        fs::copy(source, &dest).map_err(|err| err.to_string())?;
        self.settings.imports.retain(|record| record.id != id);
        self.settings.imports.push(ImportRec {
            id: id.clone(),
            label: label.clone(),
            file: file_name,
        });
        self.editions.insert(
            id.clone(),
            Edition {
                id: id.clone(),
                label: label.clone(),
                builtin: false,
                path: dest,
                books: Some(books),
            },
        );
        if !self.settings.enabled.iter().any(|item| item == &id) {
            self.settings.enabled.push(id.clone());
        }
        self.settings.active = id;
        self.rebuild_order();
        self.restore_place();
        Ok(label)
    }

    fn drop_import(&mut self, id: &str) -> Result<String, String> {
        let edition = self
            .editions
            .get(id)
            .ok_or_else(|| "That text is not installed.".to_string())?;
        if edition.builtin {
            return Err("Built-in texts stay in the library. Turn one off instead.".into());
        }
        let label = edition.label.clone();
        let path = edition.path.clone();
        self.editions.remove(id);
        self.settings.imports.retain(|record| record.id != id);
        self.settings.enabled.retain(|item| item != id);
        if self.settings.active == id {
            self.settings.active = "kjv".into();
            self.ensure("kjv")?;
            self.restore_place();
        }
        let _ = fs::remove_file(path);
        self.rebuild_order();
        self.settings.sanitize();
        Ok(label)
    }

    fn bump_size(&mut self, delta: i32) {
        self.settings.font_size = (self.settings.font_size + delta).clamp(FONT_MIN, FONT_MAX);
    }

    fn bump_gap(&mut self, delta: i32) {
        self.settings.verse_gap = (self.settings.verse_gap + delta).clamp(GAP_MIN, GAP_MAX);
    }

    fn set_theme(&mut self, mode: &str) {
        if matches!(mode, "light" | "dark" | "system") || theme::PALETTES.contains(&mode) {
            self.settings.theme_mode = mode.to_string();
        }
    }

    fn copy_text(&self) -> Result<String, String> {
        let verses = self.chapter_verses().to_vec();
        if verses.is_empty() {
            return Err("There is no chapter to copy.".into());
        }
        let name = self.book_name();
        let label = self.version_label();
        let book_id = self.book_id.clone();
        let chapter = self.chapter;
        let entries = if let Some(verse) = self.verse {
            vec![texts::PassageEntry {
                book_id,
                name,
                chapter,
                verse,
                text: verses.get(verse - 1).cloned().unwrap_or_default(),
            }]
        } else {
            verses
                .into_iter()
                .enumerate()
                .map(|(index, text)| texts::PassageEntry {
                    book_id: book_id.clone(),
                    name: name.clone(),
                    chapter,
                    verse: index + 1,
                    text,
                })
                .collect()
        };
        Ok(texts::format_passage(&entries, &label).text)
    }

    fn persist(&self) -> Result<(), String> {
        store::save(&self.settings_path, &self.settings)
    }

    fn flash(&mut self, message: impl Into<String>) {
        self.flash_message = message.into();
        self.flash_until = Some(Instant::now() + Duration::from_millis(3400));
    }

    fn status_line(&self, light: bool) -> String {
        if !self.load_error.is_empty() && self.books().is_empty() {
            return self.load_error.clone();
        }
        format!(
            "{}   ·   {}   ·   {}   ·   {} NOTES",
            self.version_label(),
            self.place_title(),
            theme::theme_label(&self.settings.theme_mode, light),
            self.settings.notes.len()
        )
    }

    fn current_status(&self, light: bool) -> String {
        if let Some(until) = self.flash_until {
            if Instant::now() < until {
                return self.flash_message.clone();
            }
        }
        self.status_line(light)
    }
}

fn clock_now() -> String {
    chrono::Local::now().format("%Y-%m-%d   %H:%M:%S").to_string()
}

fn desktop_line(scheme: Scheme) -> String {
    match scheme {
        Scheme::Light => "Desktop is light".into(),
        Scheme::Dark => "Desktop is dark".into(),
        Scheme::NoPreference => "Desktop has no preference, so this uses dark".into(),
    }
}

fn is_light(ui: &AppWindow) -> bool {
    ui.global::<Theme>().get_light()
}

fn show_status(ui: &AppWindow, core: &mut Core, message: impl Into<String>) {
    core.flash(message);
    let light = is_light(ui);
    ui.global::<Session>().set_status(core.current_status(light).into());
}

fn persist_ui(ui: &AppWindow, core: &Rc<std::cell::RefCell<Core>>) {
    let err = { core.borrow().persist().err() };
    if let Some(err) = err {
        show_status(ui, &mut core.borrow_mut(), format!("Could not save. {err}"));
    }
}

fn publish_chrome(ui: &AppWindow, core: &Core) {
    let light = is_light(ui);
    let session = ui.global::<Session>();
    session.set_place_title(core.place_title().into());
    session.set_version_label(core.version_label().into());
    session.set_continue_ref(core.place_title().into());
    session.set_continue_snippet(core.snippet().into());
    session.set_note_count(core.settings.notes.len() as i32);
    session.set_mark_count(core.settings.highlights.len() as i32);
    session.set_version_count(core.settings.enabled.len() as i32);
    session.set_reading_size(core.settings.font_size);
    session.set_verse_gap(core.settings.verse_gap);
    session.set_mark_color(core.settings.mark_color.clone().into());
    session.set_theme_mode(core.settings.theme_mode.clone().into());
    session.set_can_prev(core.can_step(-1));
    session.set_can_next(core.can_step(1));
    session.set_resolved_theme(theme::theme_label(&core.settings.theme_mode, light).into());
    session.set_status(core.current_status(light).into());
    session.set_search_summary(core.search_summary.clone().into());
    session.set_intro_on_start(core.settings.intro_on_startup);
    session.set_compare_on(core.compare_active());
    session.set_compare_label(core.compare_label().into());
    session.set_can_back(!core.history.is_empty());
    session.set_bookmark_count(core.settings.bookmarks.len() as i32);
    let here = core.verse_key().map(|key| core.settings.bookmarks.contains(&key)).unwrap_or(false);
    session.set_is_bookmarked(here);
}

fn publish_bookmarks(models: &Models, core: &Core) {
    let rows = core
        .settings
        .bookmarks
        .iter()
        .filter_map(|key| {
            let (book, chapter, verse) = texts::split_key(key)?;
            Some(BookmarkRow {
                reference: format!("{} {chapter}:{verse}", core.name_of(book)).into(),
                snippet: core.verse_text_of(key).into(),
            })
        })
        .collect::<Vec<_>>();
    models.bookmarks.set_vec(rows);
}

fn publish_books(models: &Models, core: &Core) {
    let rows = core
        .books()
        .iter()
        .map(|book| BookRow {
            id: book.id.clone().into(),
            name: book.name.clone().into(),
            chapters: book.chapters.len() as i32,
            current: book.id == core.book_id,
            section: texts::section_label(&book.id).into(),
        })
        .collect::<Vec<_>>();
    models.books.set_vec(rows);
}

fn publish_chapters(models: &Models, core: &Core) {
    let count = core.book().map(|book| book.chapters.len()).unwrap_or(0);
    let rows = (1..=count)
        .map(|num| ChapterRow {
            num: num as i32,
            current: num == core.chapter,
        })
        .collect::<Vec<_>>();
    models.chapters.set_vec(rows);
}

fn publish_verses(models: &Models, core: &Core) {
    let second = core.compare_verses();
    let col = core.col_width();
    let rows = core
        .chapter_verses()
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let number = index + 1;
            let key = texts::ref_key(&core.book_id, core.chapter, number);
            let size = core.settings.font_size as f32;
            let wrapped = flow::wrap(text, col, size);
            let wrapped2 = flow::wrap(second.get(index).map(String::as_str).unwrap_or(""), col, size);
            VerseRow {
                text2: flow::join(&wrapped2).into(),
                lines2: wrapped2.len() as i32,
                side: core.side_by_side(),
                number: number as i32,
                lines: wrapped.len().max(1) as i32,
                text: flow::join(&wrapped).into(),
                mark: core
                    .settings
                    .highlights
                    .get(&key)
                    .cloned()
                    .unwrap_or_default()
                    .into(),
                has_note: core.settings.notes.contains_key(&key),
                bookmarked: core.settings.bookmarks.contains(&key),
                selected: core.verse == Some(number),
            }
        })
        .collect::<Vec<_>>();
    models.verses.set_vec(rows);
}

fn sync_verse_row(models: &Models, core: &Core, number: i32) {
    if number <= 0 {
        return;
    }
    let index = (number - 1) as usize;
    let Some(mut row) = models.verses.row_data(index) else {
        return;
    };
    let key = texts::ref_key(&core.book_id, core.chapter, number as usize);
    row.mark = core
        .settings
        .highlights
        .get(&key)
        .cloned()
        .unwrap_or_default()
        .into();
    row.has_note = core.settings.notes.contains_key(&key);
    row.bookmarked = core.settings.bookmarks.contains(&key);
    row.selected = core.verse == Some(number as usize);
    models.verses.set_row_data(index, row);
}

fn publish_versions(models: &Models, core: &Core) {
    let rows = core
        .order
        .iter()
        .filter_map(|id| {
            let edition = core.editions.get(id)?;
            Some(VersionRow {
                id: edition.id.clone().into(),
                label: edition.label.clone().into(),
                enabled: core.settings.enabled.iter().any(|item| item == id),
                active: core.settings.active == *id,
                builtin: edition.builtin,
                compare: core.settings.compare_id == *id,
            })
        })
        .collect::<Vec<_>>();
    models.versions.set_vec(rows);
}

fn publish_live(models: &Models, core: &Core) {
    let rows = core
        .order
        .iter()
        .filter_map(|id| {
            if !core.settings.enabled.iter().any(|item| item == id) {
                return None;
            }
            let edition = core.editions.get(id)?;
            Some(LiveRow {
                id: edition.id.clone().into(),
                label: edition.label.clone().into(),
                active: core.settings.active == *id,
            })
        })
        .collect::<Vec<_>>();
    models.live.set_vec(rows);
}

fn hit_lines(core: &Core, snippet: &str) -> ModelRc<HitLine> {
    let lines = extras::highlight_lines(snippet, &core.terms, core.hit_width, 15.0)
        .into_iter()
        .map(|segs| HitLine {
            segs: ModelRc::new(VecModel::from(
                segs.into_iter()
                    .map(|(text, hl)| HitSeg { text: text.into(), hl })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(lines))
}

fn publish_hits(models: &Models, core: &Core) {
    let rows = core
        .last_hits
        .iter()
        .map(|hit| HitRow {
            reference: hit.reference.clone().into(),
            snippet: hit.snippet.clone().into(),
            lines: hit_lines(core, &hit.snippet),
        })
        .collect::<Vec<_>>();
    models.hits.set_vec(rows);
}

fn publish_word_hits(models: &Models, core: &Core) {
    let rows = core
        .last_hits
        .iter()
        .take(60)
        .map(|hit| HitRow {
            reference: hit.reference.clone().into(),
            snippet: hit.snippet.clone().into(),
            lines: ModelRc::default(),
        })
        .collect::<Vec<_>>();
    models.word_hits.set_vec(rows);
}

fn format_links(core: &Core, keys: &[String]) -> String {
    keys.iter()
        .filter_map(|key| {
            let (book, chapter, verse) = texts::split_key(key)?;
            Some(format!("{} {chapter}:{verse}", core.name_of(book)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn publish_note_fields(ui: &AppWindow, core: &Core, models: &Models) {
    let session = ui.global::<Session>();
    let open = core.verse.is_some();
    session.set_verse_open(open);
    session.set_verse_ref(if open { core.place_title().into() } else { "".into() });
    if let Some(verse) = core.verse {
        let key = texts::ref_key(&core.book_id, core.chapter, verse);
        let note = core.settings.notes.get(&key);
        session.set_note_text(note.map(|item| item.text.clone()).unwrap_or_default().into());
        session.set_link_text(format_links(core, note.map(|item| item.links.as_slice()).unwrap_or(&[])).into());
        let chips = note
            .map(|item| item.links.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|key| {
                let (book, chapter, verse) = texts::split_key(key)?;
                Some(LinkChip {
                    label: format!("{} {chapter}:{verse}", core.name_of(book)).into(),
                    book_id: book.into(),
                    chapter: chapter as i32,
                    verse: verse as i32,
                })
            })
            .collect::<Vec<_>>();
        models.links.set_vec(chips);
    } else {
        session.set_note_text("".into());
        session.set_link_text("".into());
        models.links.set_vec(Vec::new());
    }
}

fn publish_words(ui: &AppWindow, core: &Core, models: &Models) {
    let rows = core
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.word)
        .map(|(index, token)| WordChip {
            label: token.text.clone().into(),
            token: index as i32,
        })
        .collect::<Vec<_>>();
    models.words.set_vec(rows);
    let session = ui.global::<Session>();
    session.set_active_token(-1);
    session.set_sense_word("".into());
    session.set_sense_text("Select a word in the verse, or type one below.".into());
    session.set_modern_text("".into());
    session.set_query_word("".into());
    session.set_popup_open(false);
    models.word_hits.set_vec(Vec::new());
}

fn focus_current(ui: &AppWindow, core: &Core) {
    let session = ui.global::<Session>();
    session.set_focus_verse(core.verse.unwrap_or(0) as i32);
    session.set_focus_token(session.get_focus_token() + 1);
    session.set_chapter_token(session.get_chapter_token() + 1);
}

fn after_nav(ui: &AppWindow, core: &Core, models: &Models) {
    publish_bookmarks(models, core);
    publish_books(models, core);
    publish_chapters(models, core);
    publish_versions(models, core);
    publish_live(models, core);
    publish_note_fields(ui, core, models);
    publish_words(ui, core, models);
    focus_current(ui, core);
    publish_verses(models, core);
    publish_chrome(ui, core);
}

fn apply_theme(ui: &AppWindow, core: &mut Core) {
    let scheme = theme::system_scheme();
    core.last_scheme = scheme;
    let light = theme::resolved_light(&core.settings.theme_mode, scheme);
    ui.global::<Theme>().set_light(light);
    ui.global::<Theme>().set_palette(theme::palette_of(&core.settings.theme_mode).into());
    let session = ui.global::<Session>();
    session.set_desktop_appearance(desktop_line(scheme).into());
    session.set_resolved_theme(theme::theme_label(&core.settings.theme_mode, light).into());
    session.set_theme_mode(core.settings.theme_mode.clone().into());
}

fn sense_lines(sense: &texts::Sense) -> String {
    let mut lines = Vec::new();
    if !sense.pos.is_empty() {
        lines.push(sense.pos.to_uppercase());
    }
    lines.push(sense.archaic.clone());
    if !sense.modern.is_empty() {
        lines.push(format!("Modern gloss: {}", sense.modern));
    }
    lines.join("\n")
}

fn dict_key(sense: &texts::Sense) -> String {
    if sense.word.contains(' ') {
        sense.word.to_lowercase()
    } else if sense.surface.is_empty() {
        sense.word.to_lowercase()
    } else {
        texts::dictionary_word(&sense.surface)
    }
}

fn show_sense(ui: &AppWindow, core: &Rc<std::cell::RefCell<Core>>, sense: &texts::Sense, token: i32) {
    let session = ui.global::<Session>();
    session.set_active_token(token);
    session.set_sense_word(sense.word.to_uppercase().into());
    session.set_sense_text(sense_lines(sense).into());
    let key = dict_key(sense);
    let cache = core.borrow().state_dir.join("definitions.json");
    if key.is_empty() {
        session.set_modern_text("".into());
        return;
    }
    if let Some(hit) = define::cached(&cache, &key) {
        session.set_modern_text(hit.into());
        return;
    }
    session.set_modern_text("Looking up the modern sense…".into());
    let slot = core.borrow().define_token.clone();
    let generation = slot.fetch_add(1, Ordering::Relaxed) + 1;
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let text = define::lookup_modern(&cache, &key);
        let slot = slot.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            if slot.load(Ordering::Relaxed) != generation {
                return;
            }
            ui.global::<Session>().set_modern_text(text.into());
        });
    });
}

fn clean_word(text: &str) -> String {
    text.trim_matches(|c: char| !c.is_alphanumeric())
        .replace('\u{2019}', "'")
        .to_lowercase()
}

fn show_word_hits(ui: &AppWindow, core: &Rc<std::cell::RefCell<Core>>, models: &Models, word: &str) {
    let word = clean_word(word);
    ui.global::<Session>().set_query_word(word.clone().into());
    core.borrow_mut().search_words(&word, &extras::Scope::All);
    publish_hits(models, &core.borrow());
    publish_word_hits(models, &core.borrow());
    publish_chrome(ui, &core.borrow());
}

fn pick_and_publish(ui: &AppWindow, core: &Rc<std::cell::RefCell<Core>>, models: &Models, number: i32) {
    commit_note(ui, core, models, false);
    let previous = core.borrow().verse;
    core.borrow_mut().pick_verse(number as usize);
    persist_ui(ui, core);
    if let Some(prev) = previous {
        if Some(prev) != core.borrow().verse {
            sync_verse_row(models, &core.borrow(), prev as i32);
        }
    }
    sync_verse_row(models, &core.borrow(), number);
    publish_note_fields(ui, &core.borrow(), models);
    publish_words(ui, &core.borrow(), models);
    focus_current(ui, &core.borrow());
    publish_chrome(ui, &core.borrow());
}

/// Token index (in the selected verse) of the word under a press in the verse text.
fn word_under_press(core: &Core, number: i32, line: f32, x: f32) -> Option<usize> {
    let text = core.chapter_verses().get((number as usize).checked_sub(1)?)?.clone();
    let size = core.settings.font_size as f32;
    let lines = flow::wrap(&text, core.col_width(), size);
    let chunk = flow::hit(&lines, line, x, size)?;
    let chunks: Vec<String> = lines.into_iter().flatten().collect();
    let words_before: usize = chunks[..chunk]
        .iter()
        .map(|c| texts::tokenize(c).iter().filter(|t| t.word).count())
        .sum();
    if texts::tokenize(&chunks[chunk]).iter().all(|t| !t.word) {
        return None;
    }
    core.tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.word)
        .nth(words_before)
        .map(|(index, _)| index)
}

fn commit_note(ui: &AppWindow, core: &Rc<std::cell::RefCell<Core>>, models: &Models, announce: bool) {
    let text = ui.global::<Session>().get_note_text().to_string();
    let links = ui.global::<Session>().get_link_text().to_string();
    let verse = core.borrow().verse;
    let result = core.borrow_mut().store_note(&text, &links);
    match result {
        NoteSave::NoVerse | NoteSave::Unchanged => {}
        other => {
            persist_ui(ui, core);
            if let Some(number) = verse {
                sync_verse_row(models, &core.borrow(), number as i32);
            }
            publish_note_fields(ui, &core.borrow(), models);
            publish_chrome(ui, &core.borrow());
            let message = match other {
                NoteSave::Saved => "Note saved.".to_string(),
                NoteSave::Cleared => "Note removed.".to_string(),
                NoteSave::Skipped(count) => format!(
                    "Saved the note. {count} link{} did not name a verse.",
                    if count == 1 { "" } else { "s" }
                ),
                _ => String::new(),
            };
            if announce || matches!(other, NoteSave::Skipped(_)) {
                show_status(ui, &mut core.borrow_mut(), message);
            }
        }
    }
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if clipboard.set_text(text).is_ok() {
            return Ok(());
        }
    }
    let mut child = Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "The clipboard is not available.".to_string())?;
    use std::io::Write;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes()).map_err(|err| err.to_string())?;
    }
    drop(child.stdin.take());
    let status = child.wait().map_err(|err| err.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("The clipboard is not available.".into())
    }
}

fn session_intro(ui: &AppWindow, available: bool) {
    ui.global::<Session>().set_intro_available(available);
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    // The platform has to exist first. The native window reads this id when it is shown.
    slint::set_xdg_app_id("twm-bible-native")?;
    let core = Rc::new(std::cell::RefCell::new(Core::open()));
    let models = Models {
        books: Rc::new(VecModel::from(Vec::<BookRow>::new())),
        chapters: Rc::new(VecModel::from(Vec::<ChapterRow>::new())),
        verses: Rc::new(VecModel::from(Vec::<VerseRow>::new())),
        words: Rc::new(VecModel::from(Vec::<WordChip>::new())),
        hits: Rc::new(VecModel::from(Vec::<HitRow>::new())),
        versions: Rc::new(VecModel::from(Vec::<VersionRow>::new())),
        live: Rc::new(VecModel::from(Vec::<LiveRow>::new())),
        links: Rc::new(VecModel::from(Vec::<LinkChip>::new())),
        bookmarks: Rc::new(VecModel::from(Vec::<BookmarkRow>::new())),
        word_hits: Rc::new(VecModel::from(Vec::<HitRow>::new())),
    };
    let session = ui.global::<Session>();
    session.set_books(ModelRc::from(models.books.clone()));
    session.set_chapters(ModelRc::from(models.chapters.clone()));
    session.set_verses(ModelRc::from(models.verses.clone()));
    session.set_words(ModelRc::from(models.words.clone()));
    session.set_hits(ModelRc::from(models.hits.clone()));
    session.set_versions(ModelRc::from(models.versions.clone()));
    session.set_live_versions(ModelRc::from(models.live.clone()));
    session.set_links(ModelRc::from(models.links.clone()));
    session.set_bookmarks(ModelRc::from(models.bookmarks.clone()));
    session.set_word_hits(ModelRc::from(models.word_hits.clone()));
    session.set_clock(clock_now().into());
    apply_theme(&ui, &mut core.borrow_mut());
    after_nav(&ui, &core.borrow(), &models);

    let weak = ui.as_weak();

    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_commit_note(move || {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_save_note(move || {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, true);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_pick_verse(move |number| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let previous = core.borrow().verse;
            core.borrow_mut().pick_verse(number as usize);
            persist_ui(&ui, &core);
            if let Some(prev) = previous {
                if Some(prev) != core.borrow().verse {
                    sync_verse_row(&models, &core.borrow(), prev as i32);
                }
            }
            sync_verse_row(&models, &core.borrow(), number);
            publish_note_fields(&ui, &core.borrow(), &models);
            publish_words(&ui, &core.borrow(), &models);
            focus_current(&ui, &core.borrow());
            publish_chrome(&ui, &core.borrow());
        });
    }
    for hold in [false, true] {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        let handler = move |number: i32, line: f32, x: f32| {
            let Some(ui) = weak.upgrade() else { return };
            pick_and_publish(&ui, &core, &models, number);
            let index = word_under_press(&core.borrow(), number, line, x);
            if let Some(index) = index {
                if hold {
                    ui.global::<Session>().invoke_word_popup(index as i32);
                } else {
                    ui.global::<Session>().invoke_lookup_token(index as i32);
                }
            }
        };
        if hold {
            ui.global::<Session>().on_verse_word_hold(handler);
        } else {
            ui.global::<Session>().on_verse_word_click(handler);
        }
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_set_compare(move |on| {
            let Some(ui) = weak.upgrade() else { return };
            let result = core.borrow_mut().set_compare(on);
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    publish_verses(&models, &core.borrow());
                    publish_versions(&models, &core.borrow());
                    publish_chrome(&ui, &core.borrow());
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_set_compare_version(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            let result = core.borrow_mut().set_compare_version(id.as_str());
            match result {
                Ok(label) => {
                    persist_ui(&ui, &core);
                    publish_verses(&models, &core.borrow());
                    publish_versions(&models, &core.borrow());
                    publish_chrome(&ui, &core.borrow());
                    show_status(&ui, &mut core.borrow_mut(), format!("Second text: {label}."));
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        ui.global::<Session>().on_hit_width(move |width| {
            if width < 120.0 || (core.borrow().hit_width - width).abs() < 4.0 {
                return;
            }
            core.borrow_mut().hit_width = width;
            publish_hits(&models, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_export_passage(move |kind| {
            let Some(ui) = weak.upgrade() else { return };
            let session = ui.global::<Session>();
            let from = session.get_export_from().to_string();
            let to = session.get_export_to().to_string();
            let with_notes = session.get_export_notes();
            let built = {
                let app = core.borrow();
                let verses = app.chapter_verses();
                export::parse_range(&from, &to, verses.len()).map(|(a, b)| {
                    let list = (a..=b)
                        .map(|number| {
                            let key = texts::ref_key(&app.book_id, app.chapter, number);
                            let note = if with_notes {
                                app.settings.notes.get(&key).map(|n| n.text.clone()).filter(|t| !t.is_empty())
                            } else {
                                None
                            };
                            export::ExportVerse { number, text: verses[number - 1].clone(), note }
                        })
                        .collect::<Vec<_>>();
                    let range = if a == 1 && b == verses.len() { String::new() } else if a == b { format!(":{a}") } else { format!(":{a}-{b}") };
                    (format!("{} {}{range}", app.book_name(), app.chapter), app.version_label(), list)
                })
            };
            let (title, version, list) = match built {
                Ok(parts) => parts,
                Err(err) => {
                    show_status(&ui, &mut core.borrow_mut(), err);
                    return;
                }
            };
            let (label, ext, data): (&str, &str, Vec<u8>) = match kind.as_str() {
                "pdf" => ("PDF", "pdf", export::to_pdf(&title, &version, &list)),
                "html" => ("Web page (print friendly)", "html", export::to_html(&title, &version, &list).into_bytes()),
                _ => ("Text", "txt", export::to_text(&title, &version, &list).into_bytes()),
            };
            let Some(path) = rfd::FileDialog::new()
                .set_title("Export passage")
                .set_file_name(format!("{}.{ext}", export::file_stem(&title)))
                .add_filter(label, &[ext])
                .save_file()
            else {
                return;
            };
            let path = if path.extension().is_some() { path } else { path.with_extension(ext) };
            let message = match fs::write(&path, data) {
                Ok(()) => format!("Exported {title} to {}.", path.display()),
                Err(err) => format!("Could not export. {err}"),
            };
            session.set_export_popup(false);
            show_status(&ui, &mut core.borrow_mut(), message);
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_export_backup(move || {
            let Some(ui) = weak.upgrade() else { return };
            let day = chrono::Local::now().format("%Y-%m-%d").to_string();
            let Some(path) = rfd::FileDialog::new()
                .set_title("Export notes, highlights and bookmarks")
                .set_file_name(format!("twm-bible-backup-{day}.json"))
                .add_filter("JSON", &["json"])
                .save_file()
            else {
                return;
            };
            let path = if path.extension().is_some() { path } else { path.with_extension("json") };
            let backup = extras::make_backup(&core.borrow().settings, &day);
            let result = serde_json::to_vec_pretty(&backup)
                .map_err(|err| err.to_string())
                .and_then(|data| fs::write(&path, data).map_err(|err| err.to_string()));
            let message = match result {
                Ok(()) => format!(
                    "Exported {} notes, {} highlights, {} bookmarks to {}.",
                    backup.notes.len(),
                    backup.highlights.len(),
                    backup.bookmarks.len(),
                    path.display()
                ),
                Err(err) => format!("Could not export. {err}"),
            };
            show_status(&ui, &mut core.borrow_mut(), message);
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_import_backup(move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new()
                .set_title("Restore a TWM Bible backup")
                .add_filter("JSON", &["json"])
                .pick_file()
            else {
                return;
            };
            let parsed = fs::read_to_string(&path)
                .map_err(|err| err.to_string())
                .and_then(|text| extras::parse_backup(&text));
            match parsed {
                Ok(backup) => {
                    let text = format!(
                        "{} notes, {} highlights, {} bookmarks.",
                        backup.notes.len(),
                        backup.highlights.len(),
                        backup.bookmarks.len()
                    );
                    core.borrow_mut().pending_backup = Some(backup);
                    let session = ui.global::<Session>();
                    session.set_import_text(text.into());
                    session.set_import_popup(true);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_import_apply(move |replace| {
            let Some(ui) = weak.upgrade() else { return };
            ui.global::<Session>().set_import_popup(false);
            let Some(backup) = core.borrow_mut().pending_backup.take() else { return };
            {
                let app = core.borrow();
                // A safety copy of the settings as they were, in case the restore was a mistake.
                let _ = store::save(&app.state_dir.join("settings.pre-restore.json"), &app.settings);
            }
            extras::apply_backup(&mut core.borrow_mut().settings, &backup, replace);
            persist_ui(&ui, &core);
            after_nav(&ui, &core.borrow(), &models);
            let message = if replace { "Restored. Your previous settings were kept as settings.pre-restore.json." } else { "Merged the backup into your notes, highlights and bookmarks." };
            show_status(&ui, &mut core.borrow_mut(), message);
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_import_cancel(move || {
            let Some(ui) = weak.upgrade() else { return };
            core.borrow_mut().pending_backup = None;
            ui.global::<Session>().set_import_popup(false);
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_bump_gap(move |delta| {
            let Some(ui) = weak.upgrade() else { return };
            core.borrow_mut().bump_gap(delta);
            persist_ui(&ui, &core);
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_go_back(move || {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let result = core.borrow_mut().go_back();
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    ui.global::<Session>().set_view("read".into());
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_toggle_bookmark(move || {
            let Some(ui) = weak.upgrade() else { return };
            let result = core.borrow_mut().toggle_bookmark_here();
            match result {
                Ok(now) => {
                    persist_ui(&ui, &core);
                    if let Some(number) = core.borrow().verse {
                        sync_verse_row(&models, &core.borrow(), number as i32);
                    }
                    publish_bookmarks(&models, &core.borrow());
                    publish_chrome(&ui, &core.borrow());
                    let place = core.borrow().place_title();
                    let message = if now { format!("Bookmarked {place}.") } else { format!("Bookmark removed from {place}.") };
                    show_status(&ui, &mut core.borrow_mut(), message);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_open_bookmark(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let key = core.borrow().settings.bookmarks.get(index as usize).cloned();
            let Some((book, chapter, verse)) = key.as_deref().and_then(texts::split_key).map(|(b, c, v)| (b.to_string(), c, v)) else { return };
            let result = core.borrow_mut().go_to_tracked(&book, chapter, Some(verse));
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    ui.global::<Session>().set_view("read".into());
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_delete_bookmark(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let mut app = core.borrow_mut();
                if (index as usize) < app.settings.bookmarks.len() {
                    app.settings.bookmarks.remove(index as usize);
                }
            }
            persist_ui(&ui, &core);
            publish_verses(&models, &core.borrow());
            publish_bookmarks(&models, &core.borrow());
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_text_width(move |width| {
            let Some(ui) = weak.upgrade() else { return };
            if width < 100.0 || (core.borrow().text_width - width).abs() < 2.0 {
                return;
            }
            core.borrow_mut().text_width = width;
            publish_verses(&models, &core.borrow());
            let _ = &ui;
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_mark_verse(move |number| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let previous = core.borrow().verse;
            core.borrow_mut().mark_verse(number as usize);
            persist_ui(&ui, &core);
            if let Some(prev) = previous {
                if Some(prev) != core.borrow().verse {
                    sync_verse_row(&models, &core.borrow(), prev as i32);
                }
            }
            sync_verse_row(&models, &core.borrow(), number);
            publish_note_fields(&ui, &core.borrow(), &models);
            publish_words(&ui, &core.borrow(), &models);
            focus_current(&ui, &core.borrow());
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_set_mark(move |color| {
            let Some(ui) = weak.upgrade() else { return };
            let verse = core.borrow().verse;
            core.borrow_mut().set_mark_color(color.as_str());
            persist_ui(&ui, &core);
            if let Some(number) = verse {
                sync_verse_row(&models, &core.borrow(), number as i32);
            }
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_clear_mark(move || {
            let Some(ui) = weak.upgrade() else { return };
            let verse = core.borrow().verse;
            core.borrow_mut().clear_mark();
            persist_ui(&ui, &core);
            if let Some(number) = verse {
                sync_verse_row(&models, &core.borrow(), number as i32);
            }
            publish_chrome(&ui, &core.borrow());
            show_status(&ui, &mut core.borrow_mut(), "Highlight removed.");
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_copy_verse(move || {
            let Some(ui) = weak.upgrade() else { return };
            let text = core.borrow().copy_text();
            match text.and_then(|text| copy_to_clipboard(&text).map(|()| text)) {
                Ok(_) => {
                    let place = core.borrow().place_title();
                    show_status(&ui, &mut core.borrow_mut(), format!("Copied {place}."));
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_lookup_token(move |token| {
            let Some(ui) = weak.upgrade() else { return };
            let (sense, word) = {
                let app = core.borrow();
                let word = app.tokens.get(token as usize).map(|t| t.text.clone()).unwrap_or_default();
                (texts::lookup_in_tokens(&app.tokens, token as usize, &app.glossary), word)
            };
            show_sense(&ui, &core, &sense, token);
            show_word_hits(&ui, &core, &models, &word);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_word_popup(move |token| {
            let Some(ui) = weak.upgrade() else { return };
            let (sense, word) = {
                let app = core.borrow();
                let word = app.tokens.get(token as usize).map(|t| t.text.clone()).unwrap_or_default();
                (texts::lookup_in_tokens(&app.tokens, token as usize, &app.glossary), word)
            };
            show_sense(&ui, &core, &sense, token);
            show_word_hits(&ui, &core, &models, &word);
            ui.global::<Session>().set_popup_open(true);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_search_this_word(move || {
            let Some(ui) = weak.upgrade() else { return };
            let word = ui.global::<Session>().get_query_word().to_string();
            show_word_hits(&ui, &core, &models, &word);
            let session = ui.global::<Session>();
            session.set_popup_open(false);
            session.set_view("search".into());
            let summary = core.borrow().search_summary.clone();
            show_status(&ui, &mut core.borrow_mut(), format!("{word}: {summary}"));
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_play_intro(move || {
            let Some(ui) = weak.upgrade() else { return };
            match twm_bible_native::intro::find() {
                Some(path) => {
                    if !twm_bible_native::intro::play(&path) {
                        show_status(&ui, &mut core.borrow_mut(), "No video player was found for the intro.");
                    }
                }
                None => show_status(&ui, &mut core.borrow_mut(), "The intro video is missing."),
            }
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_set_intro_on_start(move |on| {
            let Some(ui) = weak.upgrade() else { return };
            core.borrow_mut().settings.intro_on_startup = on;
            persist_ui(&ui, &core);
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_lookup_typed(move |text| {
            let Some(ui) = weak.upgrade() else { return };
            let word = text.trim().to_string();
            if word.is_empty() {
                show_status(&ui, &mut core.borrow_mut(), "Type a word to look up.");
                return;
            }
            let sense = texts::archaic_for(&word, &core.borrow().glossary);
            show_sense(&ui, &core, &sense, -1);
            show_word_hits(&ui, &core, &models, &word);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_step(move |delta| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let result = core.borrow_mut().step(delta);
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_select_book(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let id = id.to_string();
            let result = core.borrow_mut().go_to(&id, 1, None);
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    ui.global::<Session>().set_view("read".into());
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_select_chapter(move |chapter| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let book = core.borrow().book_id.clone();
            let result = core.borrow_mut().go_to(&book, chapter as usize, None);
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_set_theme(move |mode| {
            let Some(ui) = weak.upgrade() else { return };
            {
                let mut app = core.borrow_mut();
                app.set_theme(mode.as_str());
            }
            apply_theme(&ui, &mut core.borrow_mut());
            persist_ui(&ui, &core);
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_bump_size(move |delta| {
            let Some(ui) = weak.upgrade() else { return };
            core.borrow_mut().bump_size(delta);
            publish_verses(&models, &core.borrow());
            persist_ui(&ui, &core);
            publish_chrome(&ui, &core.borrow());
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_set_version(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let id = id.to_string();
            let result = core.borrow_mut().set_version(&id);
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    after_nav(&ui, &core.borrow(), &models);
                    let label = core.borrow().version_label();
                    show_status(&ui, &mut core.borrow_mut(), format!("Reading {label}."));
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_toggle_version(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            let id = id.to_string();
            let result = core.borrow_mut().toggle_version(&id);
            match result {
                Ok(message) => {
                    persist_ui(&ui, &core);
                    after_nav(&ui, &core.borrow(), &models);
                    show_status(&ui, &mut core.borrow_mut(), message);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_drop_version(move |id| {
            let Some(ui) = weak.upgrade() else { return };
            let id = id.to_string();
            let result = core.borrow_mut().drop_import(&id);
            match result {
                Ok(label) => {
                    persist_ui(&ui, &core);
                    after_nav(&ui, &core.borrow(), &models);
                    show_status(&ui, &mut core.borrow_mut(), format!("Removed {label}. The original file was left in place."));
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_import_bible(move || {
            let Some(ui) = weak.upgrade() else { return };
            let file = rfd::FileDialog::new()
                .set_title("Import a Bible JSON file")
                .add_filter("JSON", &["json"])
                .pick_file();
            let Some(file) = file else { return };
            let result = core.borrow_mut().import_file(&file);
            match result {
                Ok(label) => {
                    persist_ui(&ui, &core);
                    after_nav(&ui, &core.borrow(), &models);
                    show_status(&ui, &mut core.borrow_mut(), format!("Imported {label}."));
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_continue_reading(move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.global::<Session>().set_view("read".into());
            after_nav(&ui, &core.borrow(), &models);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_open_hit(move |index| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let hit = core.borrow().last_hits.get(index as usize).map(|hit| {
                (hit.book_id.clone(), hit.chapter, hit.verse)
            });
            let Some((book, chapter, verse)) = hit else { return };
            let result = core.borrow_mut().go_to_tracked(&book, chapter, Some(verse));
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    ui.global::<Session>().set_view("read".into());
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_open_link(move |book, chapter, verse| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let book = book.to_string();
            let result = core.borrow_mut().go_to_tracked(&book, chapter as usize, Some(verse as usize));
            match result {
                Ok(()) => {
                    persist_ui(&ui, &core);
                    ui.global::<Session>().set_view("read".into());
                    after_nav(&ui, &core.borrow(), &models);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_search_words(move |query| {
            let Some(ui) = weak.upgrade() else { return };
            let scope = {
                let session = ui.global::<Session>();
                let book = session.get_search_book().to_string();
                if !book.trim().is_empty() {
                    match texts::parse_reference(&book) {
                        Some(parsed) => extras::Scope::Book(parsed.book_id.to_string()),
                        None => {
                            show_status(&ui, &mut core.borrow_mut(), "No book by that name in the book filter.");
                            return;
                        }
                    }
                } else {
                    match session.get_search_scope().as_str() {
                        "ot" => extras::Scope::Old,
                        "nt" => extras::Scope::New,
                        _ => extras::Scope::All,
                    }
                }
            };
            let message = core.borrow_mut().search_words(query.as_str(), &scope);
            publish_hits(&models, &core.borrow());
            publish_chrome(&ui, &core.borrow());
            show_status(&ui, &mut core.borrow_mut(), message);
        });
    }
    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.global::<Session>().on_search_passage(move |query| {
            let Some(ui) = weak.upgrade() else { return };
            commit_note(&ui, &core, &models, false);
            let query = query.to_string();
            let result = core.borrow_mut().open_query(&query);
            match result {
                Ok(message) => {
                    persist_ui(&ui, &core);
                    ui.global::<Session>().set_view("read".into());
                    after_nav(&ui, &core.borrow(), &models);
                    show_status(&ui, &mut core.borrow_mut(), message);
                }
                Err(err) => show_status(&ui, &mut core.borrow_mut(), err),
            }
        });
    }

    {
        let core = core.clone();
        let models = models.clone();
        let weak = weak.clone();
        ui.window().on_close_requested(move || {
            if let Some(ui) = weak.upgrade() {
                commit_note(&ui, &core, &models, false);
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    let timer_ui = ui.as_weak();
    let timer_core = core.clone();
    let timer = slint::Timer::default();
    let mut ticks = 0u32;
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = timer_ui.upgrade() else { return };
        ticks = ticks.wrapping_add(1);
        ui.global::<Session>().set_clock(clock_now().into());
        if ticks % 3 == 0 {
            let scheme = theme::system_scheme();
            let mode = timer_core.borrow().settings.theme_mode.clone();
            let previous = timer_core.borrow().last_scheme;
            if scheme != previous || mode == "system" {
                apply_theme(&ui, &mut timer_core.borrow_mut());
            }
        }
        let light = is_light(&ui);
        let status = timer_core.borrow().current_status(light);
        ui.global::<Session>().set_status(status.into());
    });

    let intro = twm_bible_native::intro::find();
    session_intro(&ui, intro.is_some());
    let first_run = !core.borrow().settings.intro_seen;
    let every_run = core.borrow().settings.intro_on_startup;
    if intro.is_some() && (first_run || every_run) {
        if first_run {
            core.borrow_mut().settings.intro_seen = true;
            persist_ui(&ui, &core);
        }
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(800), move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Session>().invoke_play_intro();
            }
        });
    }

    ui.run()?;
    drop(timer);
    Ok(())
}
