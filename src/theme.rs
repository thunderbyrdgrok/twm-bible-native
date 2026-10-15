//! Desktop light/dark detection.
//!
//! The freedesktop color-scheme value is 0 (no preference), 1 (prefer dark),
//! or 2 (prefer light). No preference resolves to dark, which is the field
//! journal default.

use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Dark,
    Light,
    NoPreference,
}

pub fn parse_portal_output(text: &str) -> Option<Scheme> {
    let number = text
        .split_whitespace()
        .rev()
        .find_map(|word| word.parse::<u32>().ok())?;
    Some(match number {
        1 => Scheme::Dark,
        2 => Scheme::Light,
        _ => Scheme::NoPreference,
    })
}

pub fn parse_gsettings(text: &str) -> Option<Scheme> {
    let value = text.trim().trim_matches('\'').trim();
    Some(match value {
        "prefer-dark" => Scheme::Dark,
        "prefer-light" => Scheme::Light,
        "default" => Scheme::NoPreference,
        _ => return None,
    })
}

pub const PALETTES: &[&str] = &["sepia", "amber", "forest", "contrast"];

/// Palette name the UI uses for its colours. Dark, light, and system use the default one.
pub fn palette_of(mode: &str) -> &'static str {
    PALETTES.iter().copied().find(|name| *name == mode).unwrap_or("default")
}

pub fn resolved_light(mode: &str, system: Scheme) -> bool {
    match mode {
        "light" | "sepia" => true,
        "amber" | "forest" | "contrast" => false,
        "dark" => false,
        _ => matches!(system, Scheme::Light),
    }
}

pub fn theme_label(mode: &str, light: bool) -> String {
    match mode {
        "light" => "LIGHT".to_string(),
        "dark" => "DARK".to_string(),
        name if PALETTES.contains(&name) => name.to_uppercase(),
        _ => format!("SYSTEM · {}", if light { "LIGHT" } else { "DARK" }),
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() && output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn portal_scheme() -> Option<Scheme> {
    let text = command_output(
        "busctl",
        &[
            "--user",
            "call",
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            "ss",
            "org.freedesktop.appearance",
            "color-scheme",
        ],
    )?;
    parse_portal_output(&text)
}

pub fn gsettings_scheme() -> Option<Scheme> {
    let text = command_output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
    )?;
    parse_gsettings(&text)
}

/// Prefer the portal, then gsettings. A missing answer is dark.
pub fn system_scheme() -> Scheme {
    if let Some(scheme) = portal_scheme() {
        if scheme != Scheme::NoPreference {
            return scheme;
        }
    }
    if let Some(scheme) = gsettings_scheme() {
        if scheme != Scheme::NoPreference {
            return scheme;
        }
    }
    Scheme::Dark
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_values() {
        assert_eq!(parse_portal_output("v u 1"), Some(Scheme::Dark));
        assert_eq!(parse_portal_output("u 2\n"), Some(Scheme::Light));
        assert_eq!(parse_portal_output("v u 0"), Some(Scheme::NoPreference));
        assert_eq!(parse_portal_output("nope"), None);
    }

    #[test]
    fn gsettings_values() {
        assert_eq!(parse_gsettings("'prefer-dark'\n"), Some(Scheme::Dark));
        assert_eq!(parse_gsettings("prefer-light"), Some(Scheme::Light));
        assert_eq!(parse_gsettings("'default'"), Some(Scheme::NoPreference));
    }

    #[test]
    fn resolution() {
        assert!(resolved_light("light", Scheme::Dark));
        assert!(!resolved_light("dark", Scheme::Light));
        assert!(resolved_light("system", Scheme::Light));
        assert!(!resolved_light("system", Scheme::Dark));
        assert!(!resolved_light("system", Scheme::NoPreference));
        assert!(!resolved_light("desktop", Scheme::NoPreference));
        assert_eq!(theme_label("system", false), "SYSTEM · DARK");
        assert_eq!(theme_label("light", true), "LIGHT");
    }

    #[test]
    fn palettes() {
        assert_eq!(palette_of("sepia"), "sepia");
        assert_eq!(palette_of("dark"), "default");
        assert!(resolved_light("sepia", Scheme::Dark));
        assert!(!resolved_light("forest", Scheme::Light));
        assert_eq!(theme_label("contrast", false), "CONTRAST");
    }
}
