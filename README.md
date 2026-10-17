# TWM Bible

Native desktop reader for the TWM Bible texts. It is a Rust and Slint application. The Chromium edition, TWM Bible App, stays installed beside it and keeps its own notes.

## What's new

See [CHANGELOG.md](CHANGELOG.md). Version 1.2.0 adds Outreach (a planner, contacts with photos, and a daily prayer card), a loading animation, and popup and layout fixes. Version 1.1.0 added side-by-side texts, bookmarks, backup and restore, PDF/HTML export, new themes, an intro video, and keyboard shortcuts.

## Install

Download the file for your system from the GitHub Releases page.

- **Debian / Ubuntu:** `sudo apt install ./twm-bible-native_<version>-1_amd64.deb`, then open **TWM Bible** from the app menu.
- **Other Linux:** `chmod +x TWM_Bible-<version>-x86_64.AppImage` and run it. Or unpack the `linux-x86_64.tar.gz` and run `bin/twm-bible-native`.
- **Windows:** run `TWM-Bible-<version>-windows-x64-setup.exe`. It adds a Start Menu and desktop shortcut and an uninstaller. The installer is unsigned, so SmartScreen may ask you to confirm (More info → Run anyway). A portable zip is also provided.
- **macOS (11+, Apple Silicon and Intel):** open the `.dmg` and drag **TWM Bible** to Applications. The app is not notarized, so the first time right-click it and choose **Open** (or run `xattr -dr com.apple.quarantine "/Applications/TWM Bible.app"`).

## Building release packages

- Linux: `./packaging/linux/build-linux.sh` (needs `cargo install cargo-deb`) → `dist/`
- Windows: `cargo build --release`, then `makensis packaging\windows\installer.nsi`
- macOS: `./packaging/macos/build-dmg.sh`

Pushing a tag such as `v1.2.0` runs `.github/workflows/release.yml`, which builds all three and attaches them to a GitHub Release.

## Run from source

```bash
./bin/twm-bible
```

The launcher builds the release binary when the sources are newer, then starts it. Rust is installed with rustup. The menu entry is **TWM Bible**.

## Reading

Home, Read, Search, Outreach, and Options. The default text is the KJV 1611 in standard spelling. ASV, WEB, Young’s Literal, Darby, Revised Version, Geneva 1599, and Douay-Rheims are bundled and start turned off. KJV stays on.

A reference such as `John 3:16` opens that verse. Keyword search needs at least two letters and lists up to 500 verses. Click a verse to select it. Click its number to highlight it. A second click on the number does not clear the highlight. Remove does.

Notes can hold text and verse links such as `Romans 8:28, Psalm 23:1`. The note is saved when you leave the verse, change tabs, or close the window. An empty note is removed.

Word chips under the selected verse open the KJV glossary. A modern definition is fetched once from dictionaryapi.dev, then Datamuse, and kept on this computer.

## Outreach

The OUTREACH view holds a planner (month calendar with repeating events) and your contacts, each with an optional photo, copy and email buttons, a prayer toggle and a fulfilled toggle. The Home page shows a daily prayer card chosen from contacts in the prayer pool. This data stays on your computer in `contacts.json`, `events.json` and `photos/` beside your notes. Backups that include contacts and events contain personal information, so keep them private. The email button opens your mail program with a `mailto:` link.

## Appearance

Dark and light use the field-journal palette: dark `#05080e` / `#3df0ff`, light `#e7f3f6` / `#087888`. System follows the desktop color scheme (`org.freedesktop.appearance` color-scheme, then gsettings). No preference uses dark. Text size runs from 15 to 40 and starts at 21.

## Data

Notes, highlights, and imported Bibles are stored in `~/.local/state/twm-bible-slint/` on Linux, `%LOCALAPPDATA%\twm-bible-slint` on Windows, and `~/Library/Application Support/twm-bible-slint` on macOS. That store is separate from the browser app’s localStorage. An import copies a JSON file into `imports/` and leaves the original in place. Dropping an import deletes only that copy.

The bundled JSON files in `data/` are copies of the public-domain texts from `twm-bible 1.0.0`. The app looks for them in `$TWM_BIBLE_DATA`, then `data/` beside or above the executable, `../share/twm-bible-native/data` (Linux packages), and `../Resources/data` (macOS bundle). The intro video `assets/twmintro.mp4` is found in the same places (`assets/` beside `data/`).

## License

The app code is MIT. The bundled Bible texts are public domain. Rajdhani, Share Tech Mono, and Orbitron are under the SIL Open Font License (`fonts/OFL.txt`).
