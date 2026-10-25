//! Study Desk: sermon documents (block based), simple notes, passage parsing and export.
//!
//! A sermon is a list of blocks. Each block has a kind (paragraph, heading, list item,
//! quote, image, slide) and its own font, size and alignment. Inside the text of a block,
//! light inline markup is kept: `**bold**`, `*italic*` and `__underline__`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::bible::{self, Book, PassageEntry};
use crate::outreach::{b64_encode, write_private};

pub const KINDS: &[&str] = &["p", "h1", "h2", "h3", "bullet", "number", "quote", "image", "slide"];
pub const FONTS: &[&str] = &["Rajdhani", "Serif", "Sans", "Share Tech Mono", "Orbitron"];
pub const SIZE_MIN: i32 = 10;
pub const SIZE_MAX: i32 = 72;
pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];
pub const IMAGE_MAX_BYTES: u64 = 30 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: u64,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    /// Quote: the reference line. Slide: speaker notes (unused in the UI for now).
    #[serde(default)]
    pub extra: String,
    /// File name inside the images folder, for image and slide blocks.
    #[serde(default)]
    pub image: String,
    /// Empty means the document default.
    #[serde(default)]
    pub font: String,
    /// 0 means the default size for the kind.
    #[serde(default)]
    pub size: i32,
    #[serde(default = "default_align")]
    pub align: String,
}

fn default_kind() -> String {
    "p".into()
}
fn default_align() -> String {
    "left".into()
}

impl Block {
    pub fn new(id: u64, kind: &str, text: &str) -> Block {
        Block {
            id,
            kind: kind.into(),
            text: text.into(),
            extra: String::new(),
            image: String::new(),
            font: String::new(),
            size: 0,
            align: default_align(),
        }
    }
    pub fn is_text(&self) -> bool {
        !matches!(self.kind.as_str(), "image")
    }
    pub fn sanitize(&mut self) {
        if !KINDS.contains(&self.kind.as_str()) {
            self.kind = default_kind();
        }
        if !matches!(self.align.as_str(), "left" | "center" | "right") {
            self.align = default_align();
        }
        if !self.font.is_empty() && !FONTS.contains(&self.font.as_str()) {
            self.font.clear();
        }
        if self.size != 0 {
            self.size = self.size.clamp(SIZE_MIN, SIZE_MAX);
        }
        // Image names are only ever plain file names that this module wrote.
        if self.image.contains('/') || self.image.contains('\\') || self.image.starts_with('.') {
            self.image.clear();
        }
    }
}

/// Default point size for a block kind, given the document base size.
pub fn kind_size(kind: &str, base: i32) -> i32 {
    match kind {
        "h1" => base + 14,
        "h2" => base + 8,
        "h3" => base + 4,
        "slide" => base + 6,
        _ => base,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sermon {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub updated: u64,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_size")]
    pub size: i32,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

fn default_font() -> String {
    "Rajdhani".into()
}
fn default_size() -> i32 {
    18
}

impl Sermon {
    pub fn new(id: String, title: &str) -> Sermon {
        let now = now_secs();
        Sermon {
            id,
            title: title.into(),
            created: now,
            updated: now,
            font: default_font(),
            size: default_size(),
            blocks: vec![Block::new(1, "h1", title), Block::new(2, "p", "")],
        }
    }
    pub fn sanitize(&mut self) {
        if !FONTS.contains(&self.font.as_str()) {
            self.font = default_font();
        }
        self.size = self.size.clamp(SIZE_MIN, SIZE_MAX);
        let mut seen = BTreeSet::new();
        let mut next = self.blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        for block in &mut self.blocks {
            block.sanitize();
            if block.id == 0 || !seen.insert(block.id) {
                block.id = next;
                seen.insert(next);
                next += 1;
            }
        }
        if self.blocks.is_empty() {
            self.blocks.push(Block::new(next, "p", ""));
        }
        if self.title.trim().is_empty() {
            self.title = "Untitled sermon".into();
        }
    }
    pub fn next_block_id(&self) -> u64 {
        self.blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1
    }
    /// The number shown beside numbered-list items and slides (0 for other blocks).
    pub fn markers(&self) -> Vec<i32> {
        let mut out = Vec::with_capacity(self.blocks.len());
        let mut run = 0;
        let mut slides = 0;
        for block in &self.blocks {
            match block.kind.as_str() {
                "number" => {
                    run += 1;
                    out.push(run);
                }
                "slide" => {
                    run = 0;
                    slides += 1;
                    out.push(slides);
                }
                _ => {
                    run = 0;
                    out.push(0);
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub updated: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NoteBook {
    #[serde(default)]
    pub notes: Vec<Note>,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Files for the Study Desk, under `<state dir>/studydesk/`.
pub struct DeskStore {
    pub dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SermonInfo {
    pub id: String,
    pub title: String,
    pub updated: u64,
}

impl DeskStore {
    pub fn new(state_dir: &Path) -> DeskStore {
        DeskStore { dir: state_dir.join("studydesk") }
    }
    pub fn sermons_dir(&self) -> PathBuf {
        self.dir.join("sermons")
    }
    pub fn images_dir(&self) -> PathBuf {
        self.dir.join("images")
    }
    fn notes_path(&self) -> PathBuf {
        self.dir.join("notes.json")
    }
    pub fn fresh_id(&self, prefix: &str) -> String {
        let base = format!("{prefix}{}", now_millis());
        let mut id = base.clone();
        let mut n = 1;
        while self.sermons_dir().join(format!("{id}.json")).exists() {
            n += 1;
            id = format!("{base}-{n}");
        }
        id
    }
    pub fn list(&self) -> Vec<SermonInfo> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(self.sermons_dir()) else { return out };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            if let Some(s) = self.load(stem) {
                out.push(SermonInfo { id: s.id, title: s.title, updated: s.updated });
            }
        }
        out.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
        out
    }
    pub fn load(&self, id: &str) -> Option<Sermon> {
        if !valid_id(id) {
            return None;
        }
        let text = fs::read_to_string(self.sermons_dir().join(format!("{id}.json"))).ok()?;
        let mut s: Sermon = serde_json::from_str(&text).ok()?;
        s.id = id.to_string();
        s.sanitize();
        Some(s)
    }
    pub fn save(&self, sermon: &Sermon) -> Result<(), String> {
        if !valid_id(&sermon.id) {
            return Err("bad sermon id".into());
        }
        private_dir(&self.dir)?;
        private_dir(&self.sermons_dir())?;
        let data = serde_json::to_vec_pretty(sermon).map_err(|e| e.to_string())?;
        write_private(&self.sermons_dir().join(format!("{}.json", sermon.id)), &data)
    }
    /// Deletes the sermon file and the images that belong to it.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        if !valid_id(id) {
            return Err("bad sermon id".into());
        }
        let path = self.sermons_dir().join(format!("{id}.json"));
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let prefix = format!("{id}_");
        if let Ok(entries) = fs::read_dir(self.images_dir()) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
    /// Copies an image into the private images folder. Returns the stored file name.
    pub fn import_image(&self, sermon_id: &str, src: &Path) -> Result<String, String> {
        if !valid_id(sermon_id) {
            return Err("bad sermon id".into());
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !IMAGE_EXTS.contains(&ext.as_str()) {
            return Err("Choose a PNG, JPEG, GIF, WebP or BMP image.".into());
        }
        let meta = fs::metadata(src).map_err(|e| e.to_string())?;
        if meta.len() > IMAGE_MAX_BYTES {
            return Err("That image is larger than 30 MB.".into());
        }
        let bytes = fs::read(src).map_err(|e| e.to_string())?;
        image::load_from_memory(&bytes).map_err(|_| "That file is not an image the app can read.".to_string())?;
        private_dir(&self.dir)?;
        private_dir(&self.images_dir())?;
        let mut name = format!("{sermon_id}_{}.{ext}", now_millis());
        let mut n = 1;
        while self.images_dir().join(&name).exists() {
            n += 1;
            name = format!("{sermon_id}_{}-{n}.{ext}", now_millis());
        }
        write_private(&self.images_dir().join(&name), &bytes)?;
        Ok(name)
    }
    pub fn image_path(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
            return None;
        }
        let path = self.images_dir().join(name);
        path.exists().then_some(path)
    }
    pub fn load_notes(&self) -> NoteBook {
        fs::read_to_string(self.notes_path())
            .ok()
            .and_then(|t| serde_json::from_str::<NoteBook>(&t).ok())
            .unwrap_or_default()
    }
    pub fn save_notes(&self, book: &NoteBook) -> Result<(), String> {
        private_dir(&self.dir)?;
        let data = serde_json::to_vec_pretty(book).map_err(|e| e.to_string())?;
        write_private(&self.notes_path(), &data)
    }
}

// ---------------------------------------------------------------- inline markup

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Splits `**b**`, `*i*` and `__u__` markup into styled runs. Unpaired markers stay literal.
pub fn spans(text: &str) -> Vec<(Style, String)> {
    #[derive(Clone, Copy, PartialEq)]
    enum M {
        B,
        I,
        U,
    }
    // Tokenize into markers and text.
    let mut toks: Vec<Result<M, String>> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut buf = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() && matches!(chars[i + 1], '*' | '_' | '\\') {
            buf.push(chars[i + 1]);
            i += 2;
            continue;
        }
        let marker = if c == '*' && chars.get(i + 1) == Some(&'*') {
            Some((M::B, 2))
        } else if c == '_' && chars.get(i + 1) == Some(&'_') {
            Some((M::U, 2))
        } else if c == '*' {
            Some((M::I, 1))
        } else {
            None
        };
        if let Some((m, len)) = marker {
            if !buf.is_empty() {
                toks.push(Err(std::mem::take(&mut buf)));
            }
            toks.push(Ok(m));
            i += len;
        } else {
            buf.push(c);
            i += 1;
        }
    }
    if !buf.is_empty() {
        toks.push(Err(buf));
    }
    // A marker with no partner later on is literal text.
    let lit = |m: M| match m {
        M::B => "**",
        M::I => "*",
        M::U => "__",
    };
    let mut paired = vec![false; toks.len()];
    for m in [M::B, M::I, M::U] {
        let idx: Vec<usize> = toks.iter().enumerate().filter(|(_, t)| matches!(t, Ok(x) if *x == m)).map(|(i, _)| i).collect();
        let even = idx.len() - idx.len() % 2;
        for &k in &idx[..even] {
            paired[k] = true;
        }
    }
    let mut style = Style::default();
    let mut out: Vec<(Style, String)> = Vec::new();
    for (k, tok) in toks.into_iter().enumerate() {
        let piece = match tok {
            Ok(m) if paired[k] => {
                match m {
                    M::B => style.bold = !style.bold,
                    M::I => style.italic = !style.italic,
                    M::U => style.underline = !style.underline,
                }
                continue;
            }
            Ok(m) => lit(m).to_string(),
            Err(s) => s,
        };
        match out.last_mut() {
            Some((st, s)) if *st == style => s.push_str(&piece),
            _ => out.push((style, piece)),
        }
    }
    out
}

/// Removes inline markup and leaves the plain text.
pub fn plain(text: &str) -> String {
    spans(text).into_iter().map(|(_, s)| s).collect()
}

fn md_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if c == '\n' {
            out.push_str("\\\n");
        } else if c.is_ascii_punctuation() {
            out.push('\\');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

/// CommonMark (with `<u>`) for Slint's StyledText. Every literal character is escaped,
/// so the only markup that can come out is the emphasis this function writes.
pub fn to_styled_markdown(text: &str, force_bold: bool) -> String {
    let mut out = String::new();
    for (mut st, s) in spans(text) {
        st.bold |= force_bold;
        // Emphasis delimiters must touch non-space characters, so keep edge spaces outside.
        let core = s.trim_matches(|c: char| c == ' ' || c == '\t');
        let lead = &s[..s.len() - s.trim_start_matches([' ', '\t']).len()];
        let trail = &s[s.trim_end_matches([' ', '\t']).len()..];
        if core.is_empty() {
            out.push_str(&md_escape(&s));
            continue;
        }
        out.push_str(lead);
        let mut open = String::new();
        let mut close = String::new();
        if st.underline {
            open.push_str("<u>");
            close.insert_str(0, "</u>");
        }
        if st.bold {
            open.push_str("**");
            close.insert_str(0, "**");
        }
        if st.italic {
            open.push('*');
            close.insert(0, '*');
        }
        out.push_str(&open);
        out.push_str(&md_escape(core));
        out.push_str(&close);
        out.push_str(trail);
    }
    out
}

pub fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn to_html_inline(text: &str) -> String {
    let mut out = String::new();
    for (st, s) in spans(text) {
        let mut body = html_escape(&s).replace('\n', "<br>\n");
        if st.italic {
            body = format!("<em>{body}</em>");
        }
        if st.bold {
            body = format!("<strong>{body}</strong>");
        }
        if st.underline {
            body = format!("<u>{body}</u>");
        }
        out.push_str(&body);
    }
    out
}

/// Wraps (or unwraps) the byte range `[a, b)` of `text` in a marker such as `**`.
/// With an empty range the whole text is toggled. Returns the new text and new range.
pub fn toggle_marker(text: &str, a: usize, b: usize, marker: &str) -> (String, usize, usize) {
    let (mut a, mut b) = (a.min(b), a.max(b));
    a = a.min(text.len());
    b = b.min(text.len());
    while !text.is_char_boundary(a) {
        a -= 1;
    }
    while !text.is_char_boundary(b) {
        b += 1;
    }
    if a == b {
        a = 0;
        b = text.len();
    }
    let m = marker.len();
    let inner = &text[a..b];
    // Selection includes the markers.
    if inner.len() >= 2 * m && inner.starts_with(marker) && inner.ends_with(marker) && !(marker == "*" && inner.starts_with("**") && !inner.starts_with("***")) {
        let new = format!("{}{}{}", &text[..a], &inner[m..inner.len() - m], &text[b..]);
        return (new, a, b - 2 * m);
    }
    // Markers sit just outside the selection.
    if a >= m && text[..a].ends_with(marker) && text[b..].starts_with(marker) {
        let outer_ok = marker != "*" || !(text[..a - 1].ends_with('*') && text[b + 1..].starts_with('*'));
        if outer_ok {
            let new = format!("{}{}{}", &text[..a - m], inner, &text[b + m..]);
            return (new, a - m, b - m);
        }
    }
    if inner.trim().is_empty() {
        return (text.to_string(), a, b);
    }
    // Keep spaces at the edges outside the markers.
    let lead = inner.len() - inner.trim_start().len();
    let trail = inner.len() - inner.trim_end().len();
    let (ia, ib) = (a + lead, b - trail);
    let new = format!("{}{marker}{}{marker}{}", &text[..ia], &text[ia..ib], &text[ib..]);
    (new, ia, ib + 2 * m)
}

// ---------------------------------------------------------------- passages

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub book_id: &'static str,
    pub chapter: u32,
    /// 0 means from the first verse.
    pub verse: u32,
    pub end_chapter: u32,
    /// 0 means to the last verse of end_chapter.
    pub end_verse: u32,
}

fn book_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^\s*((?:[1-3]|i{1,3}|first|second|third|1st|2nd|3rd)?\s*[a-z][a-z .]*?)\s*(\d[\d:\s,\-–—.]*)?$").unwrap())
}

/// Parses references such as `John 3:16-18`, `Ps 23`, `Rom 5:8; 6:23`, `John 3:16,18`,
/// `Gen 1:1-2:3` and `Jude 3` (single-chapter books take a verse).
pub fn parse_passage(input: &str) -> Result<Vec<Span>, String> {
    let mut out = Vec::new();
    let mut book: Option<&'static str> = None;
    let cleaned = input.replace(['–', '—'], "-");
    for part in cleaned.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let numbers;
        if part.starts_with(|c: char| c.is_ascii_digit()) && book.is_some() && !book_re().is_match(part) || part.chars().all(|c| c.is_ascii_digit() || ":-, ".contains(c)) {
            if book.is_none() {
                return Err(format!("Add a book name to \"{part}\"."));
            }
            numbers = part.to_string();
        } else {
            let caps = book_re().captures(part).ok_or_else(|| format!("Could not read \"{part}\"."))?;
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let parsed = bible::parse_reference(name).filter(|r| r.chapter.is_none()).ok_or_else(|| format!("Unknown book \"{}\".", name.trim()))?;
            book = Some(parsed.book_id);
            numbers = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
        }
        let book_id = book.unwrap();
        let single = bible::book_by_id(book_id).map(|b| b.chapters == 1).unwrap_or(false);
        let numbers = numbers.replace(' ', "");
        if numbers.is_empty() {
            out.push(Span { book_id, chapter: 1, verse: 0, end_chapter: if single { 1 } else { 1 }, end_verse: 0 });
            continue;
        }
        let mut chapter: Option<u32> = None;
        for piece in numbers.split(',') {
            if piece.is_empty() {
                continue;
            }
            let (left, right) = match piece.split_once('-') {
                Some((l, r)) => (l, Some(r)),
                None => (piece, None),
            };
            let num = |s: &str| s.trim_end_matches('.').parse::<u32>().map_err(|_| format!("Could not read \"{piece}\"."));
            // Start point.
            let (sc, sv) = if let Some((c, v)) = left.split_once(':') {
                (num(c)?, num(v)?)
            } else if single {
                (1, num(left)?)
            } else if let Some(c) = chapter {
                (c, num(left)?)
            } else {
                (num(left)?, 0)
            };
            if sc == 0 {
                return Err("Chapters start at 1.".into());
            }
            let (ec, ev) = match right {
                None => (sc, sv),
                Some(r) => {
                    if let Some((c, v)) = r.split_once(':') {
                        (num(c)?, num(v)?)
                    } else if sv == 0 {
                        (num(r)?, 0)
                    } else {
                        (sc, num(r)?)
                    }
                }
            };
            if (ec, if ev == 0 { u32::MAX } else { ev }) < (sc, sv) {
                return Err(format!("\"{piece}\" runs backwards."));
            }
            chapter = Some(sc).filter(|_| sv > 0 || left.contains(':'));
            if sv > 0 {
                chapter = Some(ec);
            }
            out.push(Span { book_id, chapter: sc, verse: sv, end_chapter: ec, end_verse: ev });
        }
    }
    if out.is_empty() {
        return Err("Type a passage, such as John 3:16-18.".into());
    }
    Ok(out)
}

/// Resolves spans against a loaded Bible. Verses past the end of a chapter are dropped.
pub fn resolve(books: &[Book], spans: &[Span]) -> Result<Vec<PassageEntry>, String> {
    let mut out = Vec::new();
    for span in spans {
        let book = books
            .iter()
            .find(|b| b.id == span.book_id)
            .ok_or_else(|| format!("{} is not in this text.", bible::book_by_id(span.book_id).map(|b| b.name).unwrap_or(span.book_id)))?;
        let chapters = book.chapters.len() as u32;
        if span.chapter > chapters {
            return Err(format!("{} has {chapters} chapters.", book.name));
        }
        let end_c = span.end_chapter.min(chapters);
        for c in span.chapter..=end_c {
            let verses = &book.chapters[(c - 1) as usize];
            let first = if c == span.chapter { span.verse.max(1) } else { 1 };
            let last = if c == end_c && span.end_verse > 0 { span.end_verse.min(verses.len() as u32) } else { verses.len() as u32 };
            if c == span.chapter && first > verses.len() as u32 {
                return Err(format!("{} {c} has {} verses.", book.name, verses.len()));
            }
            for v in first..=last {
                let text = &verses[(v - 1) as usize];
                if text.trim().is_empty() {
                    continue;
                }
                out.push(PassageEntry {
                    book_id: book.id.clone(),
                    name: book.name.clone(),
                    chapter: c as usize,
                    verse: v as usize,
                    text: text.clone(),
                });
            }
        }
    }
    if out.is_empty() {
        return Err("No verses found for that passage.".into());
    }
    Ok(out)
}

/// Word or phrase search, reusing the reader's search (case-insensitive, whitespace-tolerant).
pub fn search(books: &[Book], query: &str, limit: usize) -> bible::Search {
    bible::search_verses(books, query, limit)
}

/// Canonical sort position for a verse key.
pub fn canon_pos(book_id: &str, chapter: usize, verse: usize) -> (usize, usize, usize) {
    let b = bible::CANON.iter().position(|b| b.id == book_id).unwrap_or(usize::MAX);
    (b, chapter, verse)
}

/// The quote block text for a set of verses, and its reference line.
pub fn quote_for(entries: &[PassageEntry], version_label: &str) -> (String, String) {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|e| canon_pos(&e.book_id, e.chapter, e.verse));
    sorted.dedup_by(|a, b| a.book_id == b.book_id && a.chapter == b.chapter && a.verse == b.verse);
    let passage = bible::format_passage(&sorted, version_label);
    let multi = sorted.len() > 1;
    let body = sorted
        .iter()
        .map(|e| if multi { format!("{} {}", e.verse, e.text.trim()) } else { e.text.trim().to_string() })
        .collect::<Vec<_>>()
        .join(" ");
    (escape_markup(&body), passage.reference)
}

/// Escapes characters that would otherwise read as inline markup.
pub fn escape_markup(text: &str) -> String {
    text.replace('\\', "\\\\").replace('*', "\\*").replace('_', "\\_")
}

// ---------------------------------------------------------------- export

fn css_font(font: &str) -> &'static str {
    match font {
        "Serif" => "Georgia, 'Times New Roman', serif",
        "Sans" => "'Helvetica Neue', Arial, sans-serif",
        "Share Tech Mono" => "'Share Tech Mono', 'Courier New', monospace",
        "Orbitron" => "Orbitron, 'Arial Black', sans-serif",
        _ => "Rajdhani, 'Segoe UI', Arial, sans-serif",
    }
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    }
}

/// A standalone HTML page. Images are embedded so the file can be moved or emailed.
pub fn export_html(sermon: &Sermon, store: &DeskStore) -> String {
    let mut body = String::new();
    let markers = sermon.markers();
    let mut list_open: Option<&str> = None;
    for (i, b) in sermon.blocks.iter().enumerate() {
        let want = match b.kind.as_str() {
            "bullet" => Some("ul"),
            "number" => Some("ol"),
            _ => None,
        };
        if list_open != want {
            if let Some(tag) = list_open {
                body.push_str(&format!("</{tag}>\n"));
            }
            if let Some(tag) = want {
                body.push_str(&format!("<{tag}>\n"));
            }
            list_open = want;
        }
        let size = if b.size > 0 { b.size } else { kind_size(&b.kind, sermon.size) };
        let font = if b.font.is_empty() { &sermon.font } else { &b.font };
        let style = format!("font-family:{};font-size:{}px;text-align:{}", css_font(font), size, b.align);
        let inline = to_html_inline(&b.text);
        let img = |name: &str| -> String {
            store
                .image_path(name)
                .and_then(|p| fs::read(p).ok())
                .map(|bytes| format!("<img src=\"data:{};base64,{}\" alt=\"\">", mime_for(name), b64_encode(&bytes)))
                .unwrap_or_default()
        };
        match b.kind.as_str() {
            "h1" | "h2" | "h3" => body.push_str(&format!("<{k} style=\"{style}\">{inline}</{k}>\n", k = b.kind)),
            "bullet" | "number" => body.push_str(&format!("<li style=\"{style}\">{inline}</li>\n")),
            "quote" => body.push_str(&format!(
                "<blockquote style=\"{style}\"><p>{inline}</p><footer>{}</footer></blockquote>\n",
                html_escape(&b.extra)
            )),
            "image" => body.push_str(&format!(
                "<figure style=\"text-align:{}\">{}<figcaption>{inline}</figcaption></figure>\n",
                b.align,
                img(&b.image)
            )),
            "slide" => body.push_str(&format!(
                "<section class=\"slide\"><div class=\"slide-no\">SLIDE {}</div><h2 style=\"{style}\">{inline}</h2>{}</section>\n",
                markers[i],
                img(&b.image)
            )),
            _ => {
                if b.text.trim().is_empty() {
                    body.push_str("<p>&nbsp;</p>\n");
                } else {
                    body.push_str(&format!("<p style=\"{style}\">{inline}</p>\n"));
                }
            }
        }
    }
    if let Some(tag) = list_open {
        body.push_str(&format!("</{tag}>\n"));
    }
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}</title>\n<style>\nbody{{max-width:820px;margin:40px auto;padding:0 20px;color:#111;line-height:1.5;font-family:{base}}}\nblockquote{{border-left:4px solid #0a7c8c;margin:16px 0;padding:6px 16px;background:#f2f9fa}}\nblockquote footer{{font-size:0.85em;color:#355;margin-top:4px}}\nimg{{max-width:100%}}\nfigure{{margin:16px 0}}\n.slide{{border:2px solid #0a7c8c;border-radius:6px;padding:12px 18px;margin:22px 0;page-break-inside:avoid}}\n.slide-no{{font:12px monospace;letter-spacing:2px;color:#0a7c8c}}\n</style></head><body>\n{body}</body></html>\n",
        title = html_escape(&sermon.title),
        base = css_font(&sermon.font),
    )
}

/// Markdown export. Underline has no Markdown form, so it is written as `<u>`.
/// Images are referenced by absolute path to the copy in the state folder.
pub fn export_markdown(sermon: &Sermon, store: &DeskStore) -> String {
    let mut out = String::new();
    let markers = sermon.markers();
    let md = |t: &str| -> String {
        spans(t)
            .into_iter()
            .map(|(st, s)| {
                let mut s = s.replace('*', "\\*").replace('_', "\\_");
                if st.italic {
                    s = format!("*{s}*");
                }
                if st.bold {
                    s = format!("**{s}**");
                }
                if st.underline {
                    s = format!("<u>{s}</u>");
                }
                s
            })
            .collect()
    };
    let mut prev_list = false;
    for (i, b) in sermon.blocks.iter().enumerate() {
        let is_list = matches!(b.kind.as_str(), "bullet" | "number");
        if prev_list && !is_list {
            out.push('\n');
        }
        prev_list = is_list;
        let text = md(&b.text);
        let path = |name: &str| store.image_path(name).map(|p| p.display().to_string()).unwrap_or_default();
        match b.kind.as_str() {
            "h1" => out.push_str(&format!("# {text}\n\n")),
            "h2" => out.push_str(&format!("## {text}\n\n")),
            "h3" => out.push_str(&format!("### {text}\n\n")),
            "bullet" => out.push_str(&format!("- {}\n", text.replace('\n', "\n  "))),
            "number" => out.push_str(&format!("{}. {}\n", markers[i], text.replace('\n', "\n   "))),
            "quote" => {
                for line in text.lines() {
                    out.push_str(&format!("> {line}\n"));
                }
                if !b.extra.is_empty() {
                    out.push_str(&format!(">\n> — {}\n", b.extra));
                }
                out.push('\n');
            }
            "image" => out.push_str(&format!("![{}]({})\n\n", plain(&b.text), path(&b.image))),
            "slide" => {
                out.push_str(&format!("---\n\n**SLIDE {}: {}**\n\n", markers[i], text));
                if !b.image.is_empty() {
                    out.push_str(&format!("![slide {}]({})\n\n", markers[i], path(&b.image)));
                }
            }
            _ => out.push_str(&format!("{}\n\n", text.replace('\n', "  \n"))),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("twm-studydesk-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 200, 220, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn sermon_roundtrip_list_delete_and_permissions() {
        let dir = tmp("store");
        let store = DeskStore::new(&dir);
        assert!(store.list().is_empty());
        let mut s = Sermon::new(store.fresh_id("s"), "Living Water");
        s.blocks.push(Block::new(3, "quote", "For God so loved"));
        s.blocks[2].extra = "John 3:16 (KJV)".into();
        store.save(&s).unwrap();
        let loaded = store.load(&s.id).unwrap();
        assert_eq!(loaded, s);
        let src = dir.join("pic.png");
        fs::write(&src, tiny_png()).unwrap();
        let name = store.import_image(&s.id, &src).unwrap();
        assert!(store.image_path(&name).is_some());
        assert!(store.image_path("../settings.json").is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(store.sermons_dir().join(format!("{}.json", s.id))).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            let mode = fs::metadata(store.images_dir().join(&name)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
            let mode = fs::metadata(&store.dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Living Water");
        // No temp files left behind by the atomic write.
        assert!(fs::read_dir(store.sermons_dir()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        store.delete(&s.id).unwrap();
        assert!(store.list().is_empty());
        assert!(store.image_path(&name).is_none());
        assert!(store.load("../../etc").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_images() {
        let dir = tmp("img");
        let store = DeskStore::new(&dir);
        fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake.png");
        fs::write(&fake, b"not a picture").unwrap();
        assert!(store.import_image("s1", &fake).is_err());
        let txt = dir.join("a.txt");
        fs::write(&txt, b"x").unwrap();
        assert!(store.import_image("s1", &txt).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn notes_roundtrip() {
        let dir = tmp("notes");
        let store = DeskStore::new(&dir);
        assert!(store.load_notes().notes.is_empty());
        let book = NoteBook { notes: vec![Note { id: "n1".into(), title: "Ideas".into(), body: "line 1\nline 2".into(), updated: 5 }] };
        store.save_notes(&book).unwrap();
        assert_eq!(store.load_notes().notes, book.notes);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_fixes_bad_data() {
        let mut s = Sermon::new("x".into(), "");
        s.font = "Comic".into();
        s.size = 400;
        s.blocks[1].id = s.blocks[0].id;
        s.blocks[1].kind = "weird".into();
        s.blocks[1].image = "../../x.png".into();
        s.sanitize();
        assert_eq!(s.font, "Rajdhani");
        assert_eq!(s.size, SIZE_MAX);
        assert_ne!(s.blocks[0].id, s.blocks[1].id);
        assert_eq!(s.blocks[1].kind, "p");
        assert!(s.blocks[1].image.is_empty());
        assert_eq!(s.title, "Untitled sermon");
    }

    #[test]
    fn markers_count_lists_and_slides() {
        let mut s = Sermon::new("x".into(), "t");
        s.blocks = ["number", "number", "p", "number", "slide", "slide"].iter().enumerate().map(|(i, k)| Block::new(i as u64 + 1, k, "")).collect();
        assert_eq!(s.markers(), vec![1, 2, 0, 1, 1, 2]);
    }

    #[test]
    fn inline_spans_and_markdown() {
        let sp = spans("a **b** *c* __d__ e*");
        assert_eq!(sp[1], (Style { bold: true, ..Default::default() }, "b".into()));
        assert_eq!(sp[3], (Style { italic: true, ..Default::default() }, "c".into()));
        assert_eq!(sp[5], (Style { underline: true, ..Default::default() }, "d".into()));
        assert!(sp.last().unwrap().1.ends_with("e*"));
        assert_eq!(plain("**Grace** and *peace*"), "Grace and peace");
        let md = to_styled_markdown("1. # <b>**x**", false);
        assert!(md.starts_with("1\\. \\# \\<b\\>**x**"), "{md}");
        assert_eq!(to_styled_markdown("__u__", false), "<u>u</u>");
        for sample in ["a **b ** c", "1. # <b> & *x* __y__ \\ end\nline two", "***", "__"] {
            let md = to_styled_markdown(sample, true);
            assert!(slint::StyledText::from_markdown(&md).is_ok(), "{md}");
        }
        assert_eq!(to_html_inline("<**x**>"), "&lt;<strong>x</strong>&gt;");
    }

    #[test]
    fn toggle_marker_wraps_and_unwraps() {
        let (t, a, b) = toggle_marker("hello world", 6, 11, "**");
        assert_eq!(t, "hello **world**");
        let (t2, _, _) = toggle_marker(&t, a, b, "**");
        assert_eq!(t2, "hello world");
        let (t3, _, _) = toggle_marker("hello **world**", 8, 13, "**");
        assert_eq!(t3, "hello world");
        assert_eq!(toggle_marker("word", 0, 0, "*").0, "*word*");
        assert_eq!(toggle_marker("a  b", 1, 3, "__").0, "a  b");
        assert_eq!(toggle_marker("say hi ", 4, 7, "__").0, "say __hi__ ");
    }

    #[test]
    fn passage_parsing() {
        let p = parse_passage("John 3:16-18").unwrap();
        assert_eq!(p, vec![Span { book_id: "john", chapter: 3, verse: 16, end_chapter: 3, end_verse: 18 }]);
        let p = parse_passage("1 John 4:8").unwrap();
        assert_eq!(p[0].book_id, "1john");
        assert_eq!((p[0].chapter, p[0].verse, p[0].end_verse), (4, 8, 8));
        let p = parse_passage("Ps 23").unwrap();
        assert_eq!((p[0].chapter, p[0].verse, p[0].end_chapter, p[0].end_verse), (23, 0, 23, 0));
        let p = parse_passage("Rom 5:8; 6:23").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].book_id, p[1].chapter, p[1].verse), ("rom", 6, 23));
        let p = parse_passage("John 3:16,18").unwrap();
        assert_eq!((p[1].chapter, p[1].verse), (3, 18));
        let p = parse_passage("Gen 1:1-2:3").unwrap();
        assert_eq!((p[0].end_chapter, p[0].end_verse), (2, 3));
        let p = parse_passage("Jude 3").unwrap();
        assert_eq!((p[0].chapter, p[0].verse), (1, 3));
        let p = parse_passage("Song of Solomon 2:4").unwrap();
        assert_eq!(p[0].book_id, bible::parse_reference("song of solomon").unwrap().book_id);
        let p = parse_passage("john 3:16–17").unwrap();
        assert_eq!(p[0].end_verse, 17);
        assert!(parse_passage("Hezekiah 1:1").is_err());
        assert!(parse_passage("John 3:18-16").is_err());
        assert!(parse_passage("").is_err());
        assert!(parse_passage("3:16").is_err());
    }

    fn mini_bible() -> Vec<Book> {
        vec![
            Book { id: "john".into(), abbrev: "Jn".into(), name: "John".into(), chapters: vec![vec!["In the beginning was the Word".into()], vec!["a".into(), "b".into()], (1..=36).map(|v| format!("verse {v} of chapter three")).collect()] },
            Book { id: "rom".into(), abbrev: "Ro".into(), name: "Romans".into(), chapters: vec![vec!["Paul, a servant".into()]] },
        ]
    }

    #[test]
    fn passage_resolve() {
        let books = mini_bible();
        let e = resolve(&books, &parse_passage("John 3:16-18").unwrap()).unwrap();
        assert_eq!(e.iter().map(|x| x.verse).collect::<Vec<_>>(), vec![16, 17, 18]);
        let e = resolve(&books, &parse_passage("John 2-3:2").unwrap()).unwrap();
        assert_eq!(e.len(), 4);
        let e = resolve(&books, &parse_passage("John 3:35-99").unwrap()).unwrap();
        assert_eq!(e.len(), 2);
        assert!(resolve(&books, &parse_passage("John 9").unwrap()).is_err());
        assert!(resolve(&books, &parse_passage("John 1:5").unwrap()).is_err());
        assert!(resolve(&books, &parse_passage("Gen 1").unwrap()).is_err());
        let (body, reference) = quote_for(&e, "KJV");
        assert!(body.starts_with("35 verse 35"));
        assert_eq!(reference, "John 3:35-36 (KJV)");
    }

    #[test]
    fn search_finds_words_and_phrases() {
        let books = mini_bible();
        let s = search(&books, "chapter   THREE", 10);
        assert_eq!(s.total, 36);
        assert_eq!(s.hits.len(), 10);
        assert!(s.truncated);
        let s = search(&books, "servant", 10);
        assert_eq!(s.hits.len(), 1);
        assert_eq!(s.hits[0].book_id, "rom");
        assert!(search(&books, "a", 10).too_short);
    }

    #[test]
    fn exports() {
        let dir = tmp("export");
        let store = DeskStore::new(&dir);
        let mut s = Sermon::new("s1".into(), "Title <1>");
        s.blocks.push(Block::new(3, "bullet", "one **bold**"));
        s.blocks.push(Block::new(4, "number", "first"));
        s.blocks.push(Block::new(5, "slide", "Opening"));
        let mut q = Block::new(6, "quote", "For God so loved");
        q.extra = "John 3:16 (KJV)".into();
        s.blocks.push(q);
        let html = export_html(&s, &store);
        assert!(html.contains("<title>Title &lt;1&gt;</title>"));
        assert!(html.contains("<ul>\n<li"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("</ul>\n<ol>"));
        assert!(html.contains("SLIDE 1"));
        assert!(html.contains("<footer>John 3:16 (KJV)</footer>"));
        let md = export_markdown(&s, &store);
        assert!(md.starts_with("# Title <1>\n\n"));
        assert!(md.contains("- one **bold**\n1. first\n"));
        assert!(md.contains("**SLIDE 1: Opening**"));
        assert!(md.contains("> — John 3:16 (KJV)"));
        let _ = fs::remove_dir_all(&dir);
    }
}
