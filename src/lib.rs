pub mod bible;
pub mod define;
pub mod store;
pub mod theme;
pub mod intro;
pub mod flow;
pub mod extras;
pub mod export;

/// Keeps a child process from opening a console window on Windows. No effect elsewhere.
pub fn quiet(command: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}
