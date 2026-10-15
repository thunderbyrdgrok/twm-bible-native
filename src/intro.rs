//! Intro video. The clip is played by the system's own player, so the app needs no codecs.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const FILE_NAME: &str = "twmintro.mp4";

pub fn find_in(roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join("assets").join(FILE_NAME))
        .find(|path| path.is_file())
}

/// Looks for assets/twmintro.mp4 beside the data folder, the project, or the executable.
pub fn find() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(data) = crate::store::data_dir() {
        if let Some(parent) = data.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Windows installer and portable zip: assets\ beside the exe.
            roots.push(dir.to_path_buf());
            // Linux packages and AppImage: <prefix>/bin -> <prefix>/share/twm-bible-native/assets
            roots.push(dir.join("../share/twm-bible-native"));
            // macOS app bundle: Contents/MacOS -> Contents/Resources/assets
            roots.push(dir.join("../Resources"));
            roots.push(dir.join(".."));
            roots.push(dir.join("../.."));
        }
    }
    roots.push(PathBuf::from("/usr/share/twm-bible-native"));
    find_in(&roots)
}

fn spawn(mut command: Command) -> bool {
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

/// Starts playback with audio. Returns false when no player could be started.
pub fn play(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        // The file-protocol handler opens the default player and copes with spaces in the path.
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(path);
        return spawn(command);
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);
        return spawn(command);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let mut mpv = Command::new("mpv");
        mpv.args(["--force-window=yes", "--really-quiet", "--title=TWM Bible intro"]).arg(path);
        if spawn(mpv) {
            return true;
        }
        let mut vlc = Command::new("vlc");
        vlc.args(["--play-and-exit", "--quiet"]).arg(path);
        if spawn(vlc) {
            return true;
        }
        let mut ffplay = Command::new("ffplay");
        ffplay.args(["-autoexit", "-loglevel", "quiet"]).arg(path);
        if spawn(ffplay) {
            return true;
        }
        let mut open = Command::new("xdg-open");
        open.arg(path);
        spawn(open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_none() {
        let dir = env::temp_dir().join("twm-intro-missing-test");
        assert!(find_in(&[dir]).is_none());
    }
}
