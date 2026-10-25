//! Picture of chosen verses on a solid background color.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage};

const BODY_FONT: &[u8] = include_bytes!("../fonts/Rajdhani-Medium.ttf");
const MARK_FONT: &[u8] = include_bytes!("../fonts/ShareTechMono-Regular.ttf");
const CARD_WIDTH: u32 = 1080;
const PAD: f32 = 84.0;

pub struct CardImage {
    pub png: Vec<u8>,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn parse_hex(raw: &str) -> Option<[u8; 3]> {
    let trimmed = raw.trim();
    let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let expanded = if digits.len() == 3 {
        let chars: Vec<char> = digits.chars().collect();
        format!(
            "{}{}{}{}{}{}",
            chars[0], chars[0], chars[1], chars[1], chars[2], chars[2]
        )
    } else {
        digits.to_string()
    };
    if expanded.len() != 6 || !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&expanded[0..2], 16).ok()?;
    let g = u8::from_str_radix(&expanded[2..4], 16).ok()?;
    let b = u8::from_str_radix(&expanded[4..6], 16).ok()?;
    Some([r, g, b])
}

pub fn hex_string(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

pub fn ink_for(bg: [u8; 3]) -> [u8; 3] {
    let linear = |channel: u8| {
        let unit = channel as f32 / 255.0;
        if unit <= 0.04045 {
            unit / 12.92
        } else {
            ((unit + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(bg[0]) + 0.7152 * linear(bg[1]) + 0.0722 * linear(bg[2]);
    if luminance > 0.45 {
        [0x1a, 0x16, 0x12]
    } else {
        [0xf7, 0xf4, 0xee]
    }
}

pub fn file_stem(reference: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in reference.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "twm-verse.png".into()
    } else {
        format!("twm-{slug}.png")
    }
}

/// Wrap each verse, with a blank line between verses.
pub fn verse_lines(verses: &[String], size: f32, max_width: f32) -> Vec<String> {
    let Ok(font) = FontRef::try_from_slice(BODY_FONT) else {
        return verses.to_vec();
    };
    let mut lines = Vec::new();
    for (index, verse) in verses.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.extend(wrap_line(&font, size, verse, max_width));
    }
    lines
}

fn wrap_line(font: &FontRef<'_>, size: f32, text: &str, max_width: f32) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in words {
        let trial = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if measure(font, size, &trial) <= max_width || line.is_empty() {
            if measure(font, size, word) > max_width && line.is_empty() {
                lines.extend(break_word(font, size, word, max_width));
                line.clear();
            } else {
                line = trial;
            }
        } else {
            lines.push(line);
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn break_word(font: &FontRef<'_>, size: f32, word: &str, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for ch in word.chars() {
        let trial = format!("{line}{ch}");
        if !line.is_empty() && measure(font, size, &trial) > max_width {
            lines.push(line);
            line = ch.to_string();
        } else {
            line = trial;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn measure(font: &FontRef<'_>, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars()
        .map(|ch| scaled.h_advance(font.glyph_id(ch)))
        .sum()
}

pub fn render(verses: &[String], reference: &str, background: [u8; 3]) -> Result<CardImage, String> {
    if verses.is_empty() {
        return Err("Choose at least one verse.".into());
    }
    let body = FontRef::try_from_slice(BODY_FONT).map_err(|_| "The share font could not be read.".to_string())?;
    let mark = FontRef::try_from_slice(MARK_FONT).map_err(|_| "The share font could not be read.".to_string())?;
    let max_width = CARD_WIDTH as f32 - PAD * 2.0;
    let mut size = if verses.len() > 8 {
        34.0
    } else if verses.len() > 3 {
        44.0
    } else {
        52.0
    };
    let mut lines = verse_lines(verses, size, max_width);
    while lines.len() as f32 * size * 1.38 > 1240.0 && size > 28.0 {
        size -= 2.0;
        lines = verse_lines(verses, size, max_width);
    }
    let line_height = size * 1.38;
    let block = lines.len() as f32 * line_height;
    let height = (block + 280.0).clamp(1080.0, 4096.0).round() as u32;
    let ink = ink_for(background);
    let mut image = RgbaImage::from_pixel(
        CARD_WIDTH,
        height,
        Rgba([background[0], background[1], background[2], 255]),
    );
    let top = ((height as f32 - 160.0 - block) / 2.0).max(PAD);
    draw_lines(&mut image, &body, size, line_height, &lines, PAD, top, ink);
    draw_line(
        &mut image,
        &mark,
        26.0,
        reference,
        PAD,
        height as f32 - 112.0,
        ink,
    );
    draw_line(
        &mut image,
        &mark,
        22.0,
        "TWM Bible",
        PAD,
        height as f32 - 68.0,
        ink,
    );
    let rgba = image.as_raw().clone();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(image.as_raw(), CARD_WIDTH, height, ExtendedColorType::Rgba8)
        .map_err(|err| err.to_string())?;
    Ok(CardImage {
        png,
        rgba,
        width: CARD_WIDTH,
        height,
    })
}

fn draw_lines(
    image: &mut RgbaImage,
    font: &FontRef<'_>,
    size: f32,
    line_height: f32,
    lines: &[String],
    x: f32,
    top: f32,
    ink: [u8; 3],
) {
    let scaled = font.as_scaled(PxScale::from(size));
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let baseline = top + index as f32 * line_height + scaled.ascent();
        draw_line(image, font, size, line, x, baseline, ink);
    }
}

fn draw_line(
    image: &mut RgbaImage,
    font: &FontRef<'_>,
    size: f32,
    text: &str,
    mut x: f32,
    baseline: f32,
    ink: [u8; 3],
) {
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                blend(
                    image,
                    bounds.min.x as i32 + gx as i32,
                    bounds.min.y as i32 + gy as i32,
                    coverage,
                    ink,
                );
            });
        }
        x += scaled.h_advance(id);
    }
}

fn blend(image: &mut RgbaImage, x: i32, y: i32, coverage: f32, ink: [u8; 3]) {
    if x < 0 || y < 0 {
        return;
    }
    let (width, height) = image.dimensions();
    if x as u32 >= width || y as u32 >= height {
        return;
    }
    let coverage = coverage.clamp(0.0, 1.0);
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    for channel in 0..3 {
        let source = ink[channel] as f32;
        let destination = pixel.0[channel] as f32;
        pixel.0[channel] = (source * coverage + destination * (1.0 - coverage)).round() as u8;
    }
    pixel.0[3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_accepts_short_and_long_forms() {
        assert_eq!(parse_hex("#1d3"), Some([0x11, 0xdd, 0x33]));
        assert_eq!(parse_hex("  #1D3A2E "), Some([0x1d, 0x3a, 0x2e]));
        assert_eq!(parse_hex("fbf7ef"), Some([0xfb, 0xf7, 0xef]));
        assert_eq!(parse_hex("red"), None);
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(hex_string([0x1d, 0x3a, 0x2e]), "#1d3a2e");
    }

    #[test]
    fn ink_follows_the_background() {
        assert_eq!(ink_for([0x12, 0x14, 0x1a]), [0xf7, 0xf4, 0xee]);
        assert_eq!(ink_for([0xfb, 0xf7, 0xef]), [0x1a, 0x16, 0x12]);
    }

    #[test]
    fn file_name_uses_the_reference() {
        assert_eq!(file_stem("John 3:16 (KJV 1611)"), "twm-john-3-16-kjv-1611.png");
    }

    #[test]
    fn short_verse_stays_on_one_line() {
        let lines = verse_lines(&["16 For God so loved".into()], 48.0, 900.0);
        assert_eq!(lines, vec!["16 For God so loved".to_string()]);
    }

    #[test]
    fn long_line_wraps_without_dropping_words() {
        let lines = verse_lines(
            &["alpha beta gamma delta epsilon zeta eta theta iota kappa".into()],
            48.0,
            180.0,
        );
        assert!(lines.len() > 1, "{lines:?}");
        let joined = lines.join(" ");
        assert!(joined.contains("alpha"));
        assert!(joined.contains("kappa"));
    }

    #[test]
    fn picture_is_a_png_of_the_chosen_color() {
        let card = render(
            &["16 For God so loved the world, that he gave his only begotten Son.".into()],
            "John 3:16 (KJV 1611)",
            [0x1d, 0x3a, 0x2e],
        )
        .unwrap();
        assert_eq!(&card.png[..8], &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
        assert_eq!(card.width, 1080);
        assert!(card.height >= 1080);
        let image = image::load_from_memory(&card.png).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(4, 4).0, [0x1d, 0x3a, 0x2e, 255]);
        assert_eq!(card.rgba.len(), (card.width * card.height * 4) as usize);
    }
}
