//! Outreach: contacts, prayer rotation, a small calendar with repeating events, and the
//! private on-device storage for them. Everything here is plain data and pure functions so it
//! can be unit tested without a window. No networking.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

pub const PHOTO_SIZE: u32 = 256;
const MAX_NAME: usize = 120;
const MAX_SHORT: usize = 200;
const MAX_LONG: usize = 4000;

// ---------------------------------------------------------------- data

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub prayer_request: String,
    /// The praying-hands toggle: this person is in the daily rotation pool.
    #[serde(default)]
    pub in_pool: bool,
    /// The prayer request has been answered. Drops out of the rotation, stays in the list.
    #[serde(default)]
    pub fulfilled: bool,
    /// File name inside the photos directory, when there is a picture.
    #[serde(default)]
    pub photo: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    #[default]
    None,
    Weekly,
    Twice,
    Biweekly,
}

impl Repeat {
    pub fn parse(text: &str) -> Repeat {
        match text {
            "weekly" => Repeat::Weekly,
            "twice" => Repeat::Twice,
            "biweekly" => Repeat::Biweekly,
            _ => Repeat::None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Repeat::None => "none",
            Repeat::Weekly => "weekly",
            Repeat::Twice => "twice",
            Repeat::Biweekly => "biweekly",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Repeat::None => "",
            Repeat::Weekly => "weekly",
            Repeat::Twice => "twice a week",
            Repeat::Biweekly => "every other week",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub details: String,
    /// First day, YYYY-MM-DD.
    pub date: String,
    /// HH:MM, 24 hour. Empty means no set time.
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub repeat: Repeat,
    /// Weekdays for "twice a week", 0 = Monday ... 6 = Sunday.
    #[serde(default)]
    pub days: Vec<u8>,
    #[serde(default)]
    pub contact_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub index: usize,
    pub date: NaiveDate,
}

pub fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

/// Accepts "19:30", "7:30", "7:30pm", "7pm", "730pm". Returns canonical HH:MM, or "" for blank.
pub fn parse_time(text: &str) -> Result<String, String> {
    let t = text.trim().to_lowercase().replace(' ', "");
    if t.is_empty() {
        return Ok(String::new());
    }
    let (body, pm, am) = if let Some(b) = t.strip_suffix("pm") {
        (b.to_string(), true, false)
    } else if let Some(b) = t.strip_suffix("am") {
        (b.to_string(), false, true)
    } else {
        (t.clone(), false, false)
    };
    let (h, m) = if let Some((h, m)) = body.split_once(':') {
        (h.to_string(), m.to_string())
    } else if body.len() > 2 && body.chars().all(|c| c.is_ascii_digit()) {
        (body[..body.len() - 2].to_string(), body[body.len() - 2..].to_string())
    } else {
        (body.clone(), "0".to_string())
    };
    let bad = || "Time looks like 19:30 or 7:30 pm.".to_string();
    let mut h: u32 = h.parse().map_err(|_| bad())?;
    let m: u32 = m.parse().map_err(|_| bad())?;
    if pm || am {
        if h == 0 || h > 12 {
            return Err(bad());
        }
        if pm && h < 12 {
            h += 12;
        }
        if am && h == 12 {
            h = 0;
        }
    }
    NaiveTime::from_hms_opt(h, m, 0).ok_or_else(bad)?;
    Ok(format!("{h:02}:{m:02}"))
}

pub fn clean_days(days: &[u8], fallback: u8) -> Vec<u8> {
    let set: BTreeSet<u8> = days.iter().copied().filter(|d| *d < 7).collect();
    if set.is_empty() {
        vec![fallback]
    } else {
        set.into_iter().collect()
    }
}

/// Does this event happen on `day`?
pub fn occurs_on(event: &Event, day: NaiveDate) -> bool {
    let Some(start) = parse_date(&event.date) else { return false };
    if day < start {
        return false;
    }
    let gap = (day - start).num_days();
    match event.repeat {
        Repeat::None => gap == 0,
        Repeat::Weekly => gap % 7 == 0,
        Repeat::Biweekly => gap % 14 == 0,
        Repeat::Twice => {
            let days = clean_days(&event.days, start.weekday().num_days_from_monday() as u8);
            days.contains(&(day.weekday().num_days_from_monday() as u8))
        }
    }
}

/// Every occurrence from `from` to `to` inclusive, ordered by day then time then title.
pub fn expand(events: &[Event], from: NaiveDate, to: NaiveDate) -> Vec<Occurrence> {
    let mut out = Vec::new();
    if to < from {
        return out;
    }
    let mut day = from;
    while day <= to {
        for (index, event) in events.iter().enumerate() {
            if occurs_on(event, day) {
                out.push(Occurrence { index, date: day });
            }
        }
        day += Duration::days(1);
    }
    out.sort_by(|a, b| {
        (a.date, &events[a.index].time, events[a.index].title.to_lowercase())
            .cmp(&(b.date, &events[b.index].time, events[b.index].title.to_lowercase()))
    });
    out
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    (NaiveDate::from_ymd_opt(ny, nm, 1).unwrap() - NaiveDate::from_ymd_opt(year, month, 1).unwrap()).num_days() as u32
}

pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year * 12 + month as i32 - 1 + delta;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

// ---------------------------------------------------------------- contacts helpers

pub fn initials(name: &str) -> String {
    let letters: Vec<char> = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .collect();
    let mut out = String::new();
    match letters.len() {
        0 => out.push('?'),
        1 => out.extend(letters[0].to_uppercase()),
        n => {
            out.extend(letters[0].to_uppercase());
            out.extend(letters[n - 1].to_uppercase());
        }
    }
    out
}

pub fn contact_summary(c: &Contact) -> String {
    let mut out = c.name.clone();
    if !c.phone.is_empty() {
        out.push_str(&format!("\nPhone: {}", c.phone));
    }
    if !c.email.is_empty() {
        out.push_str(&format!("\nEmail: {}", c.email));
    }
    if !c.prayer_request.is_empty() {
        out.push_str(&format!("\nPrayer request: {}", c.prayer_request));
    }
    out
}

pub fn valid_email(email: &str) -> bool {
    let e = email.trim();
    if e.is_empty() || e.len() > 254 || e.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    match e.split_once('@') {
        Some((local, domain)) => !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !domain.contains('@'),
        None => false,
    }
}

/// Percent-encodes everything except a small safe set, so the address can never smuggle
/// extra arguments or shell characters into the system opener.
pub fn mailto_url(email: &str) -> Option<String> {
    if !valid_email(email) {
        return None;
    }
    let mut out = String::from("mailto:");
    for b in email.trim().bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'@' | b'.' | b'_' | b'-' | b'+' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    Some(out)
}

fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

impl Contact {
    pub fn sanitize(&mut self) {
        self.name = clip(&self.name, MAX_NAME);
        self.phone = clip(&self.phone, MAX_SHORT);
        self.email = clip(&self.email, 254);
        self.prayer_request = clip(&self.prayer_request, MAX_LONG);
    }
}

impl Event {
    pub fn sanitize(&mut self) {
        self.title = clip(&self.title, MAX_SHORT);
        self.details = clip(&self.details, MAX_LONG);
        let start = parse_date(&self.date);
        if let Some(start) = start {
            self.date = start.format("%Y-%m-%d").to_string();
        }
        self.time = parse_time(&self.time).unwrap_or_default();
        let fallback = start.map(|d| d.weekday().num_days_from_monday() as u8).unwrap_or(0);
        self.days = if self.repeat == Repeat::Twice { clean_days(&self.days, fallback) } else { Vec::new() };
    }
}

// ---------------------------------------------------------------- rotation

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rotation {
    /// The day `today` was chosen for, YYYY-MM-DD.
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub today: Option<u64>,
    /// Shown on the most recent earlier day.
    #[serde(default)]
    pub prev: Option<u64>,
    /// Still waiting for a turn in this round, in order.
    #[serde(default)]
    pub bag: Vec<u64>,
    /// Already had a turn this round.
    #[serde(default)]
    pub done: Vec<u64>,
    #[serde(default)]
    pub last_shown: BTreeMap<u64, String>,
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A fresh round: everyone in the pool, shuffled, with people not shown for longer weighted
/// toward the front (Efraimidis-Spirakis keys). Deterministic from the date and the pool.
fn fresh_round(rot: &Rotation, pool: &[u64], today: NaiveDate) -> Vec<u64> {
    let seed = splitmix(today.num_days_from_ce() as u64);
    let mut keyed: Vec<(f64, u64)> = pool
        .iter()
        .map(|&id| {
            let age = rot
                .last_shown
                .get(&id)
                .and_then(|d| parse_date(d))
                .map(|d| (today - d).num_days().clamp(0, 365))
                .unwrap_or(365);
            let weight = 1.0 + age as f64;
            let u = ((splitmix(seed ^ id.wrapping_mul(0xA24B_AED4_963E_E407)) >> 11) as f64 + 1.0) / ((1u64 << 53) as f64 + 1.0);
            (u.ln() / weight, id)
        })
        .collect();
    keyed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    keyed.into_iter().map(|(_, id)| id).collect()
}

/// Chooses (or keeps) the person to pray for on `today`. `pool` is the ids with the hands
/// toggle on and not fulfilled. Calling again the same day returns the same person.
pub fn advance(rot: &mut Rotation, pool: &[u64], today: NaiveDate) -> Option<u64> {
    let today_key = today.format("%Y-%m-%d").to_string();
    if pool.is_empty() {
        if rot.day != today_key {
            rot.prev = rot.today.or(rot.prev);
        }
        rot.day = today_key;
        rot.today = None;
        return None;
    }
    if rot.day == today_key {
        if let Some(id) = rot.today {
            if pool.contains(&id) {
                return Some(id);
            }
        }
    } else {
        rot.prev = rot.today.or(rot.prev);
    }
    rot.bag.retain(|id| pool.contains(id));
    rot.done.retain(|id| pool.contains(id));
    if rot.bag.is_empty() && rot.done.is_empty() {
        rot.bag = fresh_round(rot, pool, today);
    }
    for id in pool {
        if !rot.bag.contains(id) && !rot.done.contains(id) {
            rot.bag.push(*id);
        }
    }
    let prev = rot.prev;
    let single_stale = pool.len() > 1 && rot.bag.len() == 1 && Some(rot.bag[0]) == prev;
    if rot.bag.is_empty() || single_stale {
        rot.done.clear();
        rot.bag = fresh_round(rot, pool, today);
    }
    if pool.len() > 1 && Some(rot.bag[0]) == prev {
        let last = rot.bag.len() - 1;
        rot.bag.swap(0, last);
    }
    let pick = rot.bag.remove(0);
    rot.done.push(pick);
    rot.last_shown.insert(pick, today_key.clone());
    rot.today = Some(pick);
    rot.day = today_key;
    Some(pick)
}

// ---------------------------------------------------------------- photos

pub fn make_thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|_| "That file is not a picture I can read.".to_string())?;
    // Resize to cover, then center-crop: a square thumbnail.
    let thumb = img.resize_to_fill(PHOTO_SIZE, PHOTO_SIZE, image::imageops::FilterType::Lanczos3).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode(thumb.as_raw(), thumb.width(), thumb.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn decode_rgba(jpeg: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(jpeg).ok()?.to_rgba8();
    Some((img.width(), img.height(), img.into_raw()))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() / 3 * 4 + 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = buf << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- storage

/// Writes through a temp file and a rename. Owner-only permissions on unix.
pub fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension(match path.extension() {
        Some(ext) => format!("{}.tmp", ext.to_string_lossy()),
        None => "tmp".into(),
    });
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
        file.write_all(data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn private_dir(path: &Path) {
    let _ = fs::create_dir_all(path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
}

#[derive(Default)]
pub struct Store {
    pub dir: PathBuf,
    pub contacts: Vec<Contact>,
    pub events: Vec<Event>,
    pub rotation: Rotation,
}

impl Store {
    pub fn load(dir: &Path) -> Store {
        let read = |name: &str| fs::read_to_string(dir.join(name)).ok();
        let mut contacts: Vec<Contact> = read("contacts.json").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let mut events: Vec<Event> = read("events.json").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let rotation: Rotation = read("prayer-rotation.json").and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        for c in &mut contacts {
            c.sanitize();
        }
        for e in &mut events {
            e.sanitize();
        }
        events.retain(|e| parse_date(&e.date).is_some());
        Store { dir: dir.to_path_buf(), contacts, events, rotation }
    }

    pub fn photos_dir(&self) -> PathBuf {
        self.dir.join("photos")
    }

    pub fn save_contacts(&self) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(&self.contacts).map_err(|e| e.to_string())?;
        write_private(&self.dir.join("contacts.json"), &data)
    }
    pub fn save_events(&self) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(&self.events).map_err(|e| e.to_string())?;
        write_private(&self.dir.join("events.json"), &data)
    }
    pub fn save_rotation(&self) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(&self.rotation).map_err(|e| e.to_string())?;
        write_private(&self.dir.join("prayer-rotation.json"), &data)
    }

    pub fn next_contact_id(&self) -> u64 {
        self.contacts.iter().map(|c| c.id).max().unwrap_or(0) + 1
    }
    pub fn next_event_id(&self) -> u64 {
        self.events.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }

    pub fn contact(&self, id: u64) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.id == id)
    }
    pub fn contact_mut(&mut self, id: u64) -> Option<&mut Contact> {
        self.contacts.iter_mut().find(|c| c.id == id)
    }

    /// Alphabetical by name, ignoring case.
    pub fn sorted(&self) -> Vec<&Contact> {
        let mut list: Vec<&Contact> = self.contacts.iter().collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
        list
    }

    pub fn pool(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.sorted().iter().filter(|c| c.in_pool && !c.fulfilled).map(|c| c.id).collect();
        ids.sort();
        ids
    }

    pub fn photo_file(&self, c: &Contact) -> Option<PathBuf> {
        c.photo.as_ref().map(|f| self.photos_dir().join(f))
    }

    pub fn write_photo(&self, id: u64, jpeg: &[u8]) -> Result<String, String> {
        private_dir(&self.photos_dir());
        let name = format!("contact-{id}.jpg");
        write_private(&self.photos_dir().join(&name), jpeg)?;
        Ok(name)
    }

    pub fn remove_photo_file(&self, c: &Contact) {
        if let Some(path) = self.photo_file(c) {
            let _ = fs::remove_file(path);
        }
    }

    pub fn delete_contact(&mut self, id: u64) {
        if let Some(c) = self.contact(id).cloned() {
            self.remove_photo_file(&c);
        }
        self.contacts.retain(|c| c.id != id);
        for e in &mut self.events {
            if e.contact_id == Some(id) {
                e.contact_id = None;
            }
        }
    }

    /// Picks today's prayer person, and saves the rotation when it changed.
    pub fn todays_prayer(&mut self, today: NaiveDate) -> Option<u64> {
        let before = self.rotation.clone();
        let pool = self.pool();
        let pick = advance(&mut self.rotation, &pool, today);
        if self.rotation != before {
            let _ = self.save_rotation();
        }
        pick
    }
}

// ---------------------------------------------------------------- backup sections

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ContactBackup {
    #[serde(flatten)]
    pub contact: Contact,
    /// The 256 px JPEG thumbnail, base64 encoded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_b64: Option<String>,
}

pub fn contacts_for_backup(store: &Store) -> Vec<ContactBackup> {
    store
        .sorted()
        .into_iter()
        .map(|c| {
            let photo_b64 = store.photo_file(c).and_then(|p| fs::read(p).ok()).map(|b| b64_encode(&b));
            let mut contact = c.clone();
            contact.photo = None;
            ContactBackup { contact, photo_b64 }
        })
        .collect()
}

fn same_person(a: &Contact, b: &Contact) -> bool {
    a.name.trim().to_lowercase() == b.name.trim().to_lowercase() && a.phone.trim() == b.phone.trim() && a.email.trim().to_lowercase() == b.email.trim().to_lowercase()
}

/// Folds backup contacts and events into the store. Replace swaps both lists; merge adds what
/// is missing. Ids are reassigned, and photos are re-encoded through the thumbnail path so
/// a hostile file cannot choose a path or smuggle a huge image. Returns (contacts, events) added.
pub fn apply_backup_sections(store: &mut Store, contacts: &[ContactBackup], events: &[Event], replace: bool) -> (usize, usize) {
    let mut id_map: BTreeMap<u64, u64> = BTreeMap::new();
    if replace {
        let old: Vec<Contact> = store.contacts.clone();
        for c in &old {
            store.remove_photo_file(c);
        }
        store.contacts.clear();
        store.events.clear();
        store.rotation = Rotation::default();
    }
    let mut added_contacts = 0;
    for item in contacts {
        let mut incoming = item.contact.clone();
        incoming.sanitize();
        incoming.photo = None;
        if incoming.name.is_empty() {
            continue;
        }
        if let Some(existing) = store.contacts.iter().find(|c| same_person(c, &incoming)) {
            id_map.insert(item.contact.id, existing.id);
            continue;
        }
        let new_id = store.next_contact_id();
        id_map.insert(item.contact.id, new_id);
        incoming.id = new_id;
        if let Some(jpeg) = item.photo_b64.as_deref().and_then(b64_decode).and_then(|b| make_thumbnail(&b).ok()) {
            incoming.photo = store.write_photo(new_id, &jpeg).ok();
        }
        store.contacts.push(incoming);
        added_contacts += 1;
    }
    let mut added_events = 0;
    for item in events {
        let mut incoming = item.clone();
        incoming.sanitize();
        if parse_date(&incoming.date).is_none() {
            continue;
        }
        incoming.contact_id = incoming.contact_id.and_then(|id| id_map.get(&id).copied());
        let dup = store.events.iter().any(|e| e.title == incoming.title && e.date == incoming.date && e.time == incoming.time && e.repeat == incoming.repeat && e.days == incoming.days);
        if dup {
            continue;
        }
        incoming.id = store.next_event_id();
        store.events.push(incoming);
        added_events += 1;
    }
    (added_contacts, added_events)
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn ev(date: &str, repeat: Repeat, days: &[u8]) -> Event {
        Event { id: 1, title: "t".into(), date: date.into(), repeat, days: days.to_vec(), ..Default::default() }
    }

    #[test]
    fn once_only_on_its_day() {
        let e = ev("2026-10-14", Repeat::None, &[]);
        assert!(occurs_on(&e, d(2026, 10, 14)));
        assert!(!occurs_on(&e, d(2026, 10, 21)));
        assert!(!occurs_on(&e, d(2026, 10, 13)));
    }

    #[test]
    fn weekly_and_biweekly() {
        let w = ev("2026-10-14", Repeat::Weekly, &[]); // a Wednesday
        assert!(occurs_on(&w, d(2026, 10, 21)));
        assert!(occurs_on(&w, d(2026, 11, 4)));
        assert!(!occurs_on(&w, d(2026, 10, 22)));
        assert!(!occurs_on(&w, d(2026, 10, 7)), "never before the start");
        let b = ev("2026-10-14", Repeat::Biweekly, &[]);
        assert!(!occurs_on(&b, d(2026, 10, 21)));
        assert!(occurs_on(&b, d(2026, 10, 28)));
        assert!(occurs_on(&b, d(2027, 1, 6)));
    }

    #[test]
    fn twice_a_week_on_chosen_days() {
        // Start Wed 2026-10-14, repeat Mon + Thu.
        let e = ev("2026-10-14", Repeat::Twice, &[0, 3]);
        let got = expand(&[e], d(2026, 10, 12), d(2026, 10, 26));
        let days: Vec<NaiveDate> = got.iter().map(|o| o.date).collect();
        assert_eq!(days, vec![d(2026, 10, 15), d(2026, 10, 19), d(2026, 10, 22), d(2026, 10, 26)]);
    }

    #[test]
    fn twice_with_no_days_falls_back_to_start_weekday() {
        let e = ev("2026-10-14", Repeat::Twice, &[]);
        assert!(occurs_on(&e, d(2026, 10, 21)));
        assert!(!occurs_on(&e, d(2026, 10, 22)));
    }

    #[test]
    fn expand_orders_by_time_and_handles_month_edges() {
        let mut a = ev("2026-02-27", Repeat::Weekly, &[]);
        a.time = "18:00".into();
        let mut b = ev("2026-02-27", Repeat::None, &[]);
        b.time = "09:00".into();
        let got = expand(&[a, b], d(2026, 2, 1), d(2026, 3, 31));
        assert_eq!(got.len(), 1 + 5); // 27 Feb, 6 13 20 27 Mar + the single
        assert_eq!(got[0].index, 1);
        assert_eq!(got[1].index, 0);
        assert!(expand(&[], d(2026, 3, 1), d(2026, 2, 1)).is_empty());
    }

    #[test]
    fn month_math() {
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 5, -17), (2024, 12));
    }

    #[test]
    fn time_parsing() {
        assert_eq!(parse_time("19:30").unwrap(), "19:30");
        assert_eq!(parse_time("7:05 pm").unwrap(), "19:05");
        assert_eq!(parse_time("12am").unwrap(), "00:00");
        assert_eq!(parse_time("12pm").unwrap(), "12:00");
        assert_eq!(parse_time("").unwrap(), "");
        assert!(parse_time("25:00").is_err());
        assert!(parse_time("soon").is_err());
        assert!(parse_time("13pm").is_err());
    }

    fn run(pool: &[u64], rot: &mut Rotation, start: NaiveDate, days: i64) -> Vec<u64> {
        (0..days).map(|i| advance(rot, pool, start + Duration::days(i)).unwrap()).collect()
    }

    #[test]
    fn rotation_everyone_once_per_round_and_deterministic() {
        let pool = [1, 2, 3, 4, 5];
        let mut a = Rotation::default();
        let mut b = Rotation::default();
        let sa = run(&pool, &mut a, d(2026, 10, 1), 20);
        let sb = run(&pool, &mut b, d(2026, 10, 1), 20);
        assert_eq!(sa, sb, "same dates, same result");
        for round in sa.chunks(5) {
            let set: BTreeSet<u64> = round.iter().copied().collect();
            assert_eq!(set.len(), 5, "no repeat until everyone had a turn: {round:?}");
        }
        for pair in sa.windows(2) {
            assert_ne!(pair[0], pair[1], "never the same person two days running");
        }
    }

    #[test]
    fn rotation_same_day_is_stable() {
        let pool = [1, 2, 3];
        let mut rot = Rotation::default();
        let first = advance(&mut rot, &pool, d(2026, 10, 1));
        for _ in 0..5 {
            assert_eq!(advance(&mut rot, &pool, d(2026, 10, 1)), first);
        }
    }

    #[test]
    fn rotation_edge_cases() {
        let mut rot = Rotation::default();
        assert_eq!(advance(&mut rot, &[], d(2026, 10, 1)), None);
        let mut one = Rotation::default();
        assert!(run(&[7], &mut one, d(2026, 10, 1), 6).iter().all(|&id| id == 7));
        let mut two = Rotation::default();
        let s = run(&[1, 2], &mut two, d(2026, 10, 1), 8);
        for pair in s.windows(2) {
            assert_ne!(pair[0], pair[1], "two people alternate: {s:?}");
        }
        let mut three = Rotation::default();
        let s = run(&[1, 2, 3], &mut three, d(2026, 10, 1), 12);
        for round in s.chunks(3) {
            assert_eq!(round.iter().copied().collect::<BTreeSet<_>>().len(), 3);
        }
        for pair in s.windows(2) {
            assert_ne!(pair[0], pair[1]);
        }
    }

    #[test]
    fn rotation_handles_pool_changes() {
        let mut rot = Rotation::default();
        let first = advance(&mut rot, &[1, 2, 3], d(2026, 10, 1)).unwrap();
        // The person shown is removed mid-day: someone else is picked, never the removed one.
        let rest: Vec<u64> = [1, 2, 3].into_iter().filter(|&i| i != first).collect();
        let swapped = advance(&mut rot, &rest, d(2026, 10, 1)).unwrap();
        assert_ne!(swapped, first);
        // A newcomer joins and is seen within one round.
        let pool = [1, 2, 3, 9];
        let seen = run(&pool, &mut rot, d(2026, 10, 2), 4);
        assert!(seen.contains(&9));
        // Pool empties, then refills.
        assert_eq!(advance(&mut rot, &[], d(2026, 10, 7)), None);
        assert!(advance(&mut rot, &[1, 2], d(2026, 10, 8)).is_some());
    }

    #[test]
    fn reshuffle_never_starts_with_yesterdays_person() {
        for seed_day in 1..28 {
            let pool = [1, 2, 3, 4];
            let mut rot = Rotation::default();
            let s = run(&pool, &mut rot, d(2026, 9, seed_day), 24);
            for round_start in (4..24).step_by(4) {
                assert_ne!(s[round_start], s[round_start - 1]);
            }
        }
    }

    #[test]
    fn staleness_weights_toward_unseen_people() {
        // 1..=6 shown yesterday-ish, 7 not for a year: across many seeds 7 leads a fresh round often.
        let mut lead = 0;
        for day in 1..=28 {
            let mut rot = Rotation::default();
            for id in 1..=6u64 {
                rot.last_shown.insert(id, "2026-09-30".into());
            }
            let pool: Vec<u64> = (1..=7).collect();
            if advance(&mut rot, &pool, d(2026, 10, day)) == Some(7) {
                lead += 1;
            }
        }
        // Uniform would be ~4 of 28.
        assert!(lead >= 14, "stale person led only {lead} of 28 rounds");
    }

    #[test]
    fn mailto_is_safe() {
        assert_eq!(mailto_url("jane.doe@example.com").unwrap(), "mailto:jane.doe@example.com");
        assert!(mailto_url("not an email").is_none());
        assert!(mailto_url("a@b.c&calc").unwrap().contains("%26"));
        assert!(mailto_url("x@y.org\n--arg").is_none());
        assert_eq!(mailto_url("a b@c.de"), None);
    }

    #[test]
    fn summary_and_initials() {
        let c = Contact { name: "Jane Q Public".into(), phone: "555-0100".into(), prayer_request: "Healing".into(), ..Default::default() };
        assert_eq!(contact_summary(&c), "Jane Q Public\nPhone: 555-0100\nPrayer request: Healing");
        assert_eq!(initials("Jane Q Public"), "JP");
        assert_eq!(initials("madonna"), "M");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn base64_roundtrip() {
        for len in 0..20 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).unwrap(), data);
        }
        assert_eq!(b64_encode(b"Man"), "TWFu");
        assert!(b64_decode("@@@").is_none());
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("twm-outreach-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_png() -> Vec<u8> {
        let img = image::RgbImage::from_fn(600, 300, |x, _| image::Rgb([(x % 256) as u8, 80, 200]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn thumbnail_is_square_jpeg() {
        let jpeg = make_thumbnail(&sample_png()).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let img = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (256, 256));
        assert!(make_thumbnail(b"nope").is_err());
    }

    #[test]
    fn store_roundtrip_atomic_private_and_backup() {
        let dir = temp("store");
        let mut store = Store::load(&dir);
        store.contacts.push(Contact { id: 1, name: "Zed Fake".into(), in_pool: true, ..Default::default() });
        store.contacts.push(Contact { id: 2, name: "amy Fake".into(), email: "amy@example.com".into(), in_pool: true, fulfilled: true, ..Default::default() });
        let name = store.write_photo(2, &make_thumbnail(&sample_png()).unwrap()).unwrap();
        store.contact_mut(2).unwrap().photo = Some(name);
        store.events.push(Event { id: 1, title: "Visit".into(), date: "2026-10-20".into(), repeat: Repeat::Twice, days: vec![1, 4], contact_id: Some(2), ..Default::default() });
        store.save_contacts().unwrap();
        store.save_events().unwrap();
        assert!(!dir.join("contacts.json.tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(dir.join("contacts.json")).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(fs::metadata(store.photos_dir().join("contact-2.jpg")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let again = Store::load(&dir);
        assert_eq!(again.sorted()[0].name, "amy Fake", "alphabetical, case-insensitive");
        assert_eq!(again.pool(), vec![1], "fulfilled drop out of the pool");
        assert_eq!(again.events[0].days, vec![1, 4]);

        // Backup into a different store.
        let backup = contacts_for_backup(&again);
        assert!(backup[0].photo_b64.is_some());
        let json = serde_json::to_string(&backup).unwrap();
        let parsed: Vec<ContactBackup> = serde_json::from_str(&json).unwrap();
        let other_dir = temp("other");
        let mut other = Store::load(&other_dir);
        other.contacts.push(Contact { id: 1, name: "Existing Fake".into(), ..Default::default() });
        let (c, e) = apply_backup_sections(&mut other, &parsed, &again.events, false);
        assert_eq!((c, e), (2, 1));
        assert_eq!(other.contacts.len(), 3);
        let amy = other.contacts.iter().find(|c| c.name == "amy Fake").unwrap();
        assert!(other.photo_file(amy).unwrap().exists());
        assert_eq!(other.events[0].contact_id, Some(amy.id), "event follows its contact's new id");
        let amy_id_first = amy.id;
        let _ = amy_id_first;
        // Merging again adds nothing.
        assert_eq!(apply_backup_sections(&mut other, &parsed, &again.events, false), (0, 0));
        // Replace swaps.
        assert_eq!(apply_backup_sections(&mut other, &parsed, &again.events, true), (2, 1));
        assert_eq!(other.contacts.len(), 2);
        let amy_id = other.contacts.iter().find(|c| c.name == "amy Fake").unwrap().id;
        other.delete_contact(amy_id);
        assert_eq!(other.contacts.len(), 1);
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(other_dir);
    }

    #[test]
    fn old_backup_without_sections_still_parses() {
        // Mirrors extras::Backup: missing sections are simply absent.
        let item: Result<Vec<ContactBackup>, _> = serde_json::from_str("[]");
        assert!(item.unwrap().is_empty());
    }
}
