fn main() {
    let paths = [
        "ui/app.slint",
        "fonts/Rajdhani-Medium.ttf",
        "fonts/Rajdhani-SemiBold.ttf",
        "fonts/Rajdhani-Bold.ttf",
        "fonts/ShareTechMono-Regular.ttf",
        "fonts/Orbitron-Bold.ttf",
        "assets/icon-256.png",
        "packaging/windows/icon.ico",
    ];
    for path in paths {
        println!("cargo:rerun-if-changed={path}");
    }
    slint_build::compile("ui/app.slint").unwrap();

    // Embed the icon and version info into the Windows executable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_icon();
    }
}

#[cfg(windows)]
fn embed_windows_icon() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("packaging/windows/icon.ico");
    res.set("ProductName", "TWM Bible");
    res.set("FileDescription", "TWM Bible native reader");
    res.compile().expect("failed to embed Windows resources");
}

#[cfg(not(windows))]
fn embed_windows_icon() {
    // Cross-compiling from Linux: winresource is only a dependency on Windows
    // hosts, so the icon is embedded by CI builds on windows runners.
}
