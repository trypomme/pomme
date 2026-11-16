//! Text shaping and bidirectional text: characters become the glyphs their
//! font draws them with (Arabic letters joined, marks placed, Latin kerned),
//! with HarfBuzz's rules (rustybuzz), and a line's runs are put in the order
//! they are shown, by Unicode's bidirectional algorithm (unicode-bidi).
//!
//! Text is shaped in segments: a stretch of characters drawn from one face
//! (the run's font, Inter for the system font, or a fallback font for a
//! script Inter lacks) at one size and weight. Emoji and what no font has
//! aren't shaped: one glyph each, as before.
//!
//! An app's own font is shaped in full, kerning too, as CoreText sets it.
//! The fonts that stand in for Apple's (Inter for SF Pro, the Noto fonts for
//! Apple's CJK, Arabic, Hebrew and Thai fonts) are shaped without kerning:
//! their kerning isn't Apple's, and their plain advances are what pomme's
//! text matches iOS with (the status bar's clock, tab and segment titles
//! moved away from the recordings with Inter's kerning).
//!
//! TextKit keeps one glyph per UTF-16 unit, so measuring gives each unit
//! the advance of the glyphs in its cluster (a ligature's or a joined
//! letter's width goes to its first unit, the others get none). Drawing
//! shapes each line in visual order.

use crate::textkit::TextFont;
use crate::Emu;
use std::ops::Range;

/// One glyph, as shaped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shaped {
    /// The UTF-16 unit its cluster starts at.
    pub unit: usize,
    /// The character there.
    pub ch: char,
    /// The glyph in its face; None to draw `ch` as before (emoji).
    pub gid: Option<u16>,
    /// The coretext face it's in (0: the system font).
    pub face: usize,
    /// How far the pen moves after it, and where it's drawn from the pen (y down), in points.
    pub advance: f64,
    pub offset: [f64; 2],
}

/// Where a character's glyph comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    /// The run's own font (an app's).
    Face(usize),
    /// A fallback font, for a script the system font lacks.
    Fallback(usize),
    /// The system font (Inter, at the run's weight and optical size).
    System,
    /// Not shaped (emoji, and what no font has).
    Plain,
}

fn has(emu: &mut Emu, face: usize, ch: char) -> bool {
    crate::coretext::face_data(emu, face).is_some_and(|(b, i, _)| ttf_parser::Face::parse(&b, i).ok().and_then(|f| f.glyph_index(ch)).is_some())
}

fn source(emu: &mut Emu, ch: char, font: &TextFont) -> Source {
    if font.face != 0 && has(emu, font.face, ch) {
        return Source::Face(font.face);
    }
    if ch.is_ascii() || crate::fallback::system_has(emu, ch) {
        return Source::System;
    }
    match crate::fallback::fallback(emu, ch, font.weight).ok().flatten() {
        Some(f) => Source::Fallback(f.face),
        None => Source::Plain,
    }
}

/// The characters of `text[range]`: (unit, char).
fn chars(text: &[u16], range: Range<usize>) -> Vec<(usize, char)> {
    let mut out = Vec::new();
    let mut k = range.start;
    while k < range.end.min(text.len()) {
        let (ch, n) = match char::decode_utf16([text[k], text.get(k + 1).copied().unwrap_or(0)]).next() {
            Some(Ok(c)) => (c, c.len_utf16()),
            _ => ('\u{fffd}', 1),
        };
        out.push((k, ch));
        k += n;
    }
    out
}

/// Shape one segment (one source, one font); `rtl`: its direction, None to guess it from its script.
fn shape_segment(emu: &mut Emu, chars: &[(usize, char)], src: Source, font: &TextFont, rtl: Option<bool>) -> Vec<Shaped> {
    let plain = |emu: &mut Emu| chars.iter().map(|&(unit, ch)| Shaped { unit, ch, gid: None, face: 0, advance: crate::textkit::advance_in(emu, ch, font), offset: [0.0; 2] }).collect();
    let (bytes, index, coords, face) = match src {
        Source::Plain => return plain(emu),
        Source::System => {
            let Some(b) = crate::textkit::font_bytes(emu) else { return plain(emu) };
            let wght = crate::textkit::ot_weight(font.weight).round() as f32;
            let opsz = font.size.clamp(14.0, 32.0).round() as f32;
            (b, 0, vec![(*b"wght", wght), (*b"opsz", opsz)], 0)
        }
        Source::Face(f) | Source::Fallback(f) => match crate::coretext::face_data(emu, f) {
            Some((b, i, c)) => (b, i, c, f),
            None => return plain(emu),
        },
    };
    let Some(mut hb) = rustybuzz::Face::from_slice(&bytes, index) else { return plain(emu) };
    let vars: Vec<rustybuzz::Variation> = coords.iter().map(|(t, v)| rustybuzz::Variation { tag: ttf_parser::Tag::from_bytes(t), value: *v }).collect();
    hb.set_variations(&vars);
    let s: String = chars.iter().map(|c| c.1).collect();
    // Each character's byte offset in `s` (the clusters shaping reports).
    let mut starts = Vec::with_capacity(chars.len());
    let mut b = 0;
    for &(_, ch) in chars {
        starts.push(b);
        b += ch.len_utf8();
    }
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(&s);
    if let Some(rtl) = rtl {
        buf.set_direction(if rtl { rustybuzz::Direction::RightToLeft } else { rustybuzz::Direction::LeftToRight });
    }
    buf.guess_segment_properties();
    // (Stand-in fonts unkerned; see above.)
    let unkerned = [rustybuzz::Feature::new(ttf_parser::Tag::from_bytes(b"kern"), 0, ..)];
    let features: &[rustybuzz::Feature] = if matches!(src, Source::Face(_)) { &[] } else { &unkerned };
    let out = rustybuzz::shape(&hb, features, buf);
    let scale = font.size / hb.units_per_em() as f64;
    out.glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(info, pos)| {
            let k = starts.partition_point(|&x| x <= info.cluster as usize).saturating_sub(1);
            let (unit, ch) = chars[k];
            Shaped { unit, ch, gid: Some(info.glyph_id as u16), face, advance: pos.x_advance as f64 * scale, offset: [pos.x_offset as f64 * scale, -pos.y_offset as f64 * scale] }
        })
        .collect()
}

/// Shape `chars` (logical order, one direction): split where the source or
/// the font changes, each segment shaped; in visual order if `rtl` is set.
fn shape_run(emu: &mut Emu, chars: &[(usize, char)], font_of: &dyn Fn(usize) -> TextFont, rtl: Option<bool>) -> Vec<Shaped> {
    let mut segments: Vec<(Source, TextFont, Vec<(usize, char)>)> = Vec::new();
    for &(unit, ch) in chars {
        let font = font_of(unit);
        let mut src = source(emu, ch, &font);
        // Spaces and other neutral characters join the segment they're in
        // (Inter has them), so a word's shaping isn't cut at them.
        if let Some(last) = segments.last()
            && last.1 == font
            && src == Source::System
            && !ch.is_alphanumeric()
            && last.0 != Source::Plain
            && (matches!(last.0, Source::System) || has_in(emu, last.0, ch))
        {
            src = last.0;
        }
        match segments.last_mut() {
            Some(last) if last.0 == src && last.1 == font => last.2.push((unit, ch)),
            _ => segments.push((src, font, vec![(unit, ch)])),
        }
    }
    if rtl == Some(true) {
        segments.reverse();
    }
    let mut out = Vec::new();
    for (src, font, cs) in segments {
        out.extend(shape_segment(emu, &cs, src, &font, rtl));
    }
    out
}

fn has_in(emu: &mut Emu, src: Source, ch: char) -> bool {
    match src {
        Source::Face(f) | Source::Fallback(f) => has(emu, f, ch),
        _ => true,
    }
}

/// Each UTF-16 unit's advance in `text`, shaped, in points (a cluster's
/// advance on its first unit).
pub(crate) fn unit_advances(emu: &mut Emu, text: &[u16], font_of: &dyn Fn(usize) -> TextFont) -> Vec<f64> {
    let mut adv = vec![0.0; text.len()];
    // One paragraph at a time (shaping doesn't cross a line break).
    let cs = chars(text, 0..text.len());
    for para in cs.split(|&(_, ch)| matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')) {
        for g in shape_run(emu, para, font_of, None) {
            adv[g.unit] += g.advance;
        }
    }
    adv
}

/// Whether the paragraph `text[range]` reads right to left: its first
/// strong character's direction, else `default`.
pub(crate) fn paragraph_rtl(text: &[u16], range: Range<usize>, default: bool) -> bool {
    for (_, ch) in chars(text, range) {
        match unicode_bidi::bidi_class(ch) {
            unicode_bidi::BidiClass::L => return false,
            unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL => return true,
            _ => {}
        }
    }
    default
}

/// The paragraph around unit `at` of `text`.
fn paragraph_at(text: &[u16], at: usize) -> Range<usize> {
    let nl = |u: &u16| matches!(*u, 0x0a | 0x0d | 0x2028 | 0x2029);
    let start = text[..at.min(text.len())].iter().rposition(nl).map_or(0, |i| i + 1);
    let end = text[at.min(text.len())..].iter().position(nl).map_or(text.len(), |i| at + i);
    start..end
}

/// The runs of the line `text[line]` in the order they're shown: each its
/// characters (logical order) and whether it reads right to left. The
/// paragraph's direction is its first strong character's (else `default_rtl`).
fn visual_runs(text: &[u16], line: Range<usize>, default_rtl: bool) -> Vec<(Vec<(usize, char)>, bool)> {
    let para = paragraph_at(text, line.start);
    let rtl = paragraph_rtl(text, para.clone(), default_rtl);
    let all = chars(text, para);
    let s: String = all.iter().map(|c| c.1).collect();
    // Byte offsets in `s` ↔ characters.
    let mut byte_of = Vec::with_capacity(all.len());
    let mut b = 0;
    for &(_, ch) in &all {
        byte_of.push(b);
        b += ch.len_utf8();
    }
    let index_of_byte = |byte: usize| byte_of.partition_point(|&x| x < byte);
    let in_line: Vec<usize> = (0..all.len()).filter(|&i| all[i].0 >= line.start && all[i].0 < line.end).collect();
    let (Some(&first), Some(&last)) = (in_line.first(), in_line.last()) else { return Vec::new() };
    let level = if rtl { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() };
    let info = unicode_bidi::BidiInfo::new(&s, Some(level));
    let Some(p) = info.paragraphs.first() else { return Vec::new() };
    let (levels, runs) = info.visual_runs(p, byte_of[first]..byte_of[last] + all[last].1.len_utf8());
    runs.into_iter().map(|run| ((index_of_byte(run.start)..index_of_byte(run.end)).map(|i| all[i]).collect(), levels[run.start].is_rtl())).collect()
}

/// The glyphs of the line `text[line]` in the order they're shown, each with
/// its x from the line's start (pen positions; add `offset` to draw).
/// `default_rtl`: the direction of a paragraph with no strong character (the
/// view's). `extra(unit)`: space added after a cluster (letter spacing).
pub(crate) fn line_visual(emu: &mut Emu, text: &[u16], line: Range<usize>, font_of: &dyn Fn(usize) -> TextFont, default_rtl: bool, extra: &dyn Fn(usize) -> f64) -> Vec<(Shaped, f64)> {
    let mut out = Vec::new();
    let mut pen = 0.0;
    for (cs, rtl) in visual_runs(text, line, default_rtl) {
        let glyphs = shape_run(emu, &cs, font_of, Some(rtl));
        for (i, g) in glyphs.iter().enumerate() {
            out.push((*g, pen));
            pen += g.advance;
            // (Letter spacing after the cluster's last glyph.)
            if glyphs.get(i + 1).is_none_or(|n| n.unit != g.unit) {
                pen += extra(g.unit);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_direction_from_the_first_strong_character() {
        let u = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert!(paragraph_rtl(&u("123 مرحبا Pomme"), 0..14, false));
        assert!(!paragraph_rtl(&u("Pomme مرحبا"), 0..11, true));
        assert!(paragraph_rtl(&u("123"), 0..3, true));
        let t = u("ab\nשלום");
        assert_eq!(paragraph_at(&t, 4), 3..7);
        assert_eq!(paragraph_at(&t, 1), 0..2);
    }

    /// The runs' text in the order shown, right-to-left runs marked.
    fn shown(s: &str, default_rtl: bool) -> Vec<String> {
        let u: Vec<u16> = s.encode_utf16().collect();
        visual_runs(&u, 0..u.len(), default_rtl).into_iter().map(|(cs, rtl)| format!("{}{}", if rtl { "<" } else { ">" }, cs.iter().map(|c| c.1).collect::<String>())).collect()
    }

    #[test]
    fn runs_in_the_order_shown() {
        // fixtures/rtl/ios26-ar.png: "مرحبا Pomme 123" shows as "Pomme 123 مرحبا".
        assert_eq!(shown("مرحبا Pomme 123", false), [">Pomme 123", "<مرحبا "]);
        // (Each run's characters are in logical order.) A left-to-right
        // paragraph: digits after an Arabic word are Arabic numbers, in its
        // run, so "Pomme مرحبا 123" also shows as "Pomme 123 مرحبا", as iOS
        // draws it (fixtures/rtl/ios26-ar.png).
        assert_eq!(shown("Pomme مرحبا 123", true), [">Pomme ", ">123", "<مرحبا "]);
        assert_eq!(shown("שלום עולם", false), ["<שלום עולם"]);
        assert_eq!(shown("Hello", true), [">Hello"]);
    }
}
