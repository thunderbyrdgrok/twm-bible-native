# Changelog

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
