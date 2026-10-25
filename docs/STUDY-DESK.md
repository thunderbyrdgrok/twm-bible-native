# Study Desk (beta)

Home → **Study Desk** card (under LIBRARY) → **Sermon prep** or **Notes**.

## Sermon prep — editor design
Slint has no rich-text editor, so the sermon is a **list of blocks**. Each block is one of:
paragraph, heading 1–3, bulleted item, numbered item, quote, image, slide.
Every block has its own font (Rajdhani, Serif, Sans, Share Tech Mono, Orbitron), size and
alignment (left/center/right). Each text block is a real Slint `TextInput`, so typing,
selection, copy/paste and IME work like any text box.

* **Inline bold / italic / underline**: select text and press B / I / U (or Ctrl+B/I/U).
  The text stores light markup (`**bold**`, `*italic*`, `__underline__`). While a block has
  the caret you see the markup; when you leave it the block is rendered with Slint's
  `StyledText` (bold, italic, underline shown). Bundled Rajdhani has no italic face, so
  italic only shows in Serif/Sans.
* **Enter** makes a new block (lists continue; Enter on an empty list item ends the list),
  **Shift+Enter** is a line break inside a block, **Backspace** at the start of a heading/list/quote
  turns it into a paragraph, then merges with the block above. Up/Down at a block edge move between blocks.
* **Undo/Redo** (buttons, Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z): document-level history (200 steps).
  Typing in one block is grouped until a 1.5 s pause.
* **Images**: +IMAGE opens a file picker; the file is checked and copied to
  `<state dir>/studydesk/images/` (0600). Each image block has a caption.
* **Slides** = a numbered *slide marker block* (SLIDE 1, 2, …) with a title and an optional slide
  image. Use them to mark where the projected slide changes.
* Blocks can be moved (↑ ↓) or deleted (✕).
* **Autosave** 0.7 s after a change and when the window closes. Atomic write (temp file + fsync +
  rename), 0600 files in a 0700 folder: `<state dir>/studydesk/sermons/<id>.json`.
* Multiple sermons: list on the left (☰ folds it; it folds automatically below 980 px), + NEW,
  rename by editing the title, DELETE with a confirmation (removes the sermon's images too).
* **Export**: Web page (.html, images embedded, slides as framed sections, quotes as blockquotes)
  or Markdown (.md, images referenced by path).

## Scripture side panel
Right edge, full height, collapsible (» / the SCRIPTURE strip). Text chips for each enabled
translation (KJV default).
* **PASSAGE**: `John 3:16-18`, `Ps 23`, `Rom 5:8; 6:23`, `John 3:16,18`, `Gen 1:1-2:3`, `Jude 3`.
  The looked-up verses are selected automatically.
* **SEARCH**: word or phrase, case-insensitive, first 200 hits; click to select.
* Selected verses stack under SELECTED with the combined reference; **INSERT INTO SERMON** adds one
  quote block (verse numbers + text) with the reference line, e.g. `John 3:16-18 (KJV)`.

## Notes
List + editor (title and plain text body), autosaved to `<state dir>/studydesk/notes.json` (0600),
delete with confirmation.

## Install log
- 2026-10-20 22:00 CDT: installed to laptop ~/Work/twm-bible-dev-test/twm-bible-native, sha256 4c25b0eec129516f0bd16534255096047681b99882c94cb4b006cc346d98978a (built from 5792224). Previous beta backed up to ~/Work/backups/twm-bible-dev-test-native.20261020-2200.pre-studydesk.bak.
