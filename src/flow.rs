//! Word wrapping and word hit-testing for the verse text.
//! The verse is drawn as one Text with explicit line breaks chosen here, so a press can be
//! mapped back to a word using the same font widths.

use std::sync::OnceLock;

use ttf_parser::Face;

static FONT: &[u8] = include_bytes!("../fonts/Rajdhani-Medium.ttf");

fn face() -> Option<&'static Face<'static>> {
    static FACE: OnceLock<Option<Face<'static>>> = OnceLock::new();
    FACE.get_or_init(|| Face::parse(FONT, 0).ok()).as_ref()
}

fn advance(c: char, size: f32) -> f32 {
    let Some(face) = face() else { return size * 0.5 };
    let units = face.units_per_em() as f32;
    face.glyph_index(c)
        .and_then(|glyph| face.glyph_hor_advance(glyph))
        .map(|adv| adv as f32 * size / units)
        .unwrap_or(size * 0.5)
}

fn width_of(text: &str, size: f32) -> f32 {
    text.chars().map(|c| advance(c, size)).sum()
}

/// Greedy wrap into lines of whitespace-separated chunks.
pub fn wrap(text: &str, width: f32, size: f32) -> Vec<Vec<String>> {
    let space = advance(' ', size);
    // A little slack, so the drawn text never runs past the right margin.
    let limit = width * 0.97;
    let mut lines: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut used = 0.0f32;
    for chunk in text.split_whitespace() {
        let w = width_of(chunk, size);
        if !current.is_empty() && used + space + w > limit {
            lines.push(std::mem::take(&mut current));
            used = 0.0;
        }
        if !current.is_empty() {
            used += space;
        }
        used += w;
        current.push(chunk.to_string());
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub fn join(lines: &[Vec<String>]) -> String {
    lines.iter().map(|line| line.join(" ")).collect::<Vec<_>>().join("\n")
}

/// Index of the chunk (counted across all lines) under a point.
pub fn hit(lines: &[Vec<String>], line: f32, x: f32, size: f32) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let row = (line.floor().max(0.0) as usize).min(lines.len() - 1);
    let before: usize = lines[..row].iter().map(Vec::len).sum();
    let space = advance(' ', size);
    let mut at = 0.0f32;
    for (i, chunk) in lines[row].iter().enumerate() {
        let end = at + width_of(chunk, size);
        if x < end + space / 2.0 {
            return Some(before + i);
        }
        at = end + space;
    }
    Some(before + lines[row].len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_hits() {
        let text = "In the beginning God created the heaven and the earth";
        let wide = wrap(text, 2000.0, 20.0);
        assert_eq!(wide.len(), 1);
        let narrow = wrap(text, 150.0, 20.0);
        assert!(narrow.len() > 2);
        assert_eq!(join(&narrow).replace('\n', " "), text);
        assert_eq!(hit(&narrow, 0.0, 1.0, 20.0), Some(0));
        let total: usize = narrow.iter().map(Vec::len).sum();
        assert_eq!(hit(&narrow, 99.0, 9999.0, 20.0), Some(total - 1));
    }
}
