//! Text, print-friendly HTML, and PDF output for a passage. No dependencies: the PDF is a
//! small hand-written file that uses the built-in Helvetica font.

pub struct ExportVerse {
    pub number: usize,
    pub text: String,
    pub note: Option<String>,
}

/// Blank both: the whole chapter. Only "from": that one verse. Only "to": verse 1 to there.
pub fn parse_range(from: &str, to: &str, total: usize) -> Result<(usize, usize), String> {
    let num = |text: &str| -> Result<Option<usize>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        text.parse::<usize>().map(Some).map_err(|_| "Verse numbers must be digits.".to_string())
    };
    let (from, to) = (num(from)?, num(to)?);
    let (a, b) = match (from, to) {
        (None, None) => (1, total),
        (Some(a), None) => (a, a),
        (None, Some(b)) => (1, b),
        (Some(a), Some(b)) => (a, b),
    };
    if a == 0 || a > total || b < a {
        return Err(format!("Pick verses from 1 to {total}, with the first before the last."));
    }
    Ok((a, b.min(total)))
}

pub fn file_stem(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if stem.is_empty() { "passage".into() } else { stem }
}

pub fn to_text(title: &str, version: &str, verses: &[ExportVerse]) -> String {
    let mut out = format!("{title} ({version})\n\n");
    for verse in verses {
        out.push_str(&format!("{} {}\n", verse.number, verse.text));
        if let Some(note) = &verse.note {
            out.push_str(&format!("    Note: {}\n", note.replace('\n', "\n    ")));
        }
    }
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn to_html(title: &str, version: &str, verses: &[ExportVerse]) -> String {
    let mut out = String::from("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    out.push_str(&format!("<title>{}</title>", escape_html(title)));
    out.push_str(
        "<style>body{font-family:Georgia,serif;max-width:40em;margin:2em auto;padding:0 1em;line-height:1.55;color:#111}\
h1{font-size:1.5em;margin-bottom:0}.v{color:#555;margin-top:0}sup{color:#666;margin-right:.3em;font-size:.7em}\
p{margin:.35em 0}.n{margin:0 0 .6em 1.5em;font-size:.9em;color:#444;border-left:3px solid #bbb;padding-left:.6em}\
@media print{body{margin:0;max-width:none}}</style></head><body>",
    );
    out.push_str(&format!("<h1>{}</h1><p class=\"v\">{}</p>\n", escape_html(title), escape_html(version)));
    for verse in verses {
        out.push_str(&format!("<p><sup>{}</sup>{}</p>\n", verse.number, escape_html(&verse.text)));
        if let Some(note) = &verse.note {
            out.push_str(&format!("<p class=\"n\">Note: {}</p>\n", escape_html(note).replace('\n', "<br>")));
        }
    }
    out.push_str("</body></html>\n");
    out
}

fn pdf_byte(c: char) -> u8 {
    match c {
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201c}' => 0x93,
        '\u{201d}' => 0x94,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{2026}' => 0x85,
        c if (' '..='~').contains(&c) => c as u8,
        c if ('\u{a0}'..='\u{ff}').contains(&c) => c as u32 as u8,
        _ => b'?',
    }
}

fn pdf_string(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        let byte = pdf_byte(c);
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out.push(b')');
    out
}

/// Rough Helvetica advance in em, on the generous side so lines never run off the page.
fn glyph_em(c: char) -> f32 {
    match c {
        ' ' | 'i' | 'j' | 'l' | 't' | 'f' | 'I' | ',' | '.' | ';' | ':' | '\'' | '!' | '|' => 0.3,
        'm' | 'w' | 'M' | 'W' => 0.85,
        c if c.is_uppercase() => 0.7,
        _ => 0.56,
    }
}

fn wrap_pdf(text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0.0f32;
        for word in paragraph.split_whitespace() {
            let w: f32 = word.chars().map(glyph_em).sum::<f32>() * size;
            let space = 0.3 * size;
            if !line.is_empty() && used + space + w > width {
                lines.push(std::mem::take(&mut line));
                used = 0.0;
            }
            if !line.is_empty() {
                line.push(' ');
                used += space;
            }
            line.push_str(word);
            used += w;
        }
        lines.push(line);
    }
    lines
}

pub fn to_pdf(title: &str, version: &str, verses: &[ExportVerse]) -> Vec<u8> {
    const PAGE_W: f32 = 595.0;
    const PAGE_H: f32 = 842.0;
    const MARGIN: f32 = 56.0;
    const SIZE: f32 = 11.0;
    const LEAD: f32 = 15.0;
    let width = PAGE_W - MARGIN * 2.0;

    // (font index, size, indent, text)
    let mut rows: Vec<(u8, f32, f32, String)> = vec![(2, 16.0, 0.0, title.to_string()), (1, 10.0, 0.0, version.to_string()), (1, SIZE, 0.0, String::new())];
    for verse in verses {
        let text = format!("{}  {}", verse.number, verse.text);
        for line in wrap_pdf(&text, SIZE, width) {
            rows.push((1, SIZE, 0.0, line));
        }
        if let Some(note) = &verse.note {
            for line in wrap_pdf(&format!("Note: {note}"), SIZE - 1.0, width - 24.0) {
                rows.push((1, SIZE - 1.0, 24.0, line));
            }
        }
    }
    let per_page = (((PAGE_H - MARGIN * 2.0) / LEAD) as usize).max(1);
    let pages: Vec<&[(u8, f32, f32, String)]> = rows.chunks(per_page).collect();

    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids = (0..pages.len()).map(|i| format!("{} 0 R", 5 + i * 2)).collect::<Vec<_>>().join(" ");
    objects.push(format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    for (i, page) in pages.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                6 + i * 2
            )
            .into_bytes(),
        );
        let mut stream: Vec<u8> = Vec::new();
        for (row, (font, size, indent, text)) in page.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            let y = PAGE_H - MARGIN - LEAD * (row as f32 + 1.0) + 4.0;
            stream.extend(format!("BT /F{font} {size} Tf {} {y} Td ", MARGIN + indent).into_bytes());
            stream.extend(pdf_string(text));
            stream.extend(b" Tj ET\n");
        }
        let mut object = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        object.extend(stream);
        object.extend(b"\nendstream");
        objects.push(object);
    }

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        out.extend(object);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).into_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").into_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).into_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(count: usize) -> Vec<ExportVerse> {
        (1..=count)
            .map(|n| ExportVerse {
                number: n,
                text: format!("Verse {n} says \u{201c}hello\u{201d} (really) \\ and more words to make a line wrap around the page, again and again."),
                note: if n == 2 { Some("my <note>".into()) } else { None },
            })
            .collect()
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("", "", 31), Ok((1, 31)));
        assert_eq!(parse_range("5", "", 31), Ok((5, 5)));
        assert_eq!(parse_range("", "4", 31), Ok((1, 4)));
        assert_eq!(parse_range("3", "99", 31), Ok((3, 31)));
        assert!(parse_range("0", "", 31).is_err());
        assert!(parse_range("9", "3", 31).is_err());
        assert!(parse_range("x", "", 31).is_err());
        assert!(parse_range("40", "", 31).is_err());
    }

    #[test]
    fn text_and_html() {
        let verses = sample(3);
        let text = to_text("John 3", "KJV 1611", &verses);
        assert!(text.starts_with("John 3 (KJV 1611)"));
        assert!(text.contains("Note: my <note>"));
        let html = to_html("John 3", "KJV 1611", &verses);
        assert!(html.contains("my &lt;note&gt;"));
        assert_eq!(file_stem("1 John 3:16"), "1-John-3-16");
    }

    #[test]
    fn pdf_is_well_formed() {
        let pdf = to_pdf("Psalms 119", "KJV 1611", &sample(80));
        let _ = std::fs::write(std::env::temp_dir().join("twm-sample.pdf"), &pdf);
        let text = String::from_utf8_lossy(&pdf).to_string();
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.matches("/Type /Page ").count() >= 2);
        let start: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(pdf[start..].starts_with(b"xref"));
        assert!(text.contains("\\(really\\)"));
    }
}
