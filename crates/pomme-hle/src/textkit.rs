//! TextKit 1: NSTextStorage, NSLayoutManager and NSTextContainer, enough
//! for apps (React Native in particular) to measure and hit-test text.
//!
//! Glyph advances come from a real font: Inter (open source, metrics close to
//! SF Pro), read from the sysroot with ttf-parser at the requested weight and
//! optical size. Every font is laid out with it. Vertical metrics stay
//! UIFont's (SF Pro proportions), so measured line heights match iOS.
//!
//! Layout is greedy: break after whitespace or a hyphen, or mid-word when a
//! word is wider than the line. One glyph per UTF-16 unit, so glyph indexes
//! and ranges are character indexes and ranges. No kerning or ligatures yet.
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

const LINE_HEIGHT: f64 = 1.193;
const ASCENT: f64 = 0.952;
const DEFAULT_SIZE: f64 = 12.0; // TextKit's default (Helvetica 12)
const NOT_FOUND: u64 = i64::MAX as u64;

// ---- fonts ----

#[derive(Default)]
pub struct State {
    /// Inter's bytes, once looked for (None inside: not in the sysroot).
    font: Option<Option<Vec<u8>>>,
    /// (char, weight, optical size) → advance in em.
    advances: HashMap<(char, u16, u16), f64>,
}

/// The sysroot, found from where CoreFoundation was loaded.
fn font_bytes(emu: &mut Emu) -> Option<&[u8]> {
    if emu.text.font.is_none() {
        let path = emu
            .p
            .images
            .iter()
            .find(|i| i.path.ends_with("System/Library/Frameworks/CoreFoundation.framework/CoreFoundation"))
            .and_then(|i| i.path.ancestors().nth(5).map(|root| root.join("System/Library/Fonts/Inter.ttf")));
        emu.text.font = Some(path.and_then(|p| std::fs::read(p).ok()));
    }
    emu.text.font.as_ref().unwrap().as_deref()
}

/// UIFontWeight (-0.8 … 0.62) → OpenType `wght` (100 … 900).
fn ot_weight(w: f64) -> f64 {
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

/// Advance width of `ch` in points.
fn advance(emu: &mut Emu, ch: char, size: f64, weight: f64) -> f64 {
    let wght = ot_weight(weight).round() as u16;
    let opsz = size.clamp(14.0, 32.0).round() as u16;
    let key = (ch, wght, opsz);
    if let Some(&em) = emu.text.advances.get(&key) {
        return em * size;
    }
    let em = match font_bytes(emu).and_then(|d| ttf_parser::Face::parse(d, 0).ok()) {
        Some(mut face) => {
            face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), wght as f32);
            face.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), opsz as f32);
            let upem = face.units_per_em() as f64;
            let gid = face.glyph_index(ch).or_else(|| face.glyph_index('\u{fffd}'));
            gid.and_then(|g| face.glyph_hor_advance(g)).map_or(0.55, |a| a as f64 / upem)
        }
        None if ch.is_whitespace() => 0.25,
        None => 0.55,
    };
    emu.text.advances.insert(key, em);
    em * size
}

// ---- layout ----

/// A stretch of text in one font, in UTF-16 units.
#[derive(Clone, Copy, Debug)]
pub struct Run {
    pub len: usize,
    pub size: f64,
    pub weight: f64,
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
}

struct Item {
    unit: usize,
    units: usize,
    ch: char,
    adv: f64,
    size: f64,
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
            if x + it.adv > width + 1e-9 && j > start && !it.ch.is_whitespace() {
                end = last_break.unwrap_or(j);
                break;
            }
            x += it.adv;
            if it.ch.is_whitespace() || it.ch == '-' {
                last_break = Some(j + 1);
            }
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
        let size = line_items.iter().map(|it| it.size).fold(0.0, f64::max);
        let (lh, asc) = (size * LINE_HEIGHT, size * ASCENT);
        if !lines.is_empty() && y + lh > height + 1e-9 {
            break;
        }
        let unit = line_items[0].unit;
        let next = items.get(end).map_or(total_units, |it| it.unit);
        lines.push(Line { start: unit, len: next - unit, y, height: lh, ascent: asc, width, xs });
        y += lh;
        i = end;
        if max_lines != 0 && lines.len() == max_lines {
            break;
        }
    }
    lines
}

/// Lay out UTF-16 text with per-run fonts; `adv(ch, size, weight)` measures a character.
pub fn layout(text: &[u16], runs: &[Run], mut adv: impl FnMut(char, f64, f64) -> f64, width: f64, height: f64, max_lines: usize) -> Vec<Line> {
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
        let (size, weight) = run.map_or((DEFAULT_SIZE, 0.0), |r| (r.size, r.weight));
        let units = ch.len_utf16();
        let a = if is_newline(ch) { 0.0 } else { adv(ch, size, weight) };
        items.push(Item { unit, units, ch, adv: a, size });
        unit += units;
    }
    break_lines(&items, text.len(), width, height, max_lines)
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

/// Font size and weight of a UIFont (other fonts: TextKit's default).
fn font_info(emu: &mut Emu, font: u64) -> Result<(f64, f64)> {
    if font == 0 || foundation::is_cf(emu, font) || class_name_of(emu, font) != "UIFont" {
        return Ok((DEFAULT_SIZE, 0.0));
    }
    Ok((f64_at(emu, font + 16)?, f64_at(emu, font + 24)?))
}

struct Laid {
    lines: Vec<Line>,
    padding: f64,
    len: usize,
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
    if s == 0 {
        return Ok(Laid { lines: Vec::new(), padding, len: 0 });
    }
    let string = cf(emu, "_CFAttributedStringGetString", &[s])?;
    let text: Vec<u16> = foundation::nsstring(emu, string)?.encode_utf16().collect();
    let font_key = foundation::new_nsstring(emu, "NSFontAttributeName")?;
    let range = emu.alloc(16)?;
    let mut runs = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let attrs = cf(emu, "_CFAttributedStringGetAttributes", &[s, i as u64, range])?;
        let (loc, n) = (emu.read_u64(range)? as usize, emu.read_u64(range + 8)? as usize);
        let font = if attrs == 0 { 0 } else { cf(emu, "_CFDictionaryGetValue", &[attrs, font_key])? };
        let (size, weight) = font_info(emu, font)?;
        let end = (loc + n).max(i + 1).min(text.len());
        runs.push(Run { len: end - i, size, weight });
        i = end;
    }
    emu.heap.free(&emu.p.memory, range)?;
    let width = (w - 2.0 * padding).max(0.0);
    let mut cache: Vec<(char, f64, f64, f64)> = Vec::new();
    let mut measure = |ch: char, size: f64, weight: f64| {
        if let Some(&(_, _, _, a)) = cache.iter().find(|(c, s, w, _)| *c == ch && *s == size && *w == weight) {
            return a;
        }
        let a = advance(emu, ch, size, weight);
        cache.push((ch, size, weight, a));
        a
    };
    let lines = layout(&text, &runs, &mut measure, width, h, max_lines);
    Ok(Laid { lines, padding, len: text.len() })
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

const TC: &str = "NSTextContainer";
const LM: &str = "NSLayoutManager";
const TS: &str = "NSTextStorage";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (TC, false, "init", tc_init),
    (TC, false, "initWithSize:", tc_init_with_size),
    (TC, false, "size", tc_size),
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
    (LM, false, "drawGlyphsForGlyphRange:atPoint:", nothing),
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
        let runs = [Run { len: units.len(), size: 10.0, weight: 0.0 }];
        layout(&units, &runs, |c, _, _| if c == ' ' { 5.0 } else { 10.0 }, width, f64::MAX, 0)
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
        let l = layout(&units, &[Run { len: 2, size: 20.0, weight: 0.0 }], |_, _, _| 7.0, 1e9, f64::MAX, 0);
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
