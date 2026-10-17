//! Wires the Outreach planner, contact cards, and the home prayer card to the window.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use std::rc::Rc;

use chrono::{Datelike, Duration, Local, NaiveDate};
use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer};

use twm_bible_native::outreach::{self as ou, Contact, Event, Repeat};

use crate::{show_status, AppWindow, CalCell, ContactRow, Core, EvRow, Models, Outreach, Session};

pub enum Confirm {
    Contact(u64),
    Event(u64),
}

pub struct OutreachState {
    pub year: i32,
    pub month: u32,
    pub selected: NaiveDate,
    pub today: NaiveDate,
    pub pending_photo: Option<Vec<u8>>,
    pub photo_cleared: bool,
    pub card_id: Option<u64>,
    pub event_id: Option<u64>,
    pub event_contact: Option<u64>,
    pub confirm: Option<Confirm>,
}

impl OutreachState {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        OutreachState {
            year: today.year(),
            month: today.month(),
            selected: today,
            today,
            pending_photo: None,
            photo_cleared: false,
            card_id: None,
            event_id: None,
            event_contact: None,
            confirm: None,
        }
    }
}

type CoreRc = Rc<RefCell<Core>>;

fn image_from_jpeg(bytes: &[u8]) -> Option<slint::Image> {
    let (w, h, raw) = ou::decode_rgba(bytes)?;
    let buf = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&raw, w, h);
    Some(slint::Image::from_rgba8(buf))
}

fn contact_row(store: &ou::Store, c: &Contact) -> ContactRow {
    let image = store.photo_file(c).and_then(|p| fs::read(p).ok()).and_then(|b| image_from_jpeg(&b));
    ContactRow {
        id: c.id as i32,
        name: c.name.clone().into(),
        phone: c.phone.clone().into(),
        email: c.email.clone().into(),
        prayer: c.prayer_request.clone().into(),
        initials: ou::initials(&c.name).into(),
        has_photo: image.is_some(),
        photo: image.unwrap_or_default(),
        has_email: ou::valid_email(&c.email),
        in_pool: c.in_pool,
        fulfilled: c.fulfilled,
    }
}

fn when_text(event: &Event) -> String {
    if event.time.is_empty() { "ALL DAY".into() } else { event.time.clone() }
}

fn ev_row(event: &Event, when: String) -> EvRow {
    EvRow {
        id: event.id as i32,
        title: event.title.clone().into(),
        when: when.into(),
        details: event.details.clone().into(),
        repeat: event.repeat.label().to_uppercase().into(),
    }
}

pub fn first_cell(year: i32, month: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    first - Duration::days(first.weekday().num_days_from_sunday() as i64)
}

pub fn publish(ui: &AppWindow, core: &Core, models: &Models) {
    let state = &core.outreach_ui;
    let store = &core.outreach;
    let out = ui.global::<Outreach>();
    let rows: Vec<ContactRow> = store.sorted().into_iter().map(|c| contact_row(store, c)).collect();
    out.set_contact_count(rows.len() as i32);
    let pool: Vec<ContactRow> = rows.iter().filter(|r| r.in_pool && !r.fulfilled).cloned().collect();
    out.set_pr_pool_size(pool.len() as i32);
    models.out_pool.set_vec(pool);
    models.out_contacts.set_vec(rows);

    // Month grid, Sunday first, six weeks.
    let start = first_cell(state.year, state.month);
    let end = start + Duration::days(41);
    let occurrences = ou::expand(&store.events, start, end);
    let mut by_day: BTreeMap<NaiveDate, Vec<&Event>> = BTreeMap::new();
    for occ in &occurrences {
        by_day.entry(occ.date).or_default().push(&store.events[occ.index]);
    }
    let cells: Vec<CalCell> = (0..42)
        .map(|i| {
            let date = start + Duration::days(i);
            let list = by_day.get(&date);
            let title = |n: usize| {
                list.and_then(|l| l.get(n))
                    .map(|e| if e.time.is_empty() { e.title.clone() } else { format!("{} {}", e.time, e.title) })
                    .unwrap_or_default()
            };
            CalCell {
                day: date.day() as i32,
                in_month: date.month() == state.month && date.year() == state.year,
                today: date == state.today,
                selected: date == state.selected,
                count: list.map(|l| l.len()).unwrap_or(0) as i32,
                t1: title(0).into(),
                t2: title(1).into(),
            }
        })
        .collect();
    models.out_cells.set_vec(cells);
    let first = NaiveDate::from_ymd_opt(state.year, state.month, 1).unwrap();
    out.set_month_label(first.format("%B %Y").to_string().to_uppercase().into());
    out.set_selected_label(state.selected.format("%A, %B %-d, %Y").to_string().into());
    let day_events: Vec<EvRow> = ou::expand(&store.events, state.selected, state.selected)
        .iter()
        .map(|o| {
            let e = &store.events[o.index];
            ev_row(e, when_text(e))
        })
        .collect();
    models.out_day.set_vec(day_events);
    let upcoming: Vec<EvRow> = ou::expand(&store.events, state.today, state.today + Duration::days(30))
        .iter()
        .take(200)
        .map(|o| {
            let e = &store.events[o.index];
            let when = format!("{} {}", o.date.format("%a %b %-d"), e.time).trim().to_string();
            ev_row(e, when)
        })
        .collect();
    models.out_upcoming.set_vec(upcoming);
}

/// Today's prayer card. Called at startup, after any contact change, and after midnight.
pub fn publish_prayer(ui: &AppWindow, core: &mut Core) {
    let today = Local::now().date_naive();
    core.outreach_ui.today = today;
    let pick = core.outreach.todays_prayer(today);
    let out = ui.global::<Outreach>();
    match pick.and_then(|id| core.outreach.contact(id).cloned()) {
        Some(c) => {
            let row = contact_row(&core.outreach, &c);
            out.set_pr_has(true);
            out.set_pr_name(row.name);
            out.set_pr_request(row.prayer);
            out.set_pr_initials(row.initials);
            out.set_pr_has_photo(row.has_photo);
            out.set_pr_photo(row.photo);
        }
        None => {
            out.set_pr_has(false);
            out.set_pr_name("".into());
            out.set_pr_request("".into());
            out.set_pr_has_photo(false);
        }
    }
}

pub fn refresh(ui: &AppWindow, core: &CoreRc, models: &Models) {
    publish_prayer(ui, &mut core.borrow_mut());
    publish(ui, &core.borrow(), models);
}

/// Once a second: roll the prayer card and the "today" highlight over at midnight.
pub fn tick(ui: &AppWindow, core: &CoreRc, models: &Models) {
    let now = Local::now().date_naive();
    if core.borrow().outreach_ui.today != now {
        let was_on_today = core.borrow().outreach_ui.selected == core.borrow().outreach_ui.today;
        if was_on_today {
            let mut c = core.borrow_mut();
            c.outreach_ui.selected = now;
            c.outreach_ui.year = now.year();
            c.outreach_ui.month = now.month();
        }
        refresh(ui, core, models);
    }
}

fn open_system(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(windows)]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    let mut child = twm_bible_native::quiet(command.arg(url))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "Could not open your mail program.".to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn find_contact(core: &CoreRc, id: i32) -> Option<Contact> {
    core.borrow().outreach.contact(id as u64).cloned()
}

fn save_err(ui: &AppWindow, core: &CoreRc, what: &str, result: Result<(), String>) {
    if let Err(err) = result {
        show_status(ui, &mut core.borrow_mut(), format!("Could not save {what}. {err}"));
    }
}

fn repeat_note(repeat: Repeat, date: &str) -> String {
    let day = ou::parse_date(date);
    let weekday = day.map(|d| d.format("%A").to_string()).unwrap_or_default();
    match repeat {
        Repeat::None => String::new(),
        Repeat::Weekly => format!("Repeats every week on {weekday}."),
        Repeat::Biweekly => format!("Repeats every other week on {weekday}."),
        Repeat::Twice => "Pick the two days. It starts on the first of them on or after the date.".into(),
    }
}

fn set_days(models: &Models, days: &[u8]) {
    for i in 0..7 {
        models.out_days.set_row_data(i, days.contains(&(i as u8)));
    }
}

fn read_days(models: &Models) -> Vec<u8> {
    (0..7).filter(|&i| models.out_days.row_data(i).unwrap_or(false)).map(|i| i as u8).collect()
}

fn open_event_popup(ui: &AppWindow, core: &CoreRc, models: &Models, event: Option<Event>, title: String, details: String, contact: Option<u64>) {
    let selected = core.borrow().outreach_ui.selected;
    let out = ui.global::<Outreach>();
    let e = event.unwrap_or(Event {
        id: 0,
        title,
        details,
        date: selected.format("%Y-%m-%d").to_string(),
        time: String::new(),
        repeat: Repeat::None,
        days: Vec::new(),
        contact_id: contact,
    });
    {
        let mut c = core.borrow_mut();
        c.outreach_ui.event_id = if e.id == 0 { None } else { Some(e.id) };
        c.outreach_ui.event_contact = e.contact_id;
    }
    out.set_e_id(if e.id == 0 { -1 } else { e.id as i32 });
    out.set_e_title(e.title.clone().into());
    out.set_e_date(e.date.clone().into());
    out.set_e_time(e.time.clone().into());
    out.set_e_details(e.details.clone().into());
    out.set_e_repeat(e.repeat.key().into());
    set_days(models, if e.repeat == Repeat::Twice { &e.days } else { &[] });
    out.set_e_note(repeat_note(e.repeat, &e.date).into());
    out.set_e_error("".into());
    out.set_event_open(true);
}

fn fill_card(ui: &AppWindow, core: &CoreRc, contact: Option<&Contact>) {
    let out = ui.global::<Outreach>();
    let c = contact.cloned().unwrap_or_default();
    {
        let mut core = core.borrow_mut();
        core.outreach_ui.pending_photo = None;
        core.outreach_ui.photo_cleared = false;
        core.outreach_ui.card_id = contact.map(|c| c.id);
    }
    let image = contact.and_then(|c| core.borrow().outreach.photo_file(c)).and_then(|p| fs::read(p).ok()).and_then(|b| image_from_jpeg(&b));
    out.set_c_id(contact.map(|c| c.id as i32).unwrap_or(-1));
    out.set_c_name(c.name.clone().into());
    out.set_c_phone(c.phone.clone().into());
    out.set_c_email(c.email.clone().into());
    out.set_c_prayer(c.prayer_request.clone().into());
    out.set_c_fulfilled(c.fulfilled);
    out.set_c_pool(c.in_pool);
    out.set_c_initials(ou::initials(&c.name).into());
    out.set_c_has_photo(image.is_some());
    out.set_c_photo(image.unwrap_or_default());
    out.set_c_error("".into());
    out.set_card_open(true);
}

pub fn setup(ui: &AppWindow, core: &CoreRc, models: &Models) {
    let out = ui.global::<Outreach>();
    out.set_contacts(ModelRc::from(models.out_contacts.clone()));
    out.set_pool_list(ModelRc::from(models.out_pool.clone()));
    out.set_cells(ModelRc::from(models.out_cells.clone()));
    out.set_day_events(ModelRc::from(models.out_day.clone()));
    out.set_upcoming(ModelRc::from(models.out_upcoming.clone()));
    out.set_e_days(ModelRc::from(models.out_days.clone()));
    refresh(ui, core, models);
    let weak = ui.as_weak();

    macro_rules! hook {
        ($name:ident, |$ui:ident, $core:ident, $models:ident $(, $arg:ident)*| $body:block) => {{
            let core = core.clone();
            let models = models.clone();
            let weak = weak.clone();
            ui.global::<Outreach>().$name(move |$($arg),*| {
                let Some($ui) = weak.upgrade() else { return };
                let $core = &core;
                let $models = &models;
                $body
            });
        }};
    }

    hook!(on_open, |ui, core, models| {
        ui.global::<Session>().set_view("outreach".into());
        ui.global::<Outreach>().set_tab("contacts".into());
        refresh(&ui, core, models);
    });
    hook!(on_prev_month, |ui, core, models| {
        {
            let mut c = core.borrow_mut();
            let (y, m) = ou::shift_month(c.outreach_ui.year, c.outreach_ui.month, -1);
            c.outreach_ui.year = y;
            c.outreach_ui.month = m;
        }
        publish(&ui, &core.borrow(), models);
    });
    hook!(on_next_month, |ui, core, models| {
        {
            let mut c = core.borrow_mut();
            let (y, m) = ou::shift_month(c.outreach_ui.year, c.outreach_ui.month, 1);
            c.outreach_ui.year = y;
            c.outreach_ui.month = m;
        }
        publish(&ui, &core.borrow(), models);
    });
    hook!(on_this_month, |ui, core, models| {
        {
            let mut c = core.borrow_mut();
            let today = Local::now().date_naive();
            c.outreach_ui.today = today;
            c.outreach_ui.year = today.year();
            c.outreach_ui.month = today.month();
            c.outreach_ui.selected = today;
        }
        publish(&ui, &core.borrow(), models);
    });
    hook!(on_select_day, |ui, core, models, index| {
        {
            let mut c = core.borrow_mut();
            let date = first_cell(c.outreach_ui.year, c.outreach_ui.month) + Duration::days(index as i64);
            c.outreach_ui.selected = date;
            c.outreach_ui.year = date.year();
            c.outreach_ui.month = date.month();
        }
        publish(&ui, &core.borrow(), models);
    });
    hook!(on_add_contact, |ui, core, _models| {
        fill_card(&ui, core, None);
    });
    hook!(on_edit_contact, |ui, core, _models, id| {
        let contact = find_contact(core, id);
        if contact.is_some() {
            fill_card(&ui, core, contact.as_ref());
        }
    });
    hook!(on_pick_photo, |ui, core, _models| {
        let out = ui.global::<Outreach>();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a picture")
            .add_filter("Pictures", &["jpg", "jpeg", "png", "webp", "gif", "bmp"])
            .pick_file()
        else {
            return;
        };
        let result = fs::metadata(&path)
            .map_err(|e| e.to_string())
            .and_then(|m| if m.len() > 40_000_000 { Err("That picture is too large (over 40 MB).".to_string()) } else { Ok(()) })
            .and_then(|_| fs::read(&path).map_err(|e| e.to_string()))
            .and_then(|bytes| ou::make_thumbnail(&bytes));
        match result {
            Ok(jpeg) => {
                if let Some(image) = image_from_jpeg(&jpeg) {
                    out.set_c_photo(image);
                    out.set_c_has_photo(true);
                    out.set_c_error("".into());
                    let mut c = core.borrow_mut();
                    c.outreach_ui.pending_photo = Some(jpeg);
                    c.outreach_ui.photo_cleared = false;
                }
            }
            Err(err) => out.set_c_error(err.into()),
        }
    });
    hook!(on_clear_photo, |ui, core, _models| {
        let out = ui.global::<Outreach>();
        out.set_c_has_photo(false);
        let mut c = core.borrow_mut();
        c.outreach_ui.pending_photo = None;
        c.outreach_ui.photo_cleared = true;
    });
    hook!(on_save_contact, |ui, core, models| {
        let out = ui.global::<Outreach>();
        let mut contact = Contact {
            id: 0,
            name: out.get_c_name().to_string(),
            phone: out.get_c_phone().to_string(),
            email: out.get_c_email().to_string(),
            prayer_request: out.get_c_prayer().to_string(),
            in_pool: out.get_c_pool(),
            fulfilled: out.get_c_fulfilled(),
            photo: None,
        };
        contact.sanitize();
        if contact.name.is_empty() {
            out.set_c_error("Give this person a name.".into());
            return;
        }
        if !contact.email.is_empty() && !ou::valid_email(&contact.email) {
            out.set_c_error("That email address does not look right. Leave it blank if there is none.".into());
            return;
        }
        let result = {
            let mut c = core.borrow_mut();
            let editing = c.outreach_ui.card_id;
            let pending = c.outreach_ui.pending_photo.take();
            let cleared = c.outreach_ui.photo_cleared;
            let id = editing.unwrap_or_else(|| c.outreach.next_contact_id());
            contact.id = id;
            let existing = c.outreach.contact(id).cloned();
            contact.photo = existing.as_ref().and_then(|e| e.photo.clone());
            let mut result = Ok(());
            if let Some(jpeg) = pending {
                match c.outreach.write_photo(id, &jpeg) {
                    Ok(name) => contact.photo = Some(name),
                    Err(err) => result = Err(err),
                }
            } else if cleared {
                if let Some(e) = &existing {
                    c.outreach.remove_photo_file(e);
                }
                contact.photo = None;
            }
            match c.outreach.contact_mut(id) {
                Some(slot) => *slot = contact.clone(),
                None => c.outreach.contacts.push(contact.clone()),
            }
            result.and_then(|_| c.outreach.save_contacts())
        };
        out.set_card_open(false);
        save_err(&ui, core, "the contact", result);
        refresh(&ui, core, models);
        show_status(&ui, &mut core.borrow_mut(), format!("Saved {}.", contact.name));
    });
    hook!(on_ask_delete_contact, |ui, core, _models| {
        let out = ui.global::<Outreach>();
        let id = out.get_c_id();
        if id < 0 {
            out.set_card_open(false);
            return;
        }
        core.borrow_mut().outreach_ui.confirm = Some(Confirm::Contact(id as u64));
        out.set_confirm_text(format!("Delete {} from your contacts? Their photo and their place in the prayer rotation go too. This cannot be undone.", out.get_c_name()).into());
        out.set_confirm_open(true);
    });
    hook!(on_copy_contact, |ui, core, _models, id| {
        let Some(contact) = find_contact(core, id) else { return };
        let message = match crate::copy_to_clipboard(&ou::contact_summary(&contact)) {
            Ok(()) => format!("Copied {}'s info to the clipboard.", contact.name),
            Err(err) => err,
        };
        show_status(&ui, &mut core.borrow_mut(), message);
    });
    hook!(on_email_contact, |ui, core, _models, id| {
        let Some(contact) = find_contact(core, id) else { return };
        let message = match ou::mailto_url(&contact.email) {
            Some(url) => match open_system(&url) {
                Ok(()) => format!("Opening your mail program for {}.", contact.name),
                Err(err) => err,
            },
            None => "That email address does not look right.".to_string(),
        };
        show_status(&ui, &mut core.borrow_mut(), message);
    });
    hook!(on_toggle_pool, |ui, core, models, id| {
        let message = {
            let mut c = core.borrow_mut();
            let Some(contact) = c.outreach.contact_mut(id as u64) else { return };
            let message = if contact.fulfilled {
                // Praying again for someone whose request was marked fulfilled.
                contact.fulfilled = false;
                contact.in_pool = true;
                format!("Praying again for {}. Back in the daily rotation.", contact.name)
            } else {
                contact.in_pool = !contact.in_pool;
                if contact.in_pool {
                    format!("{} is in the daily prayer rotation.", contact.name)
                } else {
                    format!("{} is out of the daily prayer rotation.", contact.name)
                }
            };
            let result = c.outreach.save_contacts();
            drop(c);
            save_err(&ui, core, "the contact", result);
            message
        };
        refresh(&ui, core, models);
        show_status(&ui, &mut core.borrow_mut(), message);
    });
    hook!(on_add_event_for, |ui, core, models, id| {
        let Some(contact) = find_contact(core, id) else { return };
        open_event_popup(&ui, core, models, None, format!("Outreach: {}", contact.name), format!("Contact: {}", contact.name), Some(contact.id));
    });
    hook!(on_add_event_day, |ui, core, models| {
        open_event_popup(&ui, core, models, None, String::new(), String::new(), None);
    });
    hook!(on_edit_event, |ui, core, models, id| {
        let event = core.borrow().outreach.events.iter().find(|e| e.id == id as u64).cloned();
        if event.is_some() {
            open_event_popup(&ui, core, models, event, String::new(), String::new(), None);
        }
    });
    hook!(on_set_repeat, |ui, core, models, key| {
        let _ = core;
        let out = ui.global::<Outreach>();
        let repeat = Repeat::parse(&key);
        out.set_e_repeat(repeat.key().into());
        if repeat == Repeat::Twice && read_days(models).is_empty() {
            if let Some(d) = ou::parse_date(&out.get_e_date()) {
                set_days(models, &[d.weekday().num_days_from_monday() as u8]);
            }
        }
        out.set_e_note(repeat_note(repeat, &out.get_e_date()).into());
    });
    hook!(on_toggle_day, |ui, core, models, index| {
        let _ = (&ui, core);
        let current = models.out_days.row_data(index as usize).unwrap_or(false);
        models.out_days.set_row_data(index as usize, !current);
    });
    hook!(on_step_date, |ui, core, models, delta| {
        let _ = models;
        let out = ui.global::<Outreach>();
        let base = ou::parse_date(&out.get_e_date()).unwrap_or(core.borrow().outreach_ui.selected);
        let next = base + Duration::days(delta as i64);
        out.set_e_date(next.format("%Y-%m-%d").to_string().into());
        out.set_e_note(repeat_note(Repeat::parse(&out.get_e_repeat()), &out.get_e_date()).into());
    });
    hook!(on_step_time, |ui, core, models, delta| {
        let _ = (core, models);
        let out = ui.global::<Outreach>();
        let current = ou::parse_time(&out.get_e_time()).unwrap_or_default();
        let current = if current.is_empty() { "09:00".to_string() } else { current };
        let (h, m) = current.split_once(':').map(|(h, m)| (h.parse::<i32>().unwrap_or(9), m.parse::<i32>().unwrap_or(0))).unwrap_or((9, 0));
        let total = (h * 60 + m + delta).rem_euclid(24 * 60);
        out.set_e_time(format!("{:02}:{:02}", total / 60, total % 60).into());
    });
    hook!(on_save_event, |ui, core, models| {
        let out = ui.global::<Outreach>();
        let title = out.get_e_title().trim().to_string();
        if title.is_empty() {
            out.set_e_error("Give the event a title.".into());
            return;
        }
        let Some(date) = ou::parse_date(&out.get_e_date()) else {
            out.set_e_error("The date looks like 2026-10-24.".into());
            return;
        };
        let time = match ou::parse_time(&out.get_e_time()) {
            Ok(t) => t,
            Err(err) => {
                out.set_e_error(err.into());
                return;
            }
        };
        let repeat = Repeat::parse(&out.get_e_repeat());
        let days = read_days(models);
        if repeat == Repeat::Twice && days.len() != 2 {
            out.set_e_error("Choose exactly two days of the week.".into());
            return;
        }
        let result = {
            let mut c = core.borrow_mut();
            let id = c.outreach_ui.event_id.unwrap_or_else(|| c.outreach.next_event_id());
            let mut event = Event {
                id,
                title,
                details: out.get_e_details().to_string(),
                date: date.format("%Y-%m-%d").to_string(),
                time,
                repeat,
                days,
                contact_id: c.outreach_ui.event_contact,
            };
            event.sanitize();
            match c.outreach.events.iter_mut().find(|e| e.id == id) {
                Some(slot) => *slot = event,
                None => c.outreach.events.push(event),
            }
            c.outreach_ui.selected = date;
            c.outreach_ui.year = date.year();
            c.outreach_ui.month = date.month();
            c.outreach.save_events()
        };
        out.set_event_open(false);
        save_err(&ui, core, "the event", result);
        publish(&ui, &core.borrow(), models);
        show_status(&ui, &mut core.borrow_mut(), "Event saved.");
    });
    hook!(on_ask_delete_event, |ui, core, _models| {
        let out = ui.global::<Outreach>();
        let id = out.get_e_id();
        if id < 0 {
            out.set_event_open(false);
            return;
        }
        core.borrow_mut().outreach_ui.confirm = Some(Confirm::Event(id as u64));
        let repeating = Repeat::parse(&out.get_e_repeat()) != Repeat::None;
        let extra = if repeating { " This repeats, so every occurrence goes." } else { "" };
        out.set_confirm_text(format!("Delete \u{201c}{}\u{201d}?{extra} This cannot be undone.", out.get_e_title()).into());
        out.set_confirm_open(true);
    });
    hook!(on_confirm_yes, |ui, core, models| {
        let out = ui.global::<Outreach>();
        let pending = core.borrow_mut().outreach_ui.confirm.take();
        let mut message = String::new();
        let result = {
            let mut c = core.borrow_mut();
            match pending {
                Some(Confirm::Contact(id)) => {
                    let name = c.outreach.contact(id).map(|c| c.name.clone()).unwrap_or_default();
                    c.outreach.delete_contact(id);
                    message = format!("Deleted {name}.");
                    let a = c.outreach.save_contacts();
                    a.and_then(|_| c.outreach.save_events())
                }
                Some(Confirm::Event(id)) => {
                    c.outreach.events.retain(|e| e.id != id);
                    message = "Event deleted.".into();
                    c.outreach.save_events()
                }
                None => Ok(()),
            }
        };
        out.set_confirm_open(false);
        out.set_card_open(false);
        out.set_event_open(false);
        save_err(&ui, core, "that change", result);
        refresh(&ui, core, models);
        if !message.is_empty() {
            show_status(&ui, &mut core.borrow_mut(), message);
        }
    });
}
