//! Wires the Study Desk (sermon prep and notes) to the window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use twm_bible_native::bible::{self as texts, PassageEntry};
use twm_bible_native::studydesk::{self as sd, Block, DeskStore, Note, NoteBook, Sermon};

use crate::{AppWindow, Core, DeskBlock, DeskChip, DeskDoc, DeskVerse, Session, Studio};

const UNDO_LIMIT: usize = 200;

enum Confirm {
    Sermon(String),
    Note(String),
}

pub struct Desk {
    store: DeskStore,
    doc: Option<Sermon>,
    doc_dirty: bool,
    undo: Vec<Vec<Block>>,
    redo: Vec<Vec<Block>>,
    typing: Option<(u64, Instant)>,
    current: usize,
    sel: (usize, usize),
    images: HashMap<String, (slint::Image, f32)>,
    version: String,
    results: Vec<PassageEntry>,
    picked: Vec<PassageEntry>,
    notes: NoteBook,
    note_id: Option<String>,
    notes_dirty: bool,
    confirm: Option<Confirm>,
    save_timer: slint::Timer,
    blocks: Rc<VecModel<DeskBlock>>,
    sermons: Rc<VecModel<DeskDoc>>,
    note_rows: Rc<VecModel<DeskDoc>>,
    result_rows: Rc<VecModel<DeskVerse>>,
    picked_rows: Rc<VecModel<DeskVerse>>,
    chips: Rc<VecModel<DeskChip>>,
}

type CoreRc = Rc<RefCell<Core>>;
pub type DeskRc = Rc<RefCell<Desk>>;

fn when(secs: u64) -> String {
    Local
        .timestamp_opt(secs as i64, 0)
        .single()
        .map(|t| t.format("%b %-d, %Y · %-I:%M %p").to_string())
        .unwrap_or_default()
}

/// Generic font choices resolve to a real installed family through fontconfig, because the
/// renderer does not map CSS-style generic names. Falls back to common names elsewhere.
fn generic(kind: &str) -> String {
    static CACHE: std::sync::OnceLock<(String, String)> = std::sync::OnceLock::new();
    let (serif, sans) = CACHE.get_or_init(|| {
        let ask = |pattern: &str, fallback: &str| -> String {
            let mut cmd = std::process::Command::new("fc-match");
            cmd.args([pattern, "-f", "%{family}"]);
            twm_bible_native::quiet(&mut cmd)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| s.split(',').next().map(|x| x.trim().to_string()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| fallback.to_string())
        };
        (ask("serif", "Georgia"), ask("sans-serif", "Arial"))
    });
    if kind == "serif" { serif.clone() } else { sans.clone() }
}

pub fn family(name: &str) -> String {
    match name {
        "Serif" => generic("serif"),
        "Sans" => generic("sans"),
        other => other.into(),
    }
}

fn placeholder(kind: &str) -> &'static str {
    match kind {
        "h1" | "h2" | "h3" => "Heading",
        "bullet" | "number" => "List item",
        "quote" => "Quoted text",
        "image" => "Caption (optional)",
        "slide" => "Slide title",
        _ => "Write here…",
    }
}

impl Desk {
    fn image(&mut self, name: &str) -> Option<(slint::Image, f32)> {
        if name.is_empty() {
            return None;
        }
        if let Some(hit) = self.images.get(name) {
            return Some(hit.clone());
        }
        let path = self.store.image_path(name)?;
        let img = slint::Image::load_from_path(&path).ok()?;
        let size = img.size();
        let ratio = if size.width > 0 { size.height as f32 / size.width as f32 } else { 0.6 };
        self.images.insert(name.to_string(), (img.clone(), ratio));
        Some((img, ratio))
    }

    fn row(&mut self, i: usize, markers: &[i32]) -> DeskBlock {
        let doc = self.doc.as_ref().unwrap();
        let b = doc.blocks[i].clone();
        let font = if b.font.is_empty() { doc.font.clone() } else { b.font.clone() };
        let size = if b.size > 0 { b.size } else { sd::kind_size(&b.kind, doc.size) };
        let bold = matches!(b.kind.as_str(), "h1" | "h2" | "h3" | "slide");
        let md = sd::to_styled_markdown(&b.text, bold);
        let rich = slint::StyledText::from_markdown(&md).unwrap_or_else(|_| slint::StyledText::from_plain_text(&sd::plain(&b.text)));
        let img = self.image(&b.image);
        DeskBlock {
            id: b.id as i32,
            kind: b.kind.clone().into(),
            text: b.text.clone().into(),
            extra: b.extra.clone().into(),
            rich,
            has_image: img.is_some(),
            img_ratio: img.as_ref().map(|x| x.1).unwrap_or(0.6),
            img: img.map(|x| x.0).unwrap_or_default(),
            font: family(&font).into(),
            size,
            bold,
            align: b.align.clone().into(),
            marker: markers.get(i).copied().unwrap_or(0),
            placeholder: placeholder(&b.kind).into(),
        }
    }

    fn rebuild(&mut self) {
        let Some(doc) = self.doc.as_ref() else {
            self.blocks.set_vec(Vec::new());
            return;
        };
        let markers = doc.markers();
        let rows: Vec<DeskBlock> = (0..doc.blocks.len()).map(|i| self.row(i, &markers)).collect();
        self.blocks.set_vec(rows);
    }

    fn refresh_row(&mut self, i: usize) {
        let Some(doc) = self.doc.as_ref() else { return };
        if i >= doc.blocks.len() || i >= self.blocks.row_count() {
            return;
        }
        let markers = doc.markers();
        let row = self.row(i, &markers);
        self.blocks.set_row_data(i, row);
    }

    fn snapshot(&mut self) {
        if let Some(doc) = &self.doc {
            self.undo.push(doc.blocks.clone());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.typing = None;
    }

    fn touch(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            doc.updated = sd::now_secs();
        }
        self.doc_dirty = true;
    }

    fn flush(&mut self) -> Result<Option<String>, String> {
        let mut saved = None;
        if self.doc_dirty {
            if let Some(doc) = &self.doc {
                self.store.save(doc)?;
                saved = Some(doc.id.clone());
            }
            self.doc_dirty = false;
        }
        if self.notes_dirty {
            self.store.save_notes(&self.notes)?;
            self.notes_dirty = false;
        }
        Ok(saved)
    }

    fn sermon_rows(&self) {
        let current = self.doc.as_ref().map(|d| d.id.clone()).unwrap_or_default();
        let mut list = self.store.list();
        // The open document may have unsaved title changes.
        if let Some(doc) = &self.doc {
            if let Some(row) = list.iter_mut().find(|r| r.id == doc.id) {
                row.title = doc.title.clone();
                row.updated = doc.updated;
            } else {
                list.insert(0, sd::SermonInfo { id: doc.id.clone(), title: doc.title.clone(), updated: doc.updated });
            }
        }
        let rows = list
            .into_iter()
            .map(|r| DeskDoc { current: r.id == current, id: r.id.into(), title: r.title.into(), updated: when(r.updated).into() })
            .collect::<Vec<_>>();
        self.sermons.set_vec(rows);
    }

    fn note_rows(&self) {
        let current = self.note_id.clone().unwrap_or_default();
        let rows = self
            .notes
            .notes
            .iter()
            .map(|n| DeskDoc {
                current: n.id == current,
                id: n.id.clone().into(),
                title: if n.title.trim().is_empty() { "Untitled note".into() } else { n.title.clone().into() },
                updated: when(n.updated).into(),
            })
            .collect::<Vec<_>>();
        self.note_rows.set_vec(rows);
    }

    fn verse_rows(&self) {
        let picked: Vec<(String, usize, usize)> = self.picked.iter().map(|e| (e.book_id.clone(), e.chapter, e.verse)).collect();
        let key = |e: &PassageEntry| (e.book_id.clone(), e.chapter, e.verse);
        let label = |e: &PassageEntry| format!("{} {}:{}", e.name, e.chapter, e.verse);
        self.result_rows.set_vec(
            self.results
                .iter()
                .map(|e| DeskVerse { key: texts::ref_key(&e.book_id, e.chapter, e.verse).into(), reference: label(e).into(), text: e.text.trim().into(), selected: picked.contains(&key(e)) })
                .collect::<Vec<_>>(),
        );
        self.picked_rows.set_vec(
            self.picked
                .iter()
                .map(|e| DeskVerse { key: texts::ref_key(&e.book_id, e.chapter, e.verse).into(), reference: label(e).into(), text: e.text.trim().into(), selected: true })
                .collect::<Vec<_>>(),
        );
    }
}

fn sync_current(ui: &AppWindow, desk: &Desk) {
    let st = ui.global::<Studio>();
    let Some(doc) = &desk.doc else { return };
    let i = desk.current.min(doc.blocks.len().saturating_sub(1));
    st.set_current(i as i32);
    if let Some(b) = doc.blocks.get(i) {
        st.set_cur_kind(b.kind.clone().into());
        st.set_cur_font(if b.font.is_empty() { doc.font.clone().into() } else { b.font.clone().into() });
        st.set_cur_size(if b.size > 0 { b.size } else { sd::kind_size(&b.kind, doc.size) });
        st.set_cur_align(b.align.clone().into());
    }
    st.set_can_undo(!desk.undo.is_empty());
    st.set_can_redo(!desk.redo.is_empty());
}

/// Asks the block with this index to take the caret, after the model has settled.
fn focus_block(ui: &AppWindow, desk: &Desk, index: usize, a: Option<usize>, b: Option<usize>) {
    let Some(doc) = &desk.doc else { return };
    let Some(block) = doc.blocks.get(index) else { return };
    let len = block.text.len();
    let a = a.unwrap_or(len).min(len);
    let b = b.unwrap_or(a).min(len);
    let st = ui.global::<Studio>();
    st.set_focus_id(block.id as i32);
    st.set_focus_a(a as i32);
    st.set_focus_b(b as i32);
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(15), move || {
        if let Some(ui) = weak.upgrade() {
            let st = ui.global::<Studio>();
            st.set_focus_tick(st.get_focus_tick() + 1);
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(120), move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<Studio>().set_focus_id(-1);
                }
            });
        }
    });
}

/// Like `focus_block`, but for a block that already exists in the model: focus moves in this event,
/// so held arrow keys walk through blocks without waiting for a timer.
fn focus_block_now(ui: &AppWindow, desk: &Desk, index: usize, a: Option<usize>) {
    let Some(doc) = &desk.doc else { return };
    let Some(block) = doc.blocks.get(index) else { return };
    let len = block.text.len();
    let a = a.unwrap_or(len).min(len);
    let st = ui.global::<Studio>();
    st.set_focus_id(block.id as i32);
    st.set_focus_a(a as i32);
    st.set_focus_b(a as i32);
    st.set_focus_tick(st.get_focus_tick() + 1);
    let weak = ui.as_weak();
    let id = block.id as i32;
    slint::Timer::single_shot(Duration::from_millis(120), move || {
        if let Some(ui) = weak.upgrade() {
            let st = ui.global::<Studio>();
            if st.get_focus_id() == id {
                st.set_focus_id(-1);
            }
        }
    });
}

fn save_label(ui: &AppWindow, desk: &mut Desk) {
    let st = ui.global::<Studio>();
    let had_doc = desk.doc_dirty;
    let had_notes = desk.notes_dirty;
    match desk.flush() {
        Ok(_) => {
            let now = Local::now().format("%-I:%M:%S %p").to_string();
            if had_doc {
                st.set_save_state(format!("SAVED {now}").into());
            }
            if had_notes {
                st.set_note_state(format!("SAVED {now}").into());
            }
        }
        Err(e) => {
            st.set_save_state(format!("SAVE FAILED: {e}").into());
            st.set_note_state(format!("SAVE FAILED: {e}").into());
        }
    }
}

fn schedule_save(ui: &AppWindow, desk_rc: &DeskRc) {
    let st = ui.global::<Studio>();
    {
        let d = desk_rc.borrow();
        if d.doc_dirty {
            st.set_save_state("EDITING…".into());
        }
        if d.notes_dirty {
            st.set_note_state("EDITING…".into());
        }
    }
    let weak = ui.as_weak();
    let rc = desk_rc.clone();
    desk_rc.borrow().save_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(700), move || {
        if let Some(ui) = weak.upgrade() {
            if let Ok(mut d) = rc.try_borrow_mut() {
                save_label(&ui, &mut d);
                d.sermon_rows();
            }
        }
    });
}

/// Saves whatever is pending. Called when the window closes.
pub fn flush(desk: &DeskRc) {
    if let Ok(mut d) = desk.try_borrow_mut() {
        if let Err(e) = d.flush() {
            eprintln!("study desk: save failed: {e}");
        }
    }
}

fn open_doc(ui: &AppWindow, desk: &mut Desk, doc: Sermon) {
    let _ = desk.flush();
    let st = ui.global::<Studio>();
    st.set_doc_id(doc.id.clone().into());
    st.set_doc_title(doc.title.clone().into());
    st.set_save_state(format!("OPENED · LAST CHANGE {}", when(doc.updated)).into());
    desk.undo.clear();
    desk.redo.clear();
    desk.typing = None;
    // Start where the writing is: the last block.
    desk.current = doc.blocks.len().saturating_sub(1);
    desk.sel = (0, 0);
    desk.doc = Some(doc);
    desk.rebuild();
    desk.sermon_rows();
    sync_current(ui, desk);
    let cur = desk.current;
    focus_block(ui, desk, cur, None, None);
}

fn new_sermon(ui: &AppWindow, desk: &mut Desk) {
    let id = desk.store.fresh_id("s");
    let count = desk.store.list().len();
    let title = if count == 0 { "My first sermon".to_string() } else { format!("Untitled sermon {}", count + 1) };
    let doc = Sermon::new(id, &title);
    if let Err(e) = desk.store.save(&doc) {
        ui.global::<Studio>().set_save_state(format!("SAVE FAILED: {e}").into());
    }
    open_doc(ui, desk, doc);
}

fn versions(ui: &AppWindow, core: &Core, desk: &Desk) {
    let mut chips = Vec::new();
    for id in &core.settings.enabled {
        if let Some(ed) = core.editions.get(id) {
            chips.push(DeskChip { id: id.clone().into(), label: ed.label.clone().into(), active: *id == desk.version });
        }
    }
    desk.chips.set_vec(chips);
    let _ = ui;
}

fn short_label(core: &Core, id: &str) -> String {
    match id {
        "kjv" => "KJV".into(),
        "asv" => "ASV".into(),
        "web" => "WEB".into(),
        "ylt" => "YLT".into(),
        "darby" => "Darby".into(),
        "rv" => "RV".into(),
        "geneva" => "Geneva".into(),
        "dra" => "DRA".into(),
        other => core.editions.get(other).map(|e| e.label.clone()).unwrap_or_else(|| other.to_uppercase()),
    }
}

fn picked_summary(ui: &AppWindow, core: &Core, desk: &Desk) {
    let st = ui.global::<Studio>();
    if desk.picked.is_empty() {
        st.set_picked_ref("".into());
    } else {
        let (_, reference) = sd::quote_for(&desk.picked, &short_label(core, &desk.version));
        st.set_picked_ref(reference.into());
    }
}

pub fn setup(ui: &AppWindow, core: &CoreRc) -> DeskRc {
    let state_dir = core.borrow().state_dir.clone();
    let desk = Desk {
        store: DeskStore::new(&state_dir),
        doc: None,
        doc_dirty: false,
        undo: Vec::new(),
        redo: Vec::new(),
        typing: None,
        current: 0,
        sel: (0, 0),
        images: HashMap::new(),
        version: "kjv".into(),
        results: Vec::new(),
        picked: Vec::new(),
        notes: NoteBook::default(),
        note_id: None,
        notes_dirty: false,
        confirm: None,
        save_timer: slint::Timer::default(),
        blocks: Rc::new(VecModel::from(Vec::new())),
        sermons: Rc::new(VecModel::from(Vec::new())),
        note_rows: Rc::new(VecModel::from(Vec::new())),
        result_rows: Rc::new(VecModel::from(Vec::new())),
        picked_rows: Rc::new(VecModel::from(Vec::new())),
        chips: Rc::new(VecModel::from(Vec::new())),
    };
    let st = ui.global::<Studio>();
    st.set_serif_family(family("Serif").into());
    st.set_sans_family(family("Sans").into());
    st.set_blocks(ModelRc::from(desk.blocks.clone()));
    st.set_sermons(ModelRc::from(desk.sermons.clone()));
    st.set_notes(ModelRc::from(desk.note_rows.clone()));
    st.set_results(ModelRc::from(desk.result_rows.clone()));
    st.set_picked(ModelRc::from(desk.picked_rows.clone()));
    st.set_versions(ModelRc::from(desk.chips.clone()));
    let desk: DeskRc = Rc::new(RefCell::new(desk));
    let weak = ui.as_weak();

    macro_rules! hook {
        ($name:ident, |$ui:ident, $d:ident, $c:ident $(, $arg:ident)*| $body:block) => {{
            let core = core.clone();
            let desk = desk.clone();
            let weak = weak.clone();
            ui.global::<Studio>().$name(move |$($arg),*| {
                let Some($ui) = weak.upgrade() else { return Default::default() };
                let $c = &core;
                let Ok(mut guard) = desk.try_borrow_mut() else { return Default::default() };
                let $d: &mut Desk = &mut guard;
                let result = (|| $body)();
                drop(guard);
                if desk.borrow().doc_dirty || desk.borrow().notes_dirty {
                    schedule_save(&$ui, &desk);
                }
                result
            });
        }};
    }

    hook!(on_open_sermons, |ui, d, c| {
        ui.global::<Session>().set_view("sermon".into());
        versions(&ui, &c.borrow(), d);
        if d.doc.is_none() {
            let first = d.store.list().into_iter().next().and_then(|info| d.store.load(&info.id));
            match first {
                Some(doc) => open_doc(&ui, d, doc),
                None => new_sermon(&ui, d),
            }
        } else {
            d.sermon_rows();
            sync_current(&ui, d);
            let cur = d.current;
            focus_block(&ui, d, cur, None, None);
        }
    });

    hook!(on_new_sermon, |ui, d, c| {
        let _ = c;
        new_sermon(&ui, d);
    });

    hook!(on_open_sermon, |ui, d, c, id| {
        let _ = c;
        if d.doc.as_ref().map(|x| x.id.as_str()) == Some(id.as_str()) {
            return;
        }
        let _ = d.flush();
        if let Some(doc) = d.store.load(&id) {
            open_doc(&ui, d, doc);
        }
    });

    hook!(on_rename_sermon, |ui, d, c, title| {
        let _ = c;
        let Some(doc) = d.doc.as_mut() else { return };
        let title = title.trim().to_string();
        if title.is_empty() || doc.title == title {
            return;
        }
        doc.title = title;
        d.touch();
        d.sermon_rows();
    });

    hook!(on_ask_delete_sermon, |ui, d, c| {
        let _ = c;
        let Some(doc) = &d.doc else { return };
        let st = ui.global::<Studio>();
        st.set_confirm_text(format!("Delete the sermon \"{}\" and its images? This cannot be undone.", doc.title).into());
        d.confirm = Some(Confirm::Sermon(doc.id.clone()));
        st.set_confirm_open(true);
    });

    hook!(on_ask_delete_note, |ui, d, c| {
        let _ = c;
        let Some(id) = d.note_id.clone() else { return };
        let title = d.notes.notes.iter().find(|n| n.id == id).map(|n| n.title.clone()).unwrap_or_default();
        let st = ui.global::<Studio>();
        st.set_confirm_text(format!("Delete the note \"{}\"? This cannot be undone.", if title.is_empty() { "Untitled note" } else { &title }).into());
        d.confirm = Some(Confirm::Note(id));
        st.set_confirm_open(true);
    });

    hook!(on_confirm_yes, |ui, d, c| {
        let _ = c;
        let st = ui.global::<Studio>();
        st.set_confirm_open(false);
        match d.confirm.take() {
            Some(Confirm::Sermon(id)) => {
                d.doc_dirty = false;
                d.doc = None;
                if let Err(e) = d.store.delete(&id) {
                    st.set_save_state(format!("DELETE FAILED: {e}").into());
                    return;
                }
                d.images.retain(|k, _| !k.starts_with(&format!("{id}_")));
                let next = d.store.list().into_iter().next().and_then(|info| d.store.load(&info.id));
                match next {
                    Some(doc) => open_doc(&ui, d, doc),
                    None => new_sermon(&ui, d),
                }
                st.set_save_state("SERMON DELETED".into());
            }
            Some(Confirm::Note(id)) => {
                d.notes.notes.retain(|n| n.id != id);
                d.notes_dirty = true;
                d.note_id = d.notes.notes.first().map(|n| n.id.clone());
                load_note(&ui, d);
            }
            None => {}
        }
    });

    hook!(on_block_edited, |ui, d, c, index, text| {
        let _ = (c, &ui);
        let i = index as usize;
        let id = match d.doc.as_ref().and_then(|doc| doc.blocks.get(i)) {
            Some(b) if b.text != text.as_str() => b.id,
            _ => return,
        };
        // Typing in one block within a short pause is one undo step.
        let fresh = !matches!(d.typing, Some((bid, at)) if bid == id && at.elapsed() < Duration::from_millis(1500));
        if fresh {
            d.snapshot();
        }
        d.typing = Some((id, Instant::now()));
        d.doc.as_mut().unwrap().blocks[i].text = text.to_string();
        d.current = i;
        d.touch();
        let st = ui.global::<Studio>();
        st.set_can_undo(!d.undo.is_empty());
        st.set_can_redo(false);
    });

    // A key typed while the edited block was scrolled out of view: put it back and apply the key.
    hook!(on_resume_typing, |ui, d, c, key| {
        let _ = c;
        let Some(doc) = d.doc.as_ref() else { return };
        if doc.blocks.is_empty() {
            return;
        }
        let i = d.current.min(doc.blocks.len() - 1);
        let old = doc.blocks[i].text.clone();
        let (a, b) = d.sel;
        let (lo, hi) = (a.min(b).min(old.len()), a.max(b).min(old.len()));
        if !old.is_char_boundary(lo) || !old.is_char_boundary(hi) {
            focus_block(&ui, d, i, Some(a), Some(b));
            return;
        }
        let (new, caret) = resume_edit(&old, lo, hi, key.as_str());
        if new != old {
            d.snapshot();
            d.doc.as_mut().unwrap().blocks[i].text = new;
            d.typing = None;
            d.touch();
            d.refresh_row(i);
            let st = ui.global::<Studio>();
            st.set_can_undo(!d.undo.is_empty());
            st.set_can_redo(false);
        }
        d.sel = (caret, caret);
        focus_block(&ui, d, i, Some(caret), None);
    });

    ui.global::<Studio>().on_byte_len(|text| text.len() as i32);

    hook!(on_block_extra, |ui, d, c, index, text| {
        let _ = (c, &ui);
        let i = index as usize;
        let changed = d.doc.as_ref().and_then(|doc| doc.blocks.get(i)).map(|b| b.extra != text.as_str()).unwrap_or(false);
        if changed {
            d.snapshot();
            d.doc.as_mut().unwrap().blocks[i].extra = text.to_string();
            d.touch();
        }
    });

    hook!(on_block_focus, |ui, d, c, index| {
        let _ = c;
        d.current = index.max(0) as usize;
        sync_current(&ui, d);
    });

    hook!(on_block_blur, |ui, d, c, index| {
        let _ = (c, &ui);
        d.typing = None;
        d.refresh_row(index.max(0) as usize);
    });

    hook!(on_block_sel, |ui, d, c, index, a, b| {
        let _ = (c, &ui);
        d.current = index.max(0) as usize;
        d.sel = (a.max(0) as usize, b.max(0) as usize);
    });

    hook!(on_split_block, |ui, d, c, index, cursor| {
        let _ = c;
        let i = index as usize;
        let Some(doc) = d.doc.as_ref() else { return };
        let Some(b) = doc.blocks.get(i).cloned() else { return };
        let listy = matches!(b.kind.as_str(), "bullet" | "number");
        if listy && b.text.trim().is_empty() {
            // Enter on an empty list item ends the list.
            d.snapshot();
            d.doc.as_mut().unwrap().blocks[i].kind = "p".into();
            d.touch();
            d.rebuild();
            sync_current(&ui, d);
            focus_block(&ui, d, i, Some(0), None);
            return;
        }
        let mut cut = (cursor.max(0) as usize).min(b.text.len());
        while !b.text.is_char_boundary(cut) {
            cut -= 1;
        }
        d.snapshot();
        let doc = d.doc.as_mut().unwrap();
        let id = doc.next_block_id();
        let kind = if listy { b.kind.as_str() } else { "p" };
        let mut nb = Block::new(id, kind, &b.text[cut..]);
        if listy || b.kind == "p" {
            nb.font = b.font.clone();
            nb.size = b.size;
            nb.align = b.align.clone();
        }
        doc.blocks[i].text = b.text[..cut].to_string();
        doc.blocks.insert(i + 1, nb);
        d.current = i + 1;
        d.touch();
        d.rebuild();
        sync_current(&ui, d);
        focus_block(&ui, d, i + 1, Some(0), None);
    });

    hook!(on_merge_up, |ui, d, c, index| {
        let _ = c;
        let i = index as usize;
        let Some(doc) = d.doc.as_ref() else { return false };
        let Some(b) = doc.blocks.get(i).cloned() else { return false };
        if b.kind != "p" && b.kind != "image" && b.kind != "slide" {
            // First backspace turns a heading, list item or quote back into a paragraph.
            d.snapshot();
            d.doc.as_mut().unwrap().blocks[i].kind = "p".into();
            d.touch();
            d.rebuild();
            sync_current(&ui, d);
            focus_block(&ui, d, i, Some(0), None);
            return true;
        }
        if i == 0 {
            return false;
        }
        let prev = doc.blocks[i - 1].clone();
        if prev.kind == "image" || b.kind == "image" || b.kind == "slide" {
            if b.text.is_empty() && b.image.is_empty() && b.kind != "image" {
                d.snapshot();
                d.doc.as_mut().unwrap().blocks.remove(i);
                d.touch();
                d.rebuild();
                d.current = i - 1;
                sync_current(&ui, d);
                focus_block(&ui, d, i - 1, None, None);
                return true;
            }
            focus_block(&ui, d, i - 1, None, None);
            return true;
        }
        d.snapshot();
        let doc = d.doc.as_mut().unwrap();
        let at = prev.text.len();
        doc.blocks[i - 1].text.push_str(&b.text);
        doc.blocks.remove(i);
        d.current = i - 1;
        d.touch();
        d.rebuild();
        sync_current(&ui, d);
        focus_block(&ui, d, i - 1, Some(at), None);
        true
    });

    hook!(on_step_focus, |ui, d, c, index, dir, cursor| {
        let _ = c;
        let Some(doc) = d.doc.as_ref() else { return false };
        let len = doc.blocks.len();
        let i = index.max(0) as usize;
        if i >= len {
            // Click under the last block.
            if len > 0 {
                focus_block(&ui, d, len - 1, None, None);
            }
            return true;
        }
        if dir < 0 && cursor == 0 && i > 0 {
            focus_block_now(&ui, d, i - 1, None);
            return true;
        }
        if dir > 0 && cursor as usize >= doc.blocks[i].text.len() && i + 1 < len {
            focus_block_now(&ui, d, i + 1, Some(0));
            return true;
        }
        false
    });

    fn restyle(ui: &AppWindow, d: &mut Desk, f: impl FnOnce(&mut Sermon, usize)) {
        let Some(doc) = d.doc.as_ref() else { return };
        let i = d.current.min(doc.blocks.len().saturating_sub(1));
        d.snapshot();
        f(d.doc.as_mut().unwrap(), i);
        d.touch();
        d.rebuild();
        sync_current(ui, d);
        let (a, b) = d.sel;
        focus_block(ui, d, i, Some(a), Some(b));
    }

    hook!(on_set_kind, |ui, d, c, kind| {
        let _ = c;
        let kind = kind.to_string();
        if !sd::KINDS.contains(&kind.as_str()) {
            return;
        }
        restyle(&ui, d, |doc, i| {
            let b = &mut doc.blocks[i];
            b.kind = kind;
            b.size = 0;
        });
    });

    hook!(on_set_font, |ui, d, c, font| {
        let _ = c;
        let font = font.to_string();
        if !sd::FONTS.contains(&font.as_str()) {
            return;
        }
        restyle(&ui, d, |doc, i| doc.blocks[i].font = font);
    });

    hook!(on_bump_size, |ui, d, c, delta| {
        let _ = c;
        restyle(&ui, d, |doc, i| {
            let base = doc.size;
            let b = &mut doc.blocks[i];
            let now = if b.size > 0 { b.size } else { sd::kind_size(&b.kind, base) };
            b.size = (now + delta).clamp(sd::SIZE_MIN, sd::SIZE_MAX);
        });
    });

    hook!(on_set_align, |ui, d, c, align| {
        let _ = c;
        let align = align.to_string();
        restyle(&ui, d, |doc, i| doc.blocks[i].align = align);
    });

    hook!(on_format, |ui, d, c, marker| {
        let _ = c;
        let Some(doc) = d.doc.as_ref() else { return };
        let i = d.current.min(doc.blocks.len().saturating_sub(1));
        let text = doc.blocks[i].text.clone();
        let (a, b) = d.sel;
        let (new, na, nb) = sd::toggle_marker(&text, a, b, marker.as_str());
        if new == text {
            focus_block(&ui, d, i, Some(a), Some(b));
            return;
        }
        d.snapshot();
        d.doc.as_mut().unwrap().blocks[i].text = new;
        d.touch();
        d.refresh_row(i);
        d.sel = (na, nb);
        sync_current(&ui, d);
        focus_block(&ui, d, i, Some(na), Some(nb));
    });

    fn insert_after(ui: &AppWindow, d: &mut Desk, block: impl FnOnce(u64) -> Block) -> usize {
        let doc = d.doc.as_mut().unwrap();
        let i = d.current.min(doc.blocks.len().saturating_sub(1));
        let id = doc.next_block_id();
        // An empty paragraph is replaced instead of left behind.
        let at = if doc.blocks.get(i).map(|b| b.kind == "p" && b.text.is_empty()).unwrap_or(false) {
            doc.blocks[i] = block(id);
            i
        } else {
            doc.blocks.insert(i + 1, block(id));
            i + 1
        };
        // Keep somewhere to type after the new block.
        if at + 1 == doc.blocks.len() {
            let id = doc.next_block_id();
            doc.blocks.push(Block::new(id, "p", ""));
        }
        d.current = at;
        d.touch();
        d.rebuild();
        sync_current(ui, d);
        at
    }

    fn choose_image(d: &mut Desk) -> Result<Option<String>, String> {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose an image")
            .add_filter("Images", sd::IMAGE_EXTS)
            .pick_file()
        else {
            return Ok(None);
        };
        let id = d.doc.as_ref().map(|x| x.id.clone()).unwrap_or_default();
        d.store.import_image(&id, &path).map(Some)
    }

    hook!(on_add_block, |ui, d, c, kind| {
        let _ = c;
        if d.doc.is_none() {
            return;
        }
        let st = ui.global::<Studio>();
        match kind.as_str() {
            "image" => match choose_image(d) {
                Ok(Some(name)) => {
                    d.snapshot();
                    let at = insert_after(&ui, d, |id| {
                        let mut b = Block::new(id, "image", "");
                        b.image = name;
                        b.align = "center".into();
                        b
                    });
                    focus_block(&ui, d, at, None, None);
                }
                Ok(None) => {}
                Err(e) => st.set_save_state(format!("IMAGE NOT ADDED: {e}").into()),
            },
            "slide" => {
                d.snapshot();
                let at = insert_after(&ui, d, |id| Block::new(id, "slide", ""));
                focus_block(&ui, d, at, None, None);
            }
            _ => {
                d.snapshot();
                let at = insert_after(&ui, d, |id| Block::new(id, "p", ""));
                focus_block(&ui, d, at, None, None);
            }
        }
    });

    hook!(on_pick_image, |ui, d, c, index| {
        let _ = c;
        let i = index.max(0) as usize;
        match choose_image(d) {
            Ok(Some(name)) => {
                d.snapshot();
                if let Some(b) = d.doc.as_mut().and_then(|doc| doc.blocks.get_mut(i)) {
                    b.image = name;
                }
                d.touch();
                d.refresh_row(i);
                sync_current(&ui, d);
            }
            Ok(None) => {}
            Err(e) => ui.global::<Studio>().set_save_state(format!("IMAGE NOT ADDED: {e}").into()),
        }
    });

    hook!(on_move_block, |ui, d, c, dir| {
        let _ = c;
        let Some(doc) = d.doc.as_ref() else { return };
        let i = d.current.min(doc.blocks.len().saturating_sub(1));
        let j = i as i64 + dir as i64;
        if j < 0 || j as usize >= doc.blocks.len() {
            return;
        }
        d.snapshot();
        d.doc.as_mut().unwrap().blocks.swap(i, j as usize);
        d.current = j as usize;
        d.touch();
        d.rebuild();
        sync_current(&ui, d);
        focus_block(&ui, d, j as usize, None, None);
    });

    hook!(on_delete_block, |ui, d, c| {
        let _ = c;
        let Some(doc) = d.doc.as_ref() else { return };
        let i = d.current.min(doc.blocks.len().saturating_sub(1));
        d.snapshot();
        let doc = d.doc.as_mut().unwrap();
        doc.blocks.remove(i);
        if doc.blocks.is_empty() {
            let id = doc.next_block_id();
            doc.blocks.push(Block::new(id, "p", ""));
        }
        let n = doc.blocks.len();
        d.current = i.min(n - 1);
        d.touch();
        d.rebuild();
        sync_current(&ui, d);
        let cur = d.current;
        focus_block(&ui, d, cur, None, None);
    });

    fn history(ui: &AppWindow, d: &mut Desk, back: bool) {
        let Some(doc) = d.doc.as_mut() else { return };
        let (from, to) = if back { (&mut d.undo, &mut d.redo) } else { (&mut d.redo, &mut d.undo) };
        let Some(state) = from.pop() else { return };
        to.push(std::mem::replace(&mut doc.blocks, state));
        // Land the caret in the first block that differs.
        let other = to.last().unwrap();
        let changed = doc
            .blocks
            .iter()
            .enumerate()
            .find(|(k, b)| other.get(*k) != Some(*b))
            .map(|(k, _)| k)
            .unwrap_or(doc.blocks.len().saturating_sub(1))
            .min(doc.blocks.len().saturating_sub(1));
        d.typing = None;
        d.current = changed;
        d.touch();
        d.rebuild();
        sync_current(ui, d);
        focus_block(ui, d, changed, None, None);
    }

    hook!(on_undo, |ui, d, c| {
        let _ = c;
        history(&ui, d, true);
    });

    hook!(on_redo, |ui, d, c| {
        let _ = c;
        history(&ui, d, false);
    });

    hook!(on_export, |ui, d, c, kind| {
        let _ = c;
        let Some(doc) = d.doc.clone() else { return };
        let ext = if kind.as_str() == "md" { "md" } else { "html" };
        let name = format!("{}.{ext}", texts::slug(&doc.title).trim_matches('-'));
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export sermon")
            .set_file_name(&name)
            .add_filter(if ext == "md" { "Markdown" } else { "Web page" }, &[ext])
            .save_file()
        else {
            return;
        };
        let body = if ext == "md" { sd::export_markdown(&doc, &d.store) } else { sd::export_html(&doc, &d.store) };
        let st = ui.global::<Studio>();
        match fs::write(&path, body) {
            Ok(()) => st.set_save_state(format!("EXPORTED {}", path.display()).into()),
            Err(e) => st.set_save_state(format!("EXPORT FAILED: {e}").into()),
        }
    });

    // ---------------------------------------------------------- scripture panel

    hook!(on_set_panel_version, |ui, d, c, id| {
        d.version = id.to_string();
        versions(&ui, &c.borrow(), d);
        let q = ui.global::<Studio>().get_panel_query();
        if !q.trim().is_empty() && !d.results.is_empty() {
            drop(q);
            let q = ui.global::<Studio>().get_panel_query().to_string();
            run_lookup(&ui, c, d, &q);
        }
    });

    fn run_lookup(ui: &AppWindow, core: &CoreRc, d: &mut Desk, query: &str) {
        let st = ui.global::<Studio>();
        let id = d.version.clone();
        if let Err(e) = core.borrow_mut().ensure(&id) {
            st.set_panel_summary(e.into());
            return;
        }
        let core = core.borrow();
        let Some(books) = core.editions.get(&id).and_then(|e| e.books.as_deref()) else {
            st.set_panel_summary("That text is not loaded.".into());
            return;
        };
        let label = short_label(&core, &id);
        if st.get_panel_mode() == "search" {
            let s = sd::search(books, query, 200);
            if s.too_short {
                st.set_panel_summary("Type at least two letters.".into());
                return;
            }
            d.results = s
                .hits
                .iter()
                .map(|h| PassageEntry { book_id: h.book_id.clone(), name: h.name.clone(), chapter: h.chapter, verse: h.verse, text: h.text.clone() })
                .collect();
            let summary = if s.total == 0 {
                format!("No verses in {label} contain \"{}\".", s.query)
            } else if s.truncated {
                format!("{} VERSES · SHOWING FIRST {} · {label} · CLICK TO SELECT", s.total, s.hits.len())
            } else {
                format!("{} VERSES · {label} · CLICK TO SELECT", s.total)
            };
            st.set_panel_summary(summary.into());
        } else {
            match sd::parse_passage(query).and_then(|spans| sd::resolve(books, &spans)) {
                Ok(entries) => {
                    // A looked-up passage is selected straight away.
                    for e in &entries {
                        if !d.picked.iter().any(|p| p.book_id == e.book_id && p.chapter == e.chapter && p.verse == e.verse) {
                            d.picked.push(e.clone());
                        }
                    }
                    d.picked.sort_by_key(|e| sd::canon_pos(&e.book_id, e.chapter, e.verse));
                    let (_, reference) = sd::quote_for(&entries, &label);
                    st.set_panel_summary(format!("{reference} · {} VERSES · CLICK TO UNSELECT", entries.len()).into());
                    d.results = entries;
                }
                Err(e) => st.set_panel_summary(e.into()),
            }
        }
        d.verse_rows();
        picked_summary(ui, &core, d);
    }

    hook!(on_lookup, |ui, d, c, query| {
        run_lookup(&ui, c, d, query.as_str());
    });

    hook!(on_toggle_verse, |ui, d, c, index| {
        let Some(e) = d.results.get(index.max(0) as usize).cloned() else { return };
        if let Some(pos) = d.picked.iter().position(|p| p.book_id == e.book_id && p.chapter == e.chapter && p.verse == e.verse) {
            d.picked.remove(pos);
        } else {
            d.picked.push(e);
            d.picked.sort_by_key(|e| sd::canon_pos(&e.book_id, e.chapter, e.verse));
        }
        d.verse_rows();
        picked_summary(&ui, &c.borrow(), d);
    });

    hook!(on_unpick, |ui, d, c, index| {
        let i = index.max(0) as usize;
        if i < d.picked.len() {
            d.picked.remove(i);
        }
        d.verse_rows();
        picked_summary(&ui, &c.borrow(), d);
    });

    hook!(on_clear_picked, |ui, d, c| {
        d.picked.clear();
        d.verse_rows();
        picked_summary(&ui, &c.borrow(), d);
    });

    hook!(on_insert_picked, |ui, d, c| {
        if d.picked.is_empty() || d.doc.is_none() {
            return;
        }
        let label = short_label(&c.borrow(), &d.version);
        let (body, reference) = sd::quote_for(&d.picked, &label);
        d.snapshot();
        let at = insert_after(&ui, d, |id| {
            let mut b = Block::new(id, "quote", &body);
            b.extra = reference.clone();
            b
        });
        d.picked.clear();
        d.verse_rows();
        picked_summary(&ui, &c.borrow(), d);
        ui.global::<Studio>().set_save_state(format!("INSERTED {reference}").into());
        // Caret goes to the block after the quote, ready to keep writing.
        let next = (at + 1).min(d.doc.as_ref().unwrap().blocks.len() - 1);
        d.current = next;
        sync_current(&ui, d);
        focus_block(&ui, d, next, None, None);
    });

    // ---------------------------------------------------------- notes

    fn load_note(ui: &AppWindow, d: &mut Desk) {
        let st = ui.global::<Studio>();
        let note = d.note_id.as_ref().and_then(|id| d.notes.notes.iter().find(|n| &n.id == id)).cloned();
        match note {
            Some(n) => {
                // Body before title: the title field reports changes, and must see this note's body.
                st.set_note_body(n.body.into());
                st.set_note_id(n.id.into());
                st.set_note_title(n.title.into());
                st.set_note_state(format!("LAST CHANGE {}", when(n.updated)).into());
                st.set_note_tick(st.get_note_tick() + 1);
            }
            None => {
                d.note_id = None;
                st.set_note_id("".into());
                st.set_note_title("".into());
                st.set_note_body("".into());
            }
        }
        d.note_rows();
    }

    hook!(on_open_notes, |ui, d, c| {
        let _ = c;
        if !d.notes_dirty {
            d.notes = d.store.load_notes();
        }
        d.notes.notes.sort_by(|a, b| b.updated.cmp(&a.updated));
        if d.note_id.as_ref().map(|id| !d.notes.notes.iter().any(|n| &n.id == id)).unwrap_or(true) {
            d.note_id = d.notes.notes.first().map(|n| n.id.clone());
        }
        load_note(&ui, d);
        ui.global::<Session>().set_view("notes".into());
    });

    hook!(on_new_note, |ui, d, c| {
        let _ = c;
        let id = format!("n{}", Local::now().timestamp_millis());
        d.notes.notes.insert(0, Note { id: id.clone(), title: "New note".into(), body: String::new(), updated: sd::now_secs() });
        d.notes_dirty = true;
        d.note_id = Some(id);
        load_note(&ui, d);
        // A new note starts with its title selected, ready to type over.
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(40), move || {
            if let Some(ui) = weak.upgrade() {
                let st = ui.global::<Studio>();
                st.set_note_title_tick(st.get_note_title_tick() + 1);
            }
        });
    });

    hook!(on_open_note, |ui, d, c, id| {
        let _ = c;
        d.note_id = Some(id.to_string());
        load_note(&ui, d);
    });

    hook!(on_note_edited, |ui, d, c, title, body| {
        let _ = (c, &ui);
        let Some(id) = d.note_id.clone() else { return };
        let Some(n) = d.notes.notes.iter_mut().find(|n| n.id == id) else { return };
        if n.title == title.as_str() && n.body == body.as_str() {
            return;
        }
        n.title = title.to_string();
        n.body = body.to_string();
        n.updated = sd::now_secs();
        d.notes_dirty = true;
        d.note_rows();
    });

    desk
}

/// Applies one key from Slint's key event text to `text` with the selection `lo..hi` (byte offsets).
/// Printable text replaces the selection, Backspace/Delete remove; anything else only keeps the caret.
pub(crate) fn resume_edit(text: &str, lo: usize, hi: usize, key: &str) -> (String, usize) {
    let printable = !key.is_empty() && key.chars().all(|ch| !ch.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&ch));
    if printable {
        return (format!("{}{}{}", &text[..lo], key, &text[hi..]), lo + key.len());
    }
    match key {
        "\u{8}" => {
            if lo < hi {
                (format!("{}{}", &text[..lo], &text[hi..]), lo)
            } else if let Some((p, _)) = text[..lo].char_indices().next_back() {
                (format!("{}{}", &text[..p], &text[lo..]), p)
            } else {
                (text.to_string(), lo)
            }
        }
        "\u{7f}" => {
            if lo < hi {
                (format!("{}{}", &text[..lo], &text[hi..]), lo)
            } else if let Some(ch) = text[lo..].chars().next() {
                (format!("{}{}", &text[..lo], &text[lo + ch.len_utf8()..]), lo)
            } else {
                (text.to_string(), lo)
            }
        }
        _ => (text.to_string(), hi),
    }
}

#[cfg(test)]
mod resume_tests {
    use super::resume_edit;

    #[test]
    fn resume_edit_inserts_deletes_and_keeps_caret() {
        assert_eq!(resume_edit("abc", 1, 1, "X"), ("aXbc".to_string(), 2));
        assert_eq!(resume_edit("abc", 0, 2, "é"), ("éc".to_string(), 2));
        assert_eq!(resume_edit("aéc", 3, 3, "\u{8}"), ("ac".to_string(), 1));
        assert_eq!(resume_edit("abc", 0, 0, "\u{8}"), ("abc".to_string(), 0));
        assert_eq!(resume_edit("abc", 1, 1, "\u{7f}"), ("ac".to_string(), 1));
        assert_eq!(resume_edit("abc", 1, 1, "\n"), ("abc".to_string(), 1));
        assert_eq!(resume_edit("abc", 1, 1, "\u{f700}"), ("abc".to_string(), 1));
    }
}
