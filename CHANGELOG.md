# Changelog

## Unreleased (beta) — Share

- Hold a word, then SHARE, to make a picture of the verses you check. Pick a background color from the swatches or type a hex color. Copy the picture, save a PNG, or copy the verse text.

## Unreleased (beta) — Study Desk

- Home: new Study Desk card under Library with Sermon prep and Notes.
- Sermon prep: block editor (headings, lists, quotes, images, slide markers, fonts, sizes, alignment, inline bold/italic/underline, undo/redo), autosave, multiple sermons, HTML/Markdown export.
- Scripture side panel: passage lookup and word search with insert-as-quote.
- Notes page.
- Scrolling: wheel scrolls the editor (also over text blocks), sermon/notes lists, note body and scripture lists; Up/Down/PageUp/PageDown/Home/End scroll the page when no field is edited (Esc leaves a field); Up/Down walk between blocks at the first/last line; the caret is kept in view while typing, also after the wheel scrolled the edited block away.
- Top bar: STUDY DESK button (between SEARCH and OUTREACH) with a Sermon prep / Notes dropdown; nav buttons compact below 1120 px wide (clock hidden) so it fits at 720x560.
- See docs/STUDY-DESK.md.

## 1.2.0

### Outreach
- New OUTREACH view (button on Home and in the top bar) with a planner and contacts.
- Planner: a month calendar with events, including repeating events (daily, weekly, monthly, yearly). Contacts can be added to the calendar from their row.
- Contacts, with tabs and rows: a 256x256 photo (chosen with a file picker, stored on this computer only), phone, email, address, notes and prayer request. Each row has copy, email (opens your mail program with a `mailto:` link) and prayer toggles. A contact card has a fulfilled toggle.
- Daily prayer card on Home: shows one contact from your prayer pool per day, rotating without repeats until everyone has had a turn.
- Contacts and events are stored on this computer (`contacts.json`, `events.json`, `photos/`), written atomically with owner-only permissions on Linux and macOS.
- Backup and restore can include contacts and events. A privacy warning appears when exporting, because the file then contains personal information.

### Loading animation
- A small robot animation shows in the word popup and in the word panel while a modern definition is being fetched.

### Fixes
- The word popup now opens just below the line you pressed (or above it when there is no room) instead of covering the word, and always stays inside the window.
- The robot in the popup header has room above it and is no longer clipped.
- The right-hand panel (word, note and definition) can no longer push past the window edge. The side columns shrink with the window, long word lists and definitions stay inside the panel, and the top bar simplifies in narrow windows.
- The minimum window size is now 720 x 560, so tiled windows are laid out rather than cut off.

## 1.1.1

### Fixes
- Windows: a console window no longer flashes open when a word definition is looked up. Every helper program the app starts (the definition fetch, the intro player launcher, clipboard and theme helpers) now starts without a console window.

## 1.1.0

### Reading
- Press and hold (about half a second) a word in the verse text or in the word list to open a popup with its archaic and modern definitions, Copy Verse, and Search This Word. A normal click still looks the word up.
- Looking up a word now lists the passages that contain it in the definition box (lower right). Each one is clickable.
- Highlights now colour the verse's letters with the chosen colour instead of a background block. The coloured bar on the left stays.
- The word list has a horizontal scrollbar, and the mouse wheel scrolls it sideways while the pointer is over it.
- Side-by-side versions: show a second text beside the main one (beneath it in a narrow window). Choose it with the 2ND button in Options or the SPLIT button in Read. Saved between runs.
- Back history: jumps made through links, search results, the passage box, or bookmarks can be undone with BACK (Alt+Left).
- Bookmarks: bookmark the selected verse (BOOKMARK button or Ctrl+B) and manage them in the new BOOKMARKS view.
- The chapter title in Read opens a book, chapter and verse popup. A new CHAPTER button opens a chapter picker for the open book.
- Home: the Continue card is split in two, with a "Find a verse" half that opens the same popup.
- Home: removed the privacy and spelling blurb for a cleaner layout.

### Search
- Quote a phrase for an exact match. Several unquoted words match verses that contain all of them.
- Filter by Old Testament, New Testament, or one book.
- Matched words are highlighted in the results.

### Notes and sharing
- Backup and restore: export notes, highlights and bookmarks to one JSON file, and restore from one (merge or replace). A copy of the current settings is saved before a restore.
- Export the current chapter or a verse range as PDF, a print-friendly web page, or text, optionally with your notes.

### Intro video and options
- A short intro video plays the first time the app runs. A play icon on the Home screen replays it, and an Options toggle ("Play intro video on startup") plays it on every launch. It opens in the system's video player.
- Four new themes: Sepia, Amber, Forest, and High Contrast.
- Verse spacing control and a distraction-free full-screen reading mode.
- Keyboard shortcuts: Ctrl+F search, Ctrl+G book/chapter/verse, Ctrl+B bookmark, Left/Right or PgUp/PgDn change chapter, F11 reading mode, Esc closes popups. Listed in Options.

### Packaging
- All installers include `assets/twmintro.mp4`.
- New dependency: `ttf-parser` (already in the build through Slint's font stack).

Existing notes, highlights, and settings carry over. New settings use defaults.

## 1.0.0
- First release of the native reader.
