//! CoreText's line layout: CTFramesetter, CTFrame, CTLine and CTRun, as apps
//! use them to lay out and measure attributed text. The lines come from
//! TextKit's layout (textkit.rs), as UILabel and NSString drawing set
//! text; each line is one run.
//!
//! Coordinates are CoreText's: a frame's line origins are on the baselines,
//! from the frame's bottom left, y up; a run's glyph positions are from its
//! line's origin.

use crate::textkit::Line;
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Clone)]
struct Frame {
    /// The frame's rectangle (from its path's bounds).
    rect: [f64; 4],
    lines: Vec<Line>,
    /// The CTLine objects, made on first use.
    line_objects: Vec<u64>,
}

#[derive(Clone, Default)]
pub struct State {
    /// CTFramesetter → its attributed string.
    setters: HashMap<u64, u64>,
    frames: HashMap<u64, Frame>,
    /// CTLine → its line (and its run object, made on first use).
    lines: HashMap<u64, (Line, u64)>,
    /// CTRun → its line.
    runs: HashMap<u64, Line>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = crate::objc::host_class(emu, class)?;
    crate::objc::create_instance(emu, cls, 0)
}

/// `CTFramesetterCreateWithAttributedString(string)`
fn create_framesetter(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let s = crate::textkit::cf_attributed(emu, s)?;
    crate::objc::send(emu, s, "retain", &[])?;
    let o = new_object(emu, "CTFramesetter")?;
    emu.ctframes.setters.insert(o, s);
    ret(emu, o)
}

/// The attributed string's lines at `width` × `height` (from `start`, for `len` units; 0: to the end).
fn lines_of(emu: &mut Emu, setter: u64, start: usize, len: usize, width: f64, height: f64) -> Result<Vec<Line>> {
    let Some(&s) = emu.ctframes.setters.get(&setter) else { return Ok(Vec::new()) };
    let (text, _, mut lines) = crate::textkit::attributed_lines(emu, s, width, height, 0)?;
    let end = if len == 0 { text.len() } else { (start + len).min(text.len()) };
    lines.retain(|l| l.start >= start && l.start < end.max(start + 1));
    Ok(lines)
}

/// `CTFramesetterCreateFrame(setter, range, path, attributes)`
fn create_frame(emu: &mut Emu) -> Result<()> {
    let (setter, start, len, path) = (emu.arg(0), emu.arg(1) as usize, emu.arg(2) as usize, emu.arg(3));
    let rect = crate::render::path_bounds(&crate::render::path_els(emu, path));
    let lines = lines_of(emu, setter, start, len, rect[2], rect[3])?;
    let o = new_object(emu, "CTFrame")?;
    emu.ctframes.frames.insert(o, Frame { rect, lines, line_objects: Vec::new() });
    ret(emu, o)
}

/// `CTFramesetterSuggestFrameSizeWithConstraints(setter, range, attributes, constraints, fitRange)`
fn suggest_size(emu: &mut Emu) -> Result<()> {
    let (setter, start, len, fit) = (emu.arg(0), emu.arg(1) as usize, emu.arg(2) as usize, emu.arg(4));
    let (w, h) = (d(emu, 0), d(emu, 1));
    let lines = lines_of(emu, setter, start, len, if w > 0.0 { w } else { f64::INFINITY }, if h > 0.0 { h } else { f64::INFINITY })?;
    let width = lines.iter().map(|l| l.width).fold(0.0, f64::max);
    let height = lines.last().map_or(0.0, |l| l.y + l.height);
    if fit != 0 {
        let (s, e) = (lines.first().map_or(start, |l| l.start), lines.last().map_or(start, |l| l.start + l.len));
        emu.write_u64(fit, s as u64)?;
        emu.write_u64(fit + 8, (e - s) as u64)?;
    }
    emu.cpu.q[0] = width.to_bits() as u128;
    emu.cpu.q[1] = height.to_bits() as u128;
    Ok(())
}

/// A frame's CTLine objects, made on first use.
fn frame_lines(emu: &mut Emu, frame: u64) -> Result<Vec<u64>> {
    let Some(f) = emu.ctframes.frames.get(&frame).cloned() else { return Ok(Vec::new()) };
    if f.line_objects.len() == f.lines.len() {
        return Ok(f.line_objects);
    }
    let mut objects = Vec::new();
    for line in &f.lines {
        let o = new_object(emu, "CTLine")?;
        emu.ctframes.lines.insert(o, (line.clone(), 0));
        objects.push(o);
    }
    if let Some(f) = emu.ctframes.frames.get_mut(&frame) {
        f.line_objects = objects.clone();
    }
    Ok(objects)
}

/// `CTFrameGetLineOrigins(frame, range, CGPoint *origins)`
fn line_origins(emu: &mut Emu) -> Result<()> {
    let (frame, start, len, out) = (emu.arg(0), emu.arg(1) as usize, emu.arg(2) as usize, emu.arg(3));
    let Some(f) = emu.ctframes.frames.get(&frame).cloned() else { return Ok(()) };
    let end = if len == 0 { f.lines.len() } else { (start + len).min(f.lines.len()) };
    for (i, line) in f.lines[start.min(end)..end].iter().enumerate() {
        let p = out + i as u64 * 16;
        emu.write_u64(p, 0f64.to_bits())?;
        emu.write_u64(p + 8, (f.rect[3] - (line.y + line.ascent)).to_bits())?;
    }
    Ok(())
}

fn line_of(emu: &Emu, o: u64) -> Option<Line> {
    emu.ctframes.lines.get(&o).map(|l| l.0.clone()).or_else(|| emu.ctframes.runs.get(&o).cloned())
}

/// Each unit's glyph position from the line's origin.
fn positions(line: &Line) -> Vec<f64> {
    (0..line.len).map(|k| line.xs.get(k).copied().unwrap_or(0.0)).collect()
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CTFramesetterCreateWithAttributedString", create_framesetter),
    ("_CTFramesetterCreateFrame", create_frame),
    ("_CTFramesetterSuggestFrameSizeWithConstraints", suggest_size),
    ("_CTFrameGetLines", |e| {
        let objects = frame_lines(e, e.arg(0))?;
        let a = crate::foundation::new_array_of(e, &objects)?;
        ret(e, a)
    }),
    ("_CTFrameGetLineOrigins", line_origins),
    ("_CTFrameGetVisibleStringRange", |e| {
        let f = e.ctframes.frames.get(&e.arg(0)).cloned();
        let (s, l) = f.map_or((0, 0), |f| (f.lines.first().map_or(0, |l| l.start), f.lines.last().map_or(0, |l| l.start + l.len)));
        e.cpu.x[1] = (l - s) as u64;
        ret(e, s as u64)
    }),
    ("_CTLineGetStringRange", |e| {
        let (s, l) = line_of(e, e.arg(0)).map_or((0, 0), |l| (l.start, l.len));
        e.cpu.x[1] = l as u64;
        ret(e, s as u64)
    }),
    // CTLineGetTypographicBounds(line, ascent, descent, leading) → width.
    ("_CTLineGetTypographicBounds", |e| {
        let (o, asc, desc, leading) = (e.arg(0), e.arg(1), e.arg(2), e.arg(3));
        let Some(line) = line_of(e, o) else {
            e.cpu.q[0] = 0;
            return Ok(());
        };
        for (p, v) in [(asc, line.ascent), (desc, line.height - line.ascent), (leading, 0.0)] {
            if p != 0 {
                e.write_u64(p, v.to_bits())?;
            }
        }
        e.cpu.q[0] = line.width.to_bits() as u128;
        Ok(())
    }),
    ("_CTLineGetGlyphCount", |e| {
        let n = line_of(e, e.arg(0)).map_or(0, |l| l.len);
        ret(e, n as u64)
    }),
    // One run per line.
    ("_CTLineGetGlyphRuns", |e| {
        let o = e.arg(0);
        let Some((line, mut run)) = e.ctframes.lines.get(&o).cloned() else { return ret(e, 0) };
        if run == 0 {
            run = new_object(e, "CTRun")?;
            e.ctframes.runs.insert(run, line.clone());
            e.ctframes.lines.insert(o, (line, run));
        }
        let a = crate::foundation::new_array_of(e, &[run])?;
        ret(e, a)
    }),
    ("_CTRunGetGlyphCount", |e| {
        let n = e.ctframes.runs.get(&e.arg(0)).map_or(0, |l| l.len);
        ret(e, n as u64)
    }),
    ("_CTRunGetStringRange", |e| {
        let (s, l) = e.ctframes.runs.get(&e.arg(0)).map_or((0, 0), |l| (l.start, l.len));
        e.cpu.x[1] = l as u64;
        ret(e, s as u64)
    }),
    // CTRunGetPositions(run, range, CGPoint *buffer)
    ("_CTRunGetPositions", |e| {
        let (run, start, len, out) = (e.arg(0), e.arg(1) as usize, e.arg(2) as usize, e.arg(3));
        let Some(line) = e.ctframes.runs.get(&run).cloned() else { return Ok(()) };
        let xs = positions(&line);
        let end = if len == 0 { xs.len() } else { (start + len).min(xs.len()) };
        for (i, x) in xs[start.min(end)..end].iter().enumerate() {
            e.write_u64(out + i as u64 * 16, x.to_bits())?;
            e.write_u64(out + i as u64 * 16 + 8, 0f64.to_bits())?;
        }
        Ok(())
    }),
];
