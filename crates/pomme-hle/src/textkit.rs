//! TextKit 1: NSTextStorage, NSLayoutManager and NSTextContainer, enough
//! for apps (React Native in particular) to measure and hit-test text.
//!
//! Glyph advances come from a real font: Inter (open source, metrics close to
//! SF Pro), read from the sysroot with ttf-parser at the requested weight and
//! optical size. Every font is laid out with it. Vertical metrics stay
//! UIFont's (SF Pro proportions), so measured line heights match iOS.
//!
//! Layout is greedy: lines break where Unicode's line breaking algorithm
//! (UAX #14) allows, which keeps words whole and follows the Japanese rules
//! (no `、。」` or small kana at the start of a line, no `「` at its end), or
//! mid-word when a word is wider than the line. Chinese, Japanese and Korean
//! come from fallback fonts at iOS's widths, and on a Japanese phone with
//! iOS's punctuation spacing (cjk.rs). One glyph per UTF-16 unit, so glyph
//! indexes and ranges are character indexes and ranges. No kerning or
//! ligatures yet.
//!
//! Objects (offsets in bytes):
//! - NSTextContainer {isa, width, height, lineFragmentPadding, maximumNumberOfLines, lineBreakMode, layoutManager}
//! - NSLayoutManager {isa, textStorage, textContainer}
//! - NSTextStorage {isa, CF mutable attributed string, layoutManager}; every
//!   attributed-string message is forwarded to the CF string.

use crate::foundation::{self, cf};
use crate::objc;
use crate::uikit::{ret_f64, ret_rect};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

pub(crate) const LINE_HEIGHT: f64 = 1.193;
pub(crate) const ASCENT: f64 = 0.952;
const DEFAULT_SIZE: f64 = 12.0; // TextKit's default (Helvetica 12)
const NOT_FOUND: u64 = i64::MAX as u64;

// ---- fonts ----

#[derive(Clone, Default)]
pub struct State {
    /// System fonts by file name, once looked for (None: not in the sysroot).
    fonts: HashMap<&'static str, Option<std::rc::Rc<Vec<u8>>>>,
    /// (char, weight, optical size, face) → advance in em.
    advances: HashMap<(char, u16, u16, usize), f64>,
}

/// A font from the sysroot's System/Library/Fonts (found from where
/// CoreFoundation was loaded), read once and shared.
pub(crate) fn system_font(emu: &mut Emu, file: &'static str) -> Option<std::rc::Rc<Vec<u8>>> {
    if !emu.text.fonts.contains_key(file) {
        let path = emu
            .p
            .images
            .iter()
            .find(|i| i.path.ends_with("System/Library/Frameworks/CoreFoundation.framework/CoreFoundation"))
            .and_then(|i| i.path.ancestors().nth(5).map(|root| root.join("System/Library/Fonts").join(file)));
        let bytes = path.and_then(|p| pomme_loader::vfs::read(&p).ok()).map(std::rc::Rc::new);
        emu.text.fonts.insert(file, bytes);
    }
    emu.text.fonts[file].clone()
}

/// The text font (Inter).
pub(crate) fn font_bytes(emu: &mut Emu) -> Option<std::rc::Rc<Vec<u8>>> {
    system_font(emu, "Inter.ttf")
}

/// The emoji font (Noto Color Emoji), for characters the text font lacks.
pub(crate) fn emoji_bytes(emu: &mut Emu) -> Option<std::rc::Rc<Vec<u8>>> {
    system_font(emu, "NotoColorEmoji.ttf")
}

/// UIFontWeight (-0.8 … 0.62) → OpenType `wght` (100 … 900).
pub(crate) fn ot_weight(w: f64) -> f64 {
    const STOPS: [(f64, f64); 9] = [(-0.8, 100.0), (-0.6, 200.0), (-0.4, 300.0), (0.0, 400.0), (0.23, 500.0), (0.3, 600.0), (0.4, 700.0), (0.56, 800.0), (0.62, 900.0)];
    if w <= STOPS[0].0 {
        return STOPS[0].1;
    }
    for p in STOPS.windows(2) {
        let ((a, wa), (b, wb)) = (p[0], p[1]);
        if w <= b {
            return wa + (wb - wa) * (w - a) / (b - a);
        }
    }
    900.0
}

/// Advance of `ch` in the emoji font, in em.
fn emoji_advance(emu: &mut Emu, ch: char) -> Option<f64> {
    let bytes = emoji_bytes(emu)?;
    let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
    let g = face.glyph_index(ch)?;
    Some(face.glyph_hor_advance(g)? as f64 / face.units_per_em() as f64)
}

/// One line of text as glyphs from `x` along `baseline`, and its width.
pub(crate) fn line_glyphs(emu: &mut Emu, text: &str, size: f64, weight: f64, color: [f64; 4], x: f64, baseline: f64) -> (Vec<crate::render::Glyph>, f64) {
    let mut pen = x;
    let mut out = Vec::new();
    for ch in text.chars() {
        out.push(crate::render::Glyph { ch, x: pen, y: baseline, size, weight, face: 0, color, skew: 0.0, shadow: None });
        pen += advance(emu, ch, size, weight);
    }
    let _ = crate::cjk::assign(emu, &mut out);
    (out, pen - x)
}

/// One line of text in `font`, as glyphs from `x` along `baseline`, and its width.
pub(crate) fn line_glyphs_in(emu: &mut Emu, text: &str, font: &TextFont, color: [f64; 4], x: f64, baseline: f64) -> (Vec<crate::render::Glyph>, f64) {
    let mut pen = x;
    let mut out = Vec::new();
    for ch in text.chars() {
        out.push(crate::render::Glyph { ch, x: pen, y: baseline, size: font.size, weight: font.weight, face: font.face, color, skew: font.skew(), shadow: None });
        pen += advance_in(emu, ch, font);
    }
    let _ = crate::cjk::assign(emu, &mut out);
    (out, pen - x)
}

/// A font as text is set in it: a size, a weight (for the system font's
/// weight axis) and a face (`coretext` faces; 0: the system font).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextFont {
    pub size: f64,
    pub weight: f64,
    pub face: usize,
    /// Italic where the face has no italic of its own (the system font): drawn oblique.
    pub italic: bool,
}

impl TextFont {
    pub fn system(size: f64, weight: f64) -> TextFont {
        TextFont { size, weight, face: 0, italic: false }
    }

    /// How far its glyphs lean (synthetic italic), or 0.
    pub fn skew(&self) -> f32 {
        if self.italic { crate::render::ITALIC_SKEW } else { 0.0 }
    }
}

/// Advance width of `ch` in points, in the system font.
pub(crate) fn advance(emu: &mut Emu, ch: char, size: f64, weight: f64) -> f64 {
    advance_in(emu, ch, &TextFont::system(size, weight))
}

/// The fonts available changed: what was measured in them goes.
pub(crate) fn fonts_changed(emu: &mut Emu) {
    emu.text.advances.retain(|k, _| k.3 == 0);
}

/// A run's line height and ascent: the system font's proportions (SF's), or
/// a face's own (its `hhea` ascender, descender and line gap).
pub(crate) fn line_metrics(emu: &mut Emu, f: &TextFont) -> (f64, f64) {
    if f.face != 0
        && let Some((bytes, index, _)) = crate::coretext::face_data(emu, f.face)
        && let Ok(face) = ttf_parser::Face::parse(&bytes, index)
    {
        let upem = face.units_per_em() as f64;
        let (asc, desc, gap) = (face.ascender() as f64, face.descender() as f64, face.line_gap() as f64);
        return ((asc - desc + gap) / upem * f.size, asc / upem * f.size);
    }
    (f.size * LINE_HEIGHT, f.size * ASCENT)
}

/// Advance width of `ch` in points, in `f`.
pub(crate) fn advance_in(emu: &mut Emu, ch: char, f: &TextFont) -> f64 {
    let (size, weight) = (f.size, f.weight);
    let wght = ot_weight(weight).round() as u16;
    let opsz = size.clamp(14.0, 32.0).round() as u16;
    let key = (ch, wght, opsz, f.face);
    if let Some(&em) = emu.text.advances.get(&key) {
        return em * size;
    }
    // An app's face (at its named instance), or the system font.
    let (text, index, coords, axis_weight) = match crate::coretext::face_data(emu, f.face).filter(|_| f.face != 0) {
        Some((bytes, index, coords)) => (Some(bytes), index, coords, false),
        None => (font_bytes(emu), 0, Vec::new(), true),
    };
    let face = text.as_deref().and_then(|d| ttf_parser::Face::parse(d, index).ok());
    let em = match face {
        Some(mut face) => {
            if axis_weight {
                face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), wght as f32);
                face.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), opsz as f32);
            }
            for (tag, v) in &coords {
                face.set_variation(ttf_parser::Tag::from_bytes(tag), *v);
            }
            let upem = face.units_per_em() as f64;
            match face.glyph_index(ch) {
                Some(g) => face.glyph_hor_advance(g).map_or(0.55, |a| a as f64 / upem),
                // Not in an app's font: the system font's (as iOS falls back).
                None if !axis_weight => advance(emu, ch, size, weight) / size,
                // Not in the text font: Chinese, Japanese and Korean from their
                // fonts (at iOS's widths); emoji and other symbols from the emoji font.
                None => match crate::cjk::fallback(emu, ch, weight).ok().flatten() {
                    Some(f) => f.advance,
                    None => emoji_advance(emu, ch).unwrap_or_else(|| {
                    face.glyph_index('\u{fffd}').and_then(|g| face.glyph_hor_advance(g)).map_or(0.55, |a| a as f64 / upem)
                    }),
                },
            }
        }
        None if ch.is_whitespace() => 0.25,
        None => 0.55,
    };
    emu.text.advances.insert(key, em);
    em * size
}

// ---- layout ----

/// A stretch of text in one font, in UTF-16 units, with that font's line
/// height and ascent.
#[derive(Clone, Copy, Debug)]
pub struct Run {
    pub len: usize,
    pub font: TextFont,
    pub line_height: f64,
    pub ascent: f64,
    /// Space added after each character (NSKern), in points.
    pub kern: f64,
    /// Its paragraph's minimum and maximum line height (0: none).
    pub para: [f64; 2],
}

impl Run {
    /// `len` units in `font`.
    pub(crate) fn new(emu: &mut Emu, len: usize, font: TextFont) -> Run {
        let (line_height, ascent) = line_metrics(emu, &font);
        Run { len, font, line_height, ascent, kern: 0.0, para: [0.0; 2] }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    /// UTF-16 range.
    pub start: usize,
    pub len: usize,
    pub y: f64,
    pub height: f64,
    pub ascent: f64,
    /// Width without trailing whitespace.
    pub width: f64,
    /// x of each unit's leading edge (len + 1 entries).
    pub xs: Vec<f64>,
    /// Where an ellipsis ("…") is drawn after the line's text: the last line
    /// of text cut short (a container's tail truncation).
    pub ellipsis: Option<f64>,
    /// Space before a unit's glyph within its advance (Japanese spacing before
    /// an opening bracket): (unit, points).
    pub leads: Vec<(usize, f64)>,
}

struct Item {
    unit: usize,
    units: usize,
    ch: char,
    adv: f64,
    /// Space before its glyph, included in `adv`.
    lead: f64,
    /// Its run's font size and face (Japanese spacing is for the system font).
    size: f64,
    face: usize,
    /// A line may break before it.
    break_before: bool,
    /// Its run's line height and ascent.
    metrics: (f64, f64),
    /// Its paragraph's minimum and maximum line height (0: none).
    para: [f64; 2],
}

fn is_newline(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Break `items` into lines no wider than `width` (and no taller than
/// `height`, keeping at least one line), at most `max_lines` (0: no limit).
fn break_lines(items: &[Item], total_units: usize, width: f64, height: f64, max_lines: usize) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut i = 0;
    let mut y = 0.0;
    while i < items.len() {
        let start = i;
        let mut x = 0.0;
        let mut last_break = None;
        let mut end = items.len();
        let mut j = i;
        while j < items.len() {
            let it = &items[j];
            if is_newline(it.ch) {
                end = j + 1;
                break;
            }
            if j > start && it.break_before {
                last_break = Some(j);
            }
            if x + it.adv > width + 1e-9 && j > start && !it.ch.is_whitespace() {
                end = last_break.unwrap_or(j);
                break;
            }
            x += it.adv;
            j += 1;
        }
        let line_items = &items[start..end];
        let mut xs = Vec::with_capacity(line_items.len() + 1);
        let mut cx = 0.0;
        for it in line_items {
            for _ in 0..it.units {
                xs.push(cx);
            }
            cx += it.adv;
        }
        xs.push(cx);
        let width = line_items.iter().rev().skip_while(|it| it.ch.is_whitespace()).map(|it| it.adv).sum();
        // The line is as tall as its tallest font, then as its paragraph style
        // allows (minimum and maximum line height); TextKit adds (or takes)
        // the difference above the text, so the baseline moves with it.
        let (natural, asc) = line_items.iter().map(|it| it.metrics).fold((0.0, 0.0), |a, m| (f64::max(a.0, m.0), f64::max(a.1, m.1)));
        let para = line_items.first().map_or([0.0; 2], |it| it.para);
        let mut lh = natural;
        if para[0] > 0.0 {
            lh = lh.max(para[0]);
        }
        if para[1] > 0.0 {
            lh = lh.min(para[1]);
        }
        let asc = asc + (lh - natural);
        if !lines.is_empty() && y + lh > height + 1e-9 {
            break;
        }
        let unit = line_items[0].unit;
        let next = items.get(end).map_or(total_units, |it| it.unit);
        let leads = line_items.iter().filter(|it| it.lead != 0.0).map(|it| (it.unit, it.lead)).collect();
        lines.push(Line { start: unit, len: next - unit, y, height: lh, ascent: asc, width, xs, ellipsis: None, leads });
        y += lh;
        i = end;
        if max_lines != 0 && lines.len() == max_lines {
            break;
        }
    }
    lines
}

/// Lay out UTF-16 text with per-run fonts; `adv(ch, font)` measures a
/// character; `japanese` adds iOS's Japanese punctuation spacing (a Japanese
/// phone: [`crate::cjk::japanese_spacing`]).
pub fn layout(text: &[u16], runs: &[Run], mut adv: impl FnMut(char, &TextFont) -> f64, japanese: bool, width: f64, height: f64, max_lines: usize) -> Vec<Line> {
    let mut items = Vec::new();
    let mut unit = 0;
    let mut run_iter = runs.iter();
    let mut run = run_iter.next().copied();
    let mut run_end = run.map_or(usize::MAX, |r| r.len);
    for c in char::decode_utf16(text.iter().copied()) {
        let ch = c.unwrap_or('\u{fffd}');
        while unit >= run_end {
            match run_iter.next() {
                Some(&r) => {
                    run = Some(r);
                    run_end += r.len;
                }
                None => {
                    run_end = usize::MAX;
                }
            }
        }
        let font = run.map_or(TextFont::system(DEFAULT_SIZE, 0.0), |r| r.font);
        let metrics = run.map_or((DEFAULT_SIZE * LINE_HEIGHT, DEFAULT_SIZE * ASCENT), |r| (r.line_height, r.ascent));
        let (kern, para) = run.map_or((0.0, [0.0; 2]), |r| (r.kern, r.para));
        let units = ch.len_utf16();
        let a = if is_newline(ch) { 0.0 } else { adv(ch, &font) + kern };
        items.push(Item { unit, units, ch, adv: a, lead: 0.0, size: font.size, face: font.face, break_before: false, metrics, para });
        unit += units;
    }
    if japanese {
        for i in 1..items.len() {
            let (a, b) = (&items[i - 1], &items[i]);
            if a.face != 0 || b.face != 0 {
                continue;
            }
            if let Some((em, leading)) = crate::cjk::pair_spacing(a.ch, b.ch) {
                if leading {
                    let pts = em * b.size;
                    items[i].adv += pts;
                    items[i].lead += pts;
                } else {
                    items[i - 1].adv += em * a.size;
                }
            }
        }
    }
    mark_breaks(&mut items);
    break_lines(&items, text.len(), width, height, max_lines)
}

/// Where lines may break: before each item that follows a break opportunity
/// in Unicode's line breaking algorithm.
fn mark_breaks(items: &mut [Item]) {
    let s: String = items.iter().map(|it| it.ch).collect();
    let mut starts = Vec::with_capacity(items.len());
    let mut byte = 0;
    for it in items.iter() {
        starts.push(byte);
        byte += it.ch.len_utf8();
    }
    for (at, _) in unicode_linebreak::linebreaks(&s) {
        if let Ok(k) = starts.binary_search(&at) {
            items[k].break_before = true;
        }
    }
}

// ---- reading the guest's objects ----

fn f64_at(emu: &Emu, a: u64) -> Result<f64> {
    Ok(f64::from_bits(emu.read_u64(a)?))
}

fn class_name_of(emu: &Emu, obj: u64) -> String {
    emu.read_u64(obj).map(|isa| emu.objc.class_name(isa)).unwrap_or_default()
}

/// The CF attributed string behind a text storage (or the object itself).
pub fn cf_attributed(emu: &mut Emu, obj: u64) -> Result<u64> {
    if obj != 0 && !foundation::is_cf(emu, obj) && class_name_of(emu, obj) == "NSTextStorage" {
        return emu.read_u64(obj + 8);
    }
    Ok(obj)
}

/// The font in an attributes dictionary (`NSFontAttributeName`), or 0.
pub(crate) fn font_in_attributes(emu: &mut Emu, attrs: u64) -> Result<u64> {
    if attrs == 0 {
        return Ok(0);
    }
    let key = foundation::new_nsstring(emu, "NSFontAttributeName")?;
    crate::objc::send(emu, attrs, "objectForKey:", &[key])
}

/// `-[NSString sizeWithAttributes:]`: one line, unwrapped, in the attributes' font.
fn size_with_attributes(emu: &mut Emu) -> Result<()> {
    let (string, attrs) = (emu.arg(0), emu.arg(2));
    let text = foundation::nsstring(emu, string)?;
    let font = font_in_attributes(emu, attrs)?;
    let f = font_info(emu, font)?;
    let width: f64 = text.chars().map(|ch| advance_in(emu, ch, &f)).sum();
    let height = if text.is_empty() { 0.0 } else { line_metrics(emu, &f).0.ceil() };
    emu.cpu.q[0] = width.to_bits() as u128;
    emu.cpu.q[1] = height.to_bits() as u128;
    Ok(())
}

/// `-[NSString boundingRectWithSize:options:attributes:context:]`: wrapped at
/// the width given with NSStringDrawingUsesLineFragmentOrigin (1), else one line.
fn bounding_rect_with_size(emu: &mut Emu) -> Result<()> {
    let (string, options, attrs) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let (w, h) = (d(emu, 0), d(emu, 1));
    let text: Vec<u16> = foundation::nsstring(emu, string)?.encode_utf16().collect();
    let font = font_in_attributes(emu, attrs)?;
    let f = font_info(emu, font)?;
    let runs = [Run::new(emu, text.len(), f)];
    let (width, height, max_lines) = if options & 1 != 0 { (w, h, 0) } else { (f64::INFINITY, f64::INFINITY, 1) };
    let japanese = crate::cjk::japanese_spacing(emu)?;
    let lines = layout(&text, &runs, |ch, f| advance_in(emu, ch, f), japanese, width, height, max_lines);
    let used_w = lines.iter().map(|l| l.width).fold(0.0, f64::max);
    let used_h = lines.last().map_or(0.0, |l| l.y + l.height);
    ret_rect(emu, [0.0, 0.0, used_w, used_h]);
    Ok(())
}

/// A UIFont as text is set in it: its size, weight and face (other fonts:
/// TextKit's default).
pub(crate) fn font_info(emu: &mut Emu, font: u64) -> Result<TextFont> {
    if font == 0 || foundation::is_cf(emu, font) || class_name_of(emu, font) != "UIFont" {
        return Ok(TextFont::system(DEFAULT_SIZE, 0.0));
    }
    let face = crate::uiview::font_face(emu, font)?;
    // (The system font's italics are its name's: `.SFUI-Italic`, `.SFUI-BoldItalic`.)
    let italic = face == 0 && foundation::nsstring(emu, emu.read_u64(font + 8)?)?.contains("Italic");
    Ok(TextFont { size: f64_at(emu, font + 16)?, weight: f64_at(emu, font + 24)?, face, italic })
}

struct Laid {
    lines: Vec<Line>,
    padding: f64,
    len: usize,
    /// Per run, how it's drawn.
    styles: Styles,
    text: Vec<u16>,
}

/// An underline or strikethrough: NSUnderlineStyle (single 1, thick 2,
/// double 9) and its color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Decoration {
    pub style: u64,
    pub color: [f64; 4],
}

/// How a run of a CF attributed string is drawn: UTF-16 range, font, color
/// (black where it sets none, as UIKit draws), baseline offset
/// (NSBaselineOffsetAttributeName: up), decorations and shadow.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Style {
    pub start: usize,
    pub end: usize,
    pub font: TextFont,
    pub color: [f64; 4],
    pub baseline: f64,
    pub underline: Option<Decoration>,
    pub strike: Option<Decoration>,
    pub shadow: Option<crate::render::TextShadow>,
}

type Styles = Vec<Style>;

fn plain_style() -> Style {
    Style { start: 0, end: 0, font: TextFont::system(DEFAULT_SIZE, 0.0), color: [0.0, 0.0, 0.0, 1.0], baseline: 0.0, underline: None, strike: None, shadow: None }
}

fn attributed_runs(emu: &mut Emu, s: u64) -> Result<(Vec<u16>, Vec<Run>, Styles)> {
    let string = cf(emu, "_CFAttributedStringGetString", &[s])?;
    let text: Vec<u16> = foundation::nsstring(emu, string)?.encode_utf16().collect();
    let mut styles = Vec::new();
    let range = emu.alloc(16)?;
    let mut runs = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let attrs = cf(emu, "_CFAttributedStringGetAttributes", &[s, i as u64, range])?;
        let (loc, n) = (emu.read_u64(range)? as usize, emu.read_u64(range + 8)? as usize);
        let a = Attrs(attrs);
        let font = a.get(emu, "NSFontAttributeName")?;
        let f = font_info(emu, font)?;
        let rgba = a.color(emu, "NSForegroundColorAttributeName")?.unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let end = (loc + n).max(i + 1).min(text.len());
        let mut run = Run::new(emu, end - i, f);
        run.kern = a.number(emu, "NSKernAttributeName")?.unwrap_or(0.0);
        let para = a.get(emu, "NSParagraphStyleAttributeName")?;
        if para != 0 {
            let line = |k| crate::objc::bag_get_f64(emu, para, k).unwrap_or(0.0).max(0.0);
            run.para = [line("minimumLineHeight"), line("maximumLineHeight")];
        }
        runs.push(run);
        let mut style = Style { start: i, end, font: f, color: rgba, ..plain_style() };
        style.baseline = a.number(emu, "NSBaselineOffsetAttributeName")?.unwrap_or(0.0);
        for (key, color_key, slot) in [("NSUnderlineStyleAttributeName", "NSUnderlineColorAttributeName", 0), ("NSStrikethroughStyleAttributeName", "NSStrikethroughColorAttributeName", 1)] {
            let v = a.number(emu, key)?.unwrap_or(0.0) as u64;
            if v & 0xff != 0 {
                let color = a.color(emu, color_key)?.unwrap_or(rgba);
                let d = Some(Decoration { style: v, color });
                if slot == 0 { style.underline = d } else { style.strike = d }
            }
        }
        let shadow = a.get(emu, "NSShadowAttributeName")?;
        if shadow != 0 {
            style.shadow = Some(shadow_of(emu, shadow));
        }
        styles.push(style);
        i = end;
    }
    emu.heap.free(&emu.p.memory, range)?;
    Ok((text, runs, styles))
}

/// The glyphs of laid-out lines, the first line's top-left at (x, y).
fn glyphs_of(text: &[u16], styles: &Styles, lines: &[Line], x: f64, y: f64, range: (usize, usize)) -> Vec<crate::render::Glyph> {
    let mut glyphs = Vec::new();
    for line in lines {
        let mut k = line.start.max(range.0);
        while k < (line.start + line.len).min(range.1) {
            let (ch, units) = match char::decode_utf16([text[k], text.get(k + 1).copied().unwrap_or(0)]).next() {
                Some(Ok(c)) => (c, c.len_utf16()),
                _ => ('\u{fffd}', 1),
            };
            if !ch.is_whitespace() {
                let st = styles.iter().find(|s| k >= s.start && k < s.end).copied().unwrap_or_else(plain_style);
                let f = st.font;
                let lead = line.leads.iter().find(|l| l.0 == k).map_or(0.0, |l| l.1);
                glyphs.push(crate::render::Glyph {
                    ch,
                    x: x + line.xs[k - line.start] + lead,
                    y: y + line.y + line.ascent - st.baseline,
                    size: f.size,
                    weight: f.weight,
                    face: f.face,
                    color: st.color,
                    skew: f.skew(),
                    shadow: st.shadow,
                });
            }
            k += units;
        }
        if let Some(ex) = line.ellipsis {
            let at = (line.start + line.len).saturating_sub(1);
            let st = styles.iter().find(|s| at >= s.start && at < s.end).copied().unwrap_or_else(plain_style);
            let f = st.font;
            glyphs.push(crate::render::Glyph { ch: '\u{2026}', x: x + ex, y: y + line.y + line.ascent - st.baseline, size: f.size, weight: f.weight, face: f.face, color: st.color, skew: f.skew(), shadow: st.shadow });
        }
    }
    glyphs
}

/// An attributed string laid out at `width` (wrapping; infinite: only at newlines).
fn attributed_lines(emu: &mut Emu, s: u64, width: f64, height: f64, max_lines: usize) -> Result<(Vec<u16>, Styles, Vec<Line>)> {
    let (text, runs, styles) = attributed_runs(emu, s)?;
    let japanese = crate::cjk::japanese_spacing(emu)?;
    let mut lines = layout(&text, &runs, |ch, f| advance_in(emu, ch, f), japanese, width, height, max_lines);
    align_lines(emu, s, &mut lines, width)?;
    Ok((text, styles, lines))
}

/// Tail truncation: if text goes on past the last line, that line is laid
/// out again from its start, character by character (not at word breaks),
/// with as much as fits before an ellipsis ("…") in its last run's font.
fn truncate_tail(text: &[u16], runs: &[Run], lines: &mut [Line], width: f64, adv: &mut impl FnMut(char, &TextFont) -> f64) {
    let Some(last) = lines.last_mut() else { return };
    let end = last.start + last.len;
    let rest_is_newline = text.get(end - 1).is_some_and(|&u| char::from_u32(u as u32).is_some_and(is_newline));
    if end >= text.len() && !rest_is_newline {
        return;
    }
    if end >= text.len() {
        return;
    }
    let run_at = |unit: usize| {
        let mut at = 0;
        for r in runs {
            if unit < at + r.len {
                return Some(*r);
            }
            at += r.len;
        }
        runs.last().copied()
    };
    let font_end = run_at(end.saturating_sub(1)).map_or(TextFont::system(DEFAULT_SIZE, 0.0), |r| r.font);
    let ell = adv('\u{2026}', &font_end);
    let mut xs = vec![0.0];
    let mut x = 0.0;
    let mut k = last.start;
    for c in char::decode_utf16(text[last.start..].iter().copied()) {
        let ch = c.unwrap_or('\u{fffd}');
        if is_newline(ch) {
            break;
        }
        let r = run_at(k);
        let a = adv(ch, &r.map_or(font_end, |r| r.font)) + r.map_or(0.0, |r| r.kern);
        if x + a + ell > width + 1e-9 {
            break;
        }
        x += a;
        for _ in 0..ch.len_utf16() {
            xs.push(x);
        }
        k += ch.len_utf16();
    }
    // (xs has an entry per unit kept, plus the end.)
    xs.truncate(k - last.start + 1);
    last.len = k - last.start;
    last.xs = xs;
    last.width = x + ell;
    last.ellipsis = Some(x);
}

/// Each line where its paragraph style puts it in `width` (NSTextAlignment:
/// centred 1, right 2; left, justified and natural stay at the leading edge).
fn align_lines(emu: &mut Emu, s: u64, lines: &mut [Line], width: f64) -> Result<()> {
    if !width.is_finite() || lines.is_empty() {
        return Ok(());
    }
    let key = foundation::new_nsstring(emu, "NSParagraphStyleAttributeName")?;
    let range = emu.alloc(16)?;
    for line in lines.iter_mut() {
        let attrs = cf(emu, "_CFAttributedStringGetAttributes", &[s, line.start as u64, range])?;
        let style = if attrs == 0 { 0 } else { cf(emu, "_CFDictionaryGetValue", &[attrs, key])? };
        if style == 0 {
            continue;
        }
        // (NSParagraphStyle is a property bag in pomme: `setAlignment:`'s value; unset, natural.)
        let offset = match objc::bag_get(emu, style, "alignment").unwrap_or(4) {
            1 => (width - line.width) / 2.0,
            2 => width - line.width,
            _ => 0.0,
        };
        if offset > 0.0 {
            for x in &mut line.xs {
                *x += offset;
            }
            if let Some(e) = &mut line.ellipsis {
                *e += offset;
            }
        }
    }
    emu.heap.free(&emu.p.memory, range)?;
    Ok(())
}

fn lines_size(lines: &[Line]) -> (f64, f64) {
    (lines.iter().map(|l| l.width).fold(0.0, f64::max), lines.last().map_or(0.0, |l| l.y + l.height))
}

/// The CF attributed string a string with attributes makes.
fn attributed_from(emu: &mut Emu, string: u64, attrs: u64) -> Result<u64> {
    let attrs = if attrs == 0 { foundation::new_dict(emu)? } else { attrs };
    cf(emu, "_CFAttributedStringCreate", &[0, string, attrs])
}

/// UIKit's string drawing (NSStringDrawing): `size`, `boundingRectWithSize:…`,
/// `drawAtPoint:` and `drawInRect:`, for an attributed string, into the current context.
fn attributed_size(emu: &mut Emu) -> Result<()> {
    let s = cf_attributed(emu, emu.arg(0))?;
    let (_, _, lines) = attributed_lines(emu, s, f64::INFINITY, f64::INFINITY, 0)?;
    let (w, h) = lines_size(&lines);
    emu.cpu.q[0] = w.to_bits() as u128;
    emu.cpu.q[1] = h.to_bits() as u128;
    Ok(())
}

fn attributed_bounding_rect(emu: &mut Emu) -> Result<()> {
    let (s, options) = (cf_attributed(emu, emu.arg(0))?, emu.arg(2));
    let (w, h) = (d(emu, 0), d(emu, 1));
    let (width, height, max_lines) = if options & 1 != 0 { (w, h, 0) } else { (f64::INFINITY, f64::INFINITY, 1) };
    let (_, _, lines) = attributed_lines(emu, s, width, height, max_lines)?;
    let (uw, uh) = lines_size(&lines);
    ret_rect(emu, [0.0, 0.0, uw, uh]);
    Ok(())
}

fn draw_attributed(emu: &mut Emu, s: u64, rect: [f64; 4]) -> Result<()> {
    let (text, styles, lines) = attributed_lines(emu, s, rect[2], rect[3], 0)?;
    let mut glyphs = glyphs_of(&text, &styles, &lines, rect[0], rect[1], (0, text.len()));
    crate::cjk::assign(emu, &mut glyphs)?;
    crate::render::draw_glyphs(emu, glyphs);
    draw_decorations(emu, &text, &styles, &lines, rect[0], rect[1], (0, text.len()))
}

fn attributed_draw_at(emu: &mut Emu) -> Result<()> {
    let s = cf_attributed(emu, emu.arg(0))?;
    draw_attributed(emu, s, [d(emu, 0), d(emu, 1), f64::INFINITY, f64::INFINITY])
}

fn attributed_draw_in(emu: &mut Emu) -> Result<()> {
    let s = cf_attributed(emu, emu.arg(0))?;
    draw_attributed(emu, s, [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)])
}

fn string_draw_at(emu: &mut Emu) -> Result<()> {
    let s = attributed_from(emu, emu.arg(0), emu.arg(2))?;
    draw_attributed(emu, s, [d(emu, 0), d(emu, 1), f64::INFINITY, f64::INFINITY])
}

fn string_draw_in(emu: &mut Emu) -> Result<()> {
    let s = attributed_from(emu, emu.arg(0), emu.arg(2))?;
    draw_attributed(emu, s, [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)])
}

/// Lay out a layout manager's text storage in its container.
fn laid_out(emu: &mut Emu, lm: u64) -> Result<Laid> {
    let (storage, tc) = (emu.read_u64(lm + 8)?, emu.read_u64(lm + 16)?);
    let s = cf_attributed(emu, storage)?;
    let (mut w, mut h, mut padding, mut max_lines) = (f64::MAX, f64::MAX, 5.0, 0);
    if tc != 0 {
        (w, h, padding) = (f64_at(emu, tc + 8)?, f64_at(emu, tc + 16)?, f64_at(emu, tc + 24)?);
        max_lines = emu.read_u64(tc + 32)? as usize;
    }
    let mode = if tc != 0 { emu.read_u64(tc + 40)? } else { 0 };
    if s == 0 {
        return Ok(Laid { lines: Vec::new(), padding, len: 0, styles: Vec::new(), text: Vec::new() });
    }
    let (text, runs, styles) = attributed_runs(emu, s)?;
    let width = (w - 2.0 * padding).max(0.0);
    let mut cache: Vec<(char, TextFont, f64)> = Vec::new();
    let japanese = crate::cjk::japanese_spacing(emu)?;
    let mut measure = |ch: char, f: &TextFont| {
        if let Some(&(_, _, a)) = cache.iter().find(|(c, cf, _)| *c == ch && cf == f) {
            return a;
        }
        let a = advance_in(emu, ch, f);
        cache.push((ch, *f, a));
        a
    };
    let mut lines = layout(&text, &runs, &mut measure, japanese, width, h, max_lines);
    // NSLineBreakByTruncatingTail (4): the last line that fits, if text goes on past it, ends in "…".
    if mode == 4 {
        truncate_tail(&text, &runs, &mut lines, width, &mut measure);
    }
    align_lines(emu, s, &mut lines, width)?;
    Ok(Laid { lines, padding, len: text.len(), styles, text })
}

impl Laid {
    fn glyph_end(&self) -> usize {
        self.lines.last().map_or(0, |l| l.start + l.len)
    }
    fn used_width(&self) -> f64 {
        self.lines.iter().map(|l| l.width).fold(0.0, f64::max)
    }
    fn height(&self) -> f64 {
        self.lines.last().map_or(0.0, |l| l.y + l.height)
    }
    fn line_of(&self, glyph: usize) -> Option<&Line> {
        self.lines.iter().find(|l| glyph >= l.start && glyph < l.start + l.len).or_else(|| self.lines.last().filter(|l| glyph == l.start + l.len))
    }
    /// The line fragment rect (container width) and used rect of a line.
    fn rects(&self, l: &Line, container_w: f64) -> ([f64; 4], [f64; 4]) {
        let w = if container_w >= f64::MAX / 2.0 { self.used_width() + 2.0 * self.padding } else { container_w };
        ([0.0, l.y, w, l.height], [self.padding, l.y, l.width, l.height])
    }
}

fn container_width(emu: &Emu, lm: u64) -> Result<f64> {
    let tc = emu.read_u64(lm + 16)?;
    if tc == 0 { Ok(f64::MAX) } else { f64_at(emu, tc + 8) }
}

fn ret_range(emu: &mut Emu, loc: u64, len: u64) {
    emu.cpu.x[0] = loc;
    emu.cpu.x[1] = len;
}

fn write_range(emu: &mut Emu, p: u64, loc: u64, len: u64) -> Result<()> {
    if p != 0 {
        emu.write_u64(p, loc)?;
        emu.write_u64(p + 8, len)?;
    }
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

// ---- NSTextContainer ----

fn tc_init_with_size(emu: &mut Emu) -> Result<()> {
    let tc = emu.arg(0);
    let (w, h) = (d(emu, 0), d(emu, 1));
    emu.write_u64(tc + 8, w.to_bits())?;
    emu.write_u64(tc + 16, h.to_bits())?;
    emu.write_u64(tc + 24, 5f64.to_bits())?;
    emu.ret(tc);
    Ok(())
}

fn tc_init(emu: &mut Emu) -> Result<()> {
    emu.cpu.q[0] = f64::MAX.to_bits() as u128;
    emu.cpu.q[1] = f64::MAX.to_bits() as u128;
    tc_init_with_size(emu)
}

fn tc_size(emu: &mut Emu) -> Result<()> {
    let tc = emu.arg(0);
    let (w, h) = (f64_at(emu, tc + 8)?, f64_at(emu, tc + 16)?);
    emu.cpu.q[0] = w.to_bits() as u128;
    emu.cpu.q[1] = h.to_bits() as u128;
    Ok(())
}

fn tc_set_size(emu: &mut Emu) -> Result<()> {
    let tc = emu.arg(0);
    let (w, h) = (d(emu, 0), d(emu, 1));
    emu.write_u64(tc + 8, w.to_bits())?;
    emu.write_u64(tc + 16, h.to_bits())?;
    Ok(())
}

fn tc_padding(emu: &mut Emu) -> Result<()> {
    let p = f64_at(emu, emu.arg(0) + 24)?;
    ret_f64(emu, p);
    Ok(())
}

fn tc_set_padding(emu: &mut Emu) -> Result<()> {
    let p = d(emu, 0);
    emu.write_u64(emu.arg(0) + 24, p.to_bits())
}

fn field_get(emu: &mut Emu, off: u64) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + off)?;
    emu.ret(v);
    Ok(())
}

fn field_set(emu: &mut Emu, off: u64) -> Result<()> {
    emu.write_u64(emu.arg(0) + off, emu.arg(2))
}

// ---- NSLayoutManager ----

fn lm_add_container(emu: &mut Emu) -> Result<()> {
    let (lm, tc) = (emu.arg(0), emu.arg(2));
    emu.write_u64(lm + 16, tc)?;
    if tc != 0 {
        emu.write_u64(tc + 48, lm)?;
    }
    Ok(())
}

fn lm_containers(emu: &mut Emu) -> Result<()> {
    let tc = emu.read_u64(emu.arg(0) + 16)?;
    let objs: Vec<u64> = if tc == 0 { vec![] } else { vec![tc] };
    let a = foundation::array_from(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

fn lm_used_rect(emu: &mut Emu) -> Result<()> {
    let l = laid_out(emu, emu.arg(0))?;
    ret_rect(emu, [l.padding, 0.0, l.used_width(), l.height()]);
    Ok(())
}

/// `-extraLineFragmentRect` / `-extraLineFragmentUsedRect`: the empty line
/// after a trailing newline (where the caret goes), as tall as the text's
/// last line; zero when the text doesn't end in one.
fn lm_extra_line(emu: &mut Emu, used: bool) -> Result<()> {
    let lm = emu.arg(0);
    let l = laid_out(emu, lm)?;
    let ends_in_newline = l.text.last().is_some_and(|&u| char::from_u32(u as u32).is_some_and(is_newline));
    let r = match l.lines.last().filter(|_| ends_in_newline) {
        Some(last) => {
            let y = last.y + last.height;
            if used { [l.padding, y, 0.0, last.height] } else { [0.0, y, container_width(emu, lm)?.min(f64::MAX / 2.0), last.height] }
        }
        None => [0.0; 4],
    };
    ret_rect(emu, r);
    Ok(())
}

fn lm_glyph_range_for_container(emu: &mut Emu) -> Result<()> {
    let l = laid_out(emu, emu.arg(0))?;
    ret_range(emu, 0, l.glyph_end() as u64);
    Ok(())
}

fn lm_number_of_glyphs(emu: &mut Emu) -> Result<()> {
    let l = laid_out(emu, emu.arg(0))?;
    emu.ret(l.len as u64);
    Ok(())
}

/// `-glyphRangeForCharacterRange:actualCharacterRange:` and the reverse: identity.
fn lm_identity_range(emu: &mut Emu) -> Result<()> {
    let (loc, len, actual) = (emu.arg(2), emu.arg(3), emu.arg(4));
    write_range(emu, actual, loc, len)?;
    ret_range(emu, loc, len);
    Ok(())
}

fn lm_identity_index(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(2);
    emu.ret(i);
    Ok(())
}

/// `-enumerateLineFragmentsForGlyphRange:usingBlock:` — block(rect, usedRect, container, glyphRange, stop).
fn lm_enumerate_lines(emu: &mut Emu) -> Result<()> {
    let (lm, loc, len, block) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3) as usize, emu.arg(4));
    let l = laid_out(emu, lm)?;
    let cw = container_width(emu, lm)?;
    let tc = emu.read_u64(lm + 16)?;
    let invoke = emu.read_u64(block + 16)?;
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    for line in &l.lines {
        if line.start + line.len <= loc || line.start >= loc + len {
            continue;
        }
        let (rect, used) = l.rects(line, cw);
        let v: Vec<u128> = rect.iter().chain(used.iter()).map(|f| f.to_bits() as u128).collect();
        emu.call_abi(invoke, &[block, tc, line.start as u64, line.len as u64, stop], &v, &[], None)?;
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    emu.heap.free(&emu.p.memory, stop)?;
    Ok(())
}

fn lm_line_rect(emu: &mut Emu, used: bool) -> Result<()> {
    let (lm, glyph, eff) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3));
    let l = laid_out(emu, lm)?;
    let cw = container_width(emu, lm)?;
    match l.line_of(glyph) {
        Some(line) => {
            let (r, u) = l.rects(line, cw);
            write_range(emu, eff, line.start as u64, line.len as u64)?;
            ret_rect(emu, if used { u } else { r });
        }
        None => {
            write_range(emu, eff, 0, 0)?;
            ret_rect(emu, [0.0; 4]);
        }
    }
    Ok(())
}

/// `-boundingRectForGlyphRange:inTextContainer:`
fn lm_bounding_rect(emu: &mut Emu) -> Result<()> {
    let (lm, loc, len) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3) as usize);
    let l = laid_out(emu, lm)?;
    let mut r: Option<[f64; 4]> = None;
    for line in &l.lines {
        let (s, e) = (loc.max(line.start), (loc + len).min(line.start + line.len));
        if s >= e && !(len == 0 && loc == line.start) {
            continue;
        }
        let x0 = line.xs[s - line.start] + l.padding;
        let x1 = line.xs[e.max(s) - line.start] + l.padding;
        let lr = [x0, line.y, x1 - x0, line.height];
        r = Some(match r {
            None => lr,
            Some(a) => {
                let (minx, miny) = (a[0].min(lr[0]), a[1].min(lr[1]));
                let (maxx, maxy) = ((a[0] + a[2]).max(lr[0] + lr[2]), (a[1] + a[3]).max(lr[1] + lr[3]));
                [minx, miny, maxx - minx, maxy - miny]
            }
        });
    }
    ret_rect(emu, r.unwrap_or([0.0; 4]));
    Ok(())
}

/// `-locationForGlyphAtIndex:` → baseline point within the line fragment.
fn lm_location(emu: &mut Emu) -> Result<()> {
    let (lm, glyph) = (emu.arg(0), emu.arg(2) as usize);
    let l = laid_out(emu, lm)?;
    let (x, y) = l.line_of(glyph).map_or((0.0, 0.0), |line| (line.xs[glyph - line.start] + l.padding, line.ascent));
    emu.cpu.q[0] = x.to_bits() as u128;
    emu.cpu.q[1] = y.to_bits() as u128;
    Ok(())
}

/// `-glyphIndexForPoint:inTextContainer:fractionOfDistanceThroughGlyph:` (point in d0/d1).
fn lm_glyph_at_point(emu: &mut Emu) -> Result<()> {
    let (lm, frac) = (emu.arg(0), emu.arg(3));
    let (px, py) = (d(emu, 0), d(emu, 1));
    let l = laid_out(emu, lm)?;
    let line = l.lines.iter().find(|ln| py < ln.y + ln.height).or(l.lines.last());
    let (mut glyph, mut f) = (0usize, 0.0);
    if let Some(line) = line {
        let x = px - l.padding;
        glyph = line.start + line.len.saturating_sub(1);
        for k in 0..line.len {
            let (a, b) = (line.xs[k], line.xs[k + 1]);
            if x < b {
                glyph = line.start + k;
                f = if b > a { ((x - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
                break;
            }
        }
        if x >= line.xs[line.len] {
            f = 1.0;
        }
    }
    if frac != 0 {
        emu.write_u64(frac, f.to_bits())?;
    }
    emu.ret(glyph as u64);
    Ok(())
}

/// `-drawGlyphsForGlyphRange:atPoint:` (range x2/x3, origin d0/d1): into the current CGContext.
fn lm_draw_glyphs(emu: &mut Emu) -> Result<()> {
    let (lm, loc, len) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3) as usize);
    let (ox, oy) = (d(emu, 0), d(emu, 1));
    let l = laid_out(emu, lm)?;
    let mut glyphs = glyphs_of(&l.text, &l.styles, &l.lines, ox + l.padding, oy, (loc, loc + len));
    crate::cjk::assign(emu, &mut glyphs)?;
    crate::render::draw_glyphs(emu, glyphs);
    draw_decorations(emu, &l.text, &l.styles, &l.lines, ox + l.padding, oy, (loc, loc + len))
}

fn lm_truncated(emu: &mut Emu) -> Result<()> {
    ret_range(emu, NOT_FOUND, 0);
    Ok(())
}

fn nothing(_: &mut Emu) -> Result<()> {
    Ok(())
}

// ---- NSTextStorage ----

fn ts_init_with(emu: &mut Emu, string: u64, attrs: u64, attributed: u64) -> Result<()> {
    let ts = emu.arg(0);
    let inner = if attributed != 0 {
        let a = cf_attributed(emu, attributed)?;
        cf(emu, "_CFAttributedStringCreateMutableCopy", &[0, 0, a])?
    } else {
        let s = if string == 0 { foundation::new_nsstring(emu, "")? } else { foundation::as_cf_string(emu, string)? };
        let attrs = foundation::as_cf_dict(emu, attrs)?;
        let a = cf(emu, "_CFAttributedStringCreate", &[0, s, attrs])?;
        cf(emu, "_CFAttributedStringCreateMutableCopy", &[0, 0, a])?
    };
    emu.write_u64(ts + 8, inner)?;
    emu.ret(ts);
    Ok(())
}

fn ts_add_layout_manager(emu: &mut Emu) -> Result<()> {
    let (ts, lm) = (emu.arg(0), emu.arg(2));
    emu.write_u64(ts + 16, lm)?;
    if lm != 0 {
        emu.write_u64(lm + 8, ts)?;
    }
    Ok(())
}

fn ts_layout_managers(emu: &mut Emu) -> Result<()> {
    let lm = emu.read_u64(emu.arg(0) + 16)?;
    let objs: Vec<u64> = if lm == 0 { vec![] } else { vec![lm] };
    let a = foundation::array_from(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

/// Everything else goes to the CF attributed string.
fn ts_forward(emu: &mut Emu) -> Result<()> {
    let inner = emu.read_u64(emu.arg(0) + 8)?;
    emu.ret(inner);
    Ok(())
}

fn ts_inner_send(emu: &mut Emu, sel: &str) -> Result<()> {
    let inner = emu.read_u64(emu.arg(0) + 8)?;
    let args: Vec<u64> = (2..8).map(|i| emu.arg(i)).collect();
    let r = objc::send(emu, inner, sel, &args)?;
    emu.ret(r);
    Ok(())
}

/// An attributes dictionary (a CFDictionary, or 0), read by key name.
struct Attrs(u64);

impl Attrs {
    fn get(&self, emu: &mut Emu, key: &str) -> Result<u64> {
        if self.0 == 0 {
            return Ok(0);
        }
        let k = foundation::new_nsstring(emu, key)?;
        cf(emu, "_CFDictionaryGetValue", &[self.0, k])
    }

    fn number(&self, emu: &mut Emu, key: &str) -> Result<Option<f64>> {
        let n = self.get(emu, key)?;
        if n == 0 {
            return Ok(None);
        }
        let (i, f) = foundation::number_value(emu, n)?;
        Ok(Some(if foundation::number_is_float(emu, n)? { f } else { i as f64 }))
    }

    fn color(&self, emu: &mut Emu, key: &str) -> Result<Option<[f64; 4]>> {
        let c = self.get(emu, key)?;
        Ok(emu.ui.views.colors.get(&c).copied().filter(|_| c != 0))
    }
}

/// An NSShadow as drawn (UIKit's defaults where unset: 3 pt down, no blur, black at a third).
fn shadow_of(emu: &mut Emu, shadow: u64) -> crate::render::TextShadow {
    let get = |k| crate::objc::bag_get(emu, shadow, k).map(f64::from_bits);
    let (w, h, blur) = (get("pommeShadowWidth"), get("pommeShadowHeight"), get("pommeShadowBlur"));
    let color = crate::objc::bag_get(emu, shadow, "pommeShadowColor").and_then(|c| emu.ui.views.colors.get(&c).copied()).unwrap_or([0.0, 0.0, 0.0, 1.0 / 3.0]);
    crate::render::TextShadow { offset: [w.unwrap_or(0.0), h.unwrap_or(-3.0)], blur: blur.unwrap_or(0.0), color }
}

/// Underline and strikethrough positions and thickness for a font, in points
/// (underline center below the baseline, its thickness; strikethrough center
/// above it, its thickness): the face's own `post` and `OS/2` metrics.
fn decoration_metrics(emu: &mut Emu, f: &TextFont) -> (f64, f64, f64, f64) {
    let bytes = if f.face != 0 { crate::coretext::face_data(emu, f.face).map(|(b, i, _)| (b, i)) } else { font_bytes(emu).map(|b| (b, 0)) };
    if let Some((bytes, index)) = bytes
        && let Ok(face) = ttf_parser::Face::parse(&bytes, index)
    {
        let k = f.size / face.units_per_em() as f64;
        let u = face.underline_metrics();
        let st = face.strikeout_metrics();
        let thick = u.map_or(f.size / 18.0, |m| m.thickness as f64 * k);
        return (
            u.map_or(f.size * 0.11, |m| -(m.position as f64) * k + thick / 2.0),
            thick,
            st.map_or(f.size * 0.28, |m| m.position as f64 * k - m.thickness as f64 * k / 2.0),
            st.map_or(thick, |m| m.thickness as f64 * k),
        );
    }
    (f.size * 0.11, f.size / 18.0, f.size * 0.28, f.size / 18.0)
}

/// Underlines and strikethroughs (NSUnderlineStyle: single, thick, double)
/// under and through the glyphs of `range`, each line from its first
/// character to its last (not a line's trailing spaces), in the current context.
fn draw_decorations(emu: &mut Emu, text: &[u16], styles: &Styles, lines: &[Line], x: f64, y: f64, range: (usize, usize)) -> Result<()> {
    let mut fills: Vec<([f64; 4], Vec<[f64; 4]>)> = Vec::new();
    for st in styles.iter().filter(|s| s.underline.is_some() || s.strike.is_some()) {
        let (upos, uthick, spos, sthick) = decoration_metrics(emu, &st.font);
        for line in lines {
            let mut e = (line.start + line.len).min(st.end).min(range.1);
            let s = line.start.max(st.start).max(range.0);
            while e > s && text.get(e - 1).is_some_and(|&u| char::from_u32(u as u32).is_some_and(char::is_whitespace)) {
                e -= 1;
            }
            if e <= s {
                continue;
            }
            let (x0, x1) = (x + line.xs[s - line.start], x + line.xs[e - line.start]);
            let baseline = y + line.y + line.ascent - st.baseline;
            for (deco, center, thick) in [(st.underline, baseline + upos, uthick), (st.strike, baseline - spos, sthick)] {
                let Some(deco) = deco else { continue };
                let t = if deco.style & 0xff == 2 { thick * 2.0 } else { thick };
                let mut rects = vec![[x0, center - t / 2.0, x1 - x0, t]];
                // Double: a second line one gap (a thickness) below.
                if deco.style & 0xff == 9 {
                    rects = vec![[x0, center - t, x1 - x0, t], [x0, center + t, x1 - x0, t]];
                }
                match fills.iter_mut().find(|(c, _)| *c == deco.color) {
                    Some((_, r)) => r.extend(rects),
                    None => fills.push((deco.color, rects)),
                }
            }
        }
    }
    for (color, rects) in fills {
        crate::render::fill_rects_in_current(emu, &rects, color)?;
    }
    Ok(())
}

/// NSShadow: its offset (a CGSize), blur radius and color, read by the text drawing.
fn shadow_methods() -> [(&'static str, bool, &'static str, HostFn); 6] {
    [
        ("NSShadow", false, "setShadowOffset:", |e| {
            let (o, w, h) = (e.arg(0), d(e, 0), d(e, 1));
            crate::objc::bag_set(e, o, "pommeShadowWidth", w.to_bits());
            crate::objc::bag_set(e, o, "pommeShadowHeight", h.to_bits());
            Ok(())
        }),
        ("NSShadow", false, "shadowOffset", |e| {
            let s = shadow_of(e, e.arg(0));
            e.cpu.q[0] = s.offset[0].to_bits() as u128;
            e.cpu.q[1] = s.offset[1].to_bits() as u128;
            Ok(())
        }),
        ("NSShadow", false, "setShadowBlurRadius:", |e| {
            let (o, r) = (e.arg(0), d(e, 0));
            crate::objc::bag_set(e, o, "pommeShadowBlur", r.to_bits());
            Ok(())
        }),
        ("NSShadow", false, "shadowBlurRadius", |e| {
            let r = shadow_of(e, e.arg(0)).blur;
            e.cpu.q[0] = r.to_bits() as u128;
            Ok(())
        }),
        ("NSShadow", false, "setShadowColor:", |e| {
            let (o, c) = (e.arg(0), e.arg(2));
            if c != 0 {
                crate::objc::send(e, c, "retain", &[])?;
            }
            crate::objc::bag_set(e, o, "pommeShadowColor", c);
            Ok(())
        }),
        ("NSShadow", false, "shadowColor", |e| {
            let c = crate::objc::bag_get(e, e.arg(0), "pommeShadowColor").unwrap_or(0);
            e.ret(c);
            Ok(())
        }),
    ]
}

pub(crate) fn methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    let mut m = METHODS.to_vec();
    m.extend(shadow_methods());
    m
}

const TC: &str = "NSTextContainer";
const LM: &str = "NSLayoutManager";
const TS: &str = "NSTextStorage";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (TC, false, "init", tc_init),
    (TC, false, "initWithSize:", tc_init_with_size),
    (TC, false, "size", tc_size),
    ("NSString", false, "sizeWithAttributes:", size_with_attributes),
    ("NSString", false, "boundingRectWithSize:options:attributes:context:", bounding_rect_with_size),
    ("NSString", false, "drawAtPoint:withAttributes:", string_draw_at),
    ("NSString", false, "drawInRect:withAttributes:", string_draw_in),
    ("NSAttributedString", false, "size", attributed_size),
    ("NSAttributedString", false, "boundingRectWithSize:options:context:", attributed_bounding_rect),
    ("NSAttributedString", false, "drawAtPoint:", attributed_draw_at),
    ("NSAttributedString", false, "drawInRect:", attributed_draw_in),
    ("NSAttributedString", false, "drawWithRect:options:context:", attributed_draw_in),
    (TC, false, "setSize:", tc_set_size),
    (TC, false, "lineFragmentPadding", tc_padding),
    (TC, false, "setLineFragmentPadding:", tc_set_padding),
    (TC, false, "maximumNumberOfLines", |e| field_get(e, 32)),
    (TC, false, "setMaximumNumberOfLines:", |e| field_set(e, 32)),
    (TC, false, "lineBreakMode", |e| field_get(e, 40)),
    (TC, false, "setLineBreakMode:", |e| field_set(e, 40)),
    (TC, false, "layoutManager", |e| field_get(e, 48)),
    (LM, false, "addTextContainer:", lm_add_container),
    (LM, false, "textContainers", lm_containers),
    (LM, false, "textStorage", |e| field_get(e, 8)),
    (LM, false, "setTextStorage:", |e| field_set(e, 8)),
    (LM, false, "ensureLayoutForTextContainer:", nothing),
    (LM, false, "ensureLayoutForGlyphRange:", nothing),
    (LM, false, "ensureLayoutForCharacterRange:", nothing),
    (LM, false, "invalidateLayoutForCharacterRange:actualCharacterRange:", nothing),
    (LM, false, "invalidateDisplayForCharacterRange:", nothing),
    (LM, false, "usedRectForTextContainer:", lm_used_rect),
    (LM, false, "extraLineFragmentRect", |e| lm_extra_line(e, false)),
    (LM, false, "extraLineFragmentUsedRect", |e| lm_extra_line(e, true)),
    (LM, false, "glyphRangeForTextContainer:", lm_glyph_range_for_container),
    (LM, false, "numberOfGlyphs", lm_number_of_glyphs),
    (LM, false, "glyphRangeForCharacterRange:actualCharacterRange:", lm_identity_range),
    (LM, false, "characterRangeForGlyphRange:actualGlyphRange:", lm_identity_range),
    (LM, false, "glyphIndexForCharacterAtIndex:", lm_identity_index),
    (LM, false, "characterIndexForGlyphAtIndex:", lm_identity_index),
    (LM, false, "enumerateLineFragmentsForGlyphRange:usingBlock:", lm_enumerate_lines),
    (LM, false, "lineFragmentRectForGlyphAtIndex:effectiveRange:", |e| lm_line_rect(e, false)),
    (LM, false, "lineFragmentUsedRectForGlyphAtIndex:effectiveRange:", |e| lm_line_rect(e, true)),
    (LM, false, "boundingRectForGlyphRange:inTextContainer:", lm_bounding_rect),
    (LM, false, "locationForGlyphAtIndex:", lm_location),
    (LM, false, "glyphIndexForPoint:inTextContainer:fractionOfDistanceThroughGlyph:", lm_glyph_at_point),
    (LM, false, "glyphIndexForPoint:inTextContainer:", |e| {
        e.cpu.x[3] = 0;
        lm_glyph_at_point(e)
    }),
    (LM, false, "truncatedGlyphRangeInLineFragmentForGlyphAtIndex:", lm_truncated),
    (LM, false, "drawGlyphsForGlyphRange:atPoint:", lm_draw_glyphs),
    (LM, false, "drawBackgroundForGlyphRange:atPoint:", nothing),
    (TS, false, "init", |e| ts_init_with(e, 0, 0, 0)),
    (TS, false, "initWithString:", |e| {
        let s = e.arg(2);
        ts_init_with(e, s, 0, 0)
    }),
    (TS, false, "initWithString:attributes:", |e| {
        let (s, a) = (e.arg(2), e.arg(3));
        ts_init_with(e, s, a, 0)
    }),
    (TS, false, "initWithAttributedString:", |e| {
        let a = e.arg(2);
        ts_init_with(e, 0, 0, a)
    }),
    (TS, false, "addLayoutManager:", ts_add_layout_manager),
    (TS, false, "removeLayoutManager:", |e| e.write_u64(e.arg(0) + 16, 0)),
    (TS, false, "layoutManagers", ts_layout_managers),
    (TS, false, "forwardingTargetForSelector:", ts_forward),
    (TS, false, "string", |e| ts_inner_send(e, "string")),
    (TS, false, "length", |e| ts_inner_send(e, "length")),
    (TS, false, "copy", |e| ts_inner_send(e, "copy")),
    (TS, false, "mutableCopy", |e| ts_inner_send(e, "mutableCopy")),
    (TS, false, "isEqual:", |e| ts_inner_send(e, "isEqual:")),
    (TS, false, "hash", |e| ts_inner_send(e, "hash")),
    (TS, false, "description", |e| ts_inner_send(e, "description")),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn lay(s: &str, width: f64) -> Vec<(String, f64)> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let runs = [Run { len: units.len(), font: TextFont::system(10.0, 0.0), line_height: 10.0 * LINE_HEIGHT, ascent: 10.0 * ASCENT, kern: 0.0, para: [0.0; 2] }];
        layout(&units, &runs, |c, _| if c == ' ' { 5.0 } else { 10.0 }, false, width, f64::MAX, 0)
            .iter()
            .map(|l| (String::from_utf16_lossy(&units[l.start..l.start + l.len]), l.width))
            .collect()
    }

    #[test]
    fn wraps_at_spaces() {
        assert_eq!(lay("ab cd ef", 1e9), vec![("ab cd ef".into(), 70.0)]);
        // "ab cd " fits in 55 (trailing space hangs); "ef" moves down.
        assert_eq!(lay("ab cd ef", 55.0), vec![("ab cd ".into(), 45.0), ("ef".into(), 20.0)]);
        // A word longer than the line breaks mid-word.
        assert_eq!(lay("abcdef", 35.0), vec![("abc".into(), 30.0), ("def".into(), 30.0)]);
        assert_eq!(lay("a\nb", 1e9), vec![("a\n".into(), 10.0), ("b".into(), 10.0)]);
        assert!(lay("", 100.0).is_empty());
    }

    #[test]
    fn line_metrics() {
        let units: Vec<u16> = "hi".encode_utf16().collect();
        let l = layout(&units, &[Run { len: 2, font: TextFont::system(20.0, 0.0), line_height: 20.0 * LINE_HEIGHT, ascent: 20.0 * ASCENT, kern: 0.0, para: [0.0; 2] }], |_, _| 7.0, false, 1e9, f64::MAX, 0);
        assert_eq!(l.len(), 1);
        assert!((l[0].height - 20.0 * LINE_HEIGHT).abs() < 1e-9);
        assert_eq!(l[0].xs, vec![0.0, 7.0, 14.0]);
    }

    #[test]
    fn weights() {
        assert_eq!(ot_weight(0.0), 400.0);
        assert_eq!(ot_weight(0.4), 700.0);
        assert_eq!(ot_weight(-2.0), 100.0);
    }
}
