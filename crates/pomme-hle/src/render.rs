//! Drawing: CGContext / CGPath recording, and a software compositor that
//! renders the window's view and layer tree to pixels with tiny-skia.
//!
//! Views that override `-drawRect:` in guest code draw into a recording
//! CGContext during a display pass. The recorded ops are in the view's own
//! coordinates (the CTM is applied as paths are built, as CoreGraphics does).
//! The compositor then walks the tree: background (rounded), drawn content,
//! the app's own sublayers, subviews (clipped if asked), border.
//!
//! Everything is integer-free floating-point math on the host with no system
//! libraries involved, so the same app state gives the same pixels anywhere.
//! Not yet: images, shadows, gradients, group opacity (alpha multiplies down
//! instead), emoji (Inter has none).

use crate::calayer::{self, Layer};
use crate::objc::{self, Imp};
use crate::uiview::{frame_of, view};
use crate::{Emu, HostFn, Result, Stop};
use std::collections::HashMap;
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform};

#[derive(Clone, Copy, Debug)]
pub enum El {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: char,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub weight: f64,
    pub color: [f64; 4],
}

/// Clip paths in force (in the context's base coordinates; even-odd flag), all intersected.
type Clip = Vec<(Vec<El>, bool)>;

#[derive(Clone, Debug)]
pub(crate) enum Op {
    Fill { path: Vec<El>, color: [f32; 4], even_odd: bool, clip: Clip },
    Stroke { path: Vec<El>, color: [f32; 4], width: f32, clip: Clip },
    Glyphs { glyphs: Vec<Glyph>, ctm: [f32; 6], clip: Clip },
}

#[derive(Clone, Debug)]
struct GState {
    ctm: [f32; 6],
    fill: [f32; 4],
    stroke: [f32; 4],
    line_width: f32,
    clip: Clip,
}

impl Default for GState {
    fn default() -> Self {
        GState { ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], fill: [0.0, 0.0, 0.0, 1.0], stroke: [0.0, 0.0, 0.0, 1.0], line_width: 1.0, clip: Vec::new() }
    }
}

#[derive(Default)]
struct Ctx {
    ops: Vec<Op>,
    gs: GState,
    saved: Vec<GState>,
    path: Vec<El>,
}

/// A bitmap image (UIImage / CGImage: pomme's UIImage objects are their own CGImages).
#[derive(Clone)]
pub struct Image {
    pub pix: std::rc::Rc<Pixmap>,
    /// In points.
    pub size: [f64; 2],
    pub scale: f64,
    /// UIEdgeInsets (top, left, bottom, right) for resizable images.
    pub caps: [f64; 4],
}

#[derive(Default)]
pub struct State {
    pub images: HashMap<u64, Image>,
    contexts: HashMap<u64, Ctx>,
    /// UIGraphicsPushContext stack; the top is the current context.
    stack: Vec<u64>,
    paths: HashMap<u64, Vec<El>>,
    /// What each view drew in its last `drawRect:`.
    content: HashMap<u64, Vec<Op>>,
}

fn apply(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// `m1` then `m2`.
fn concat(m1: &[f32; 6], m2: &[f32; 6]) -> [f32; 6] {
    let [a, b, c, d, tx, ty] = *m1;
    let [a2, b2, c2, d2, tx2, ty2] = *m2;
    [a * a2 + b * c2, a * b2 + b * d2, c * a2 + d * c2, c * b2 + d * d2, tx * a2 + ty * c2 + tx2, tx * b2 + ty * d2 + ty2]
}

fn transform_els(els: &[El], m: &[f32; 6]) -> Vec<El> {
    els.iter()
        .map(|e| match *e {
            El::Move(x, y) => {
                let (x, y) = apply(m, x, y);
                El::Move(x, y)
            }
            El::Line(x, y) => {
                let (x, y) = apply(m, x, y);
                El::Line(x, y)
            }
            El::Cubic(a, b, c, d, x, y) => {
                let (a, b) = apply(m, a, b);
                let (c, d) = apply(m, c, d);
                let (x, y) = apply(m, x, y);
                El::Cubic(a, b, c, d, x, y)
            }
            El::Close => El::Close,
        })
        .collect()
}

fn rect_els(x: f32, y: f32, w: f32, h: f32) -> Vec<El> {
    vec![El::Move(x, y), El::Line(x + w, y), El::Line(x + w, y + h), El::Line(x, y + h), El::Close]
}

/// Circular arc as cubic Béziers (≤ 90° each), starting with a line or move to its start.
fn arc_els(out: &mut Vec<El>, cx: f32, cy: f32, r: f32, start: f32, end: f32, clockwise: bool) {
    let tau = std::f32::consts::TAU;
    let mut sweep = end - start;
    // In UIKit's flipped coordinates "clockwise" means decreasing angle in CG's terms.
    if clockwise {
        while sweep > 0.0 {
            sweep -= tau;
        }
    } else {
        while sweep < 0.0 {
            sweep += tau;
        }
    }
    let (sx, sy) = (cx + r * start.cos(), cy + r * start.sin());
    if out.is_empty() || matches!(out.last(), Some(El::Close)) {
        out.push(El::Move(sx, sy));
    } else {
        out.push(El::Line(sx, sy));
    }
    let n = (sweep.abs() / (tau / 4.0)).ceil().max(1.0) as usize;
    let step = sweep / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = start;
    for _ in 0..n {
        let b = a + step;
        let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
        out.push(El::Cubic(cx + r * (ca - k * sa), cy + r * (sa + k * ca), cx + r * (cb + k * sb), cy + r * (sb - k * cb), cx + r * cb, cy + r * sb));
        a = b;
    }
}

fn rounded_rect(w: f32, h: f32, r: f32) -> Vec<El> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r == 0.0 {
        return rect_els(0.0, 0.0, w, h);
    }
    use std::f32::consts::{FRAC_PI_2, PI};
    let mut out = Vec::new();
    arc_els(&mut out, w - r, r, r, -FRAC_PI_2, 0.0, false);
    arc_els(&mut out, w - r, h - r, r, 0.0, FRAC_PI_2, false);
    arc_els(&mut out, r, h - r, r, FRAC_PI_2, PI, false);
    arc_els(&mut out, r, r, r, PI, PI + FRAC_PI_2, false);
    out.push(El::Close);
    out
}

fn build(els: &[El]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for e in els {
        match *e {
            El::Move(x, y) => pb.move_to(x, y),
            El::Line(x, y) => pb.line_to(x, y),
            El::Cubic(a, b, c, d, x, y) => pb.cubic_to(a, b, c, d, x, y),
            El::Close => pb.close(),
        }
    }
    pb.finish()
}

// ---- CGContext ----

fn d(emu: &Emu, i: usize) -> f32 {
    f64::from_bits(emu.cpu.q[i] as u64) as f32
}

fn rgba(emu: &Emu, c: u64) -> [f32; 4] {
    emu.ui.views.colors.get(&c).map_or([0.0; 4], |c| c.map(|v| v as f32))
}

fn ctx_mut(emu: &mut Emu, c: u64) -> Option<&mut Ctx> {
    emu.render.contexts.get_mut(&c)
}

/// Start a recording context and make it current.
pub(crate) fn begin_context(emu: &mut Emu) -> Result<u64> {
    let c = emu.alloc(16)?;
    emu.render.contexts.insert(c, Ctx::default());
    emu.render.stack.push(c);
    Ok(c)
}

pub(crate) fn end_context(emu: &mut Emu, c: u64) -> Vec<Op> {
    emu.render.stack.retain(|&x| x != c);
    emu.render.contexts.remove(&c).map(|x| x.ops).unwrap_or_default()
}

/// Draw glyphs into the current context (from NSLayoutManager).
pub fn draw_glyphs(emu: &mut Emu, glyphs: Vec<Glyph>) {
    let Some(&c) = emu.render.stack.last() else { return };
    if let Some(ctx) = ctx_mut(emu, c) {
        let (ctm, clip) = (ctx.gs.ctm, ctx.gs.clip.clone());
        ctx.ops.push(Op::Glyphs { glyphs, ctm, clip });
    }
}

fn with_ctx(emu: &mut Emu, f: impl FnOnce(&mut Ctx)) -> Result<()> {
    let c = emu.arg(0);
    if let Some(ctx) = ctx_mut(emu, c) {
        f(ctx);
    }
    Ok(())
}

fn add_path_els(ctx: &mut Ctx, els: &[El]) {
    let t = transform_els(els, &ctx.gs.ctm);
    ctx.path.extend(t);
}

fn fill(emu: &mut Emu, even_odd: bool) -> Result<()> {
    with_ctx(emu, |ctx| {
        let path = std::mem::take(&mut ctx.path);
        ctx.ops.push(Op::Fill { path, color: ctx.gs.fill, even_odd, clip: ctx.gs.clip.clone() });
    })
}

fn current_context(emu: &mut Emu) -> Result<()> {
    let c = emu.render.stack.last().copied().unwrap_or(0);
    emu.ret(c);
    Ok(())
}

fn clip_to_rect(emu: &mut Emu) -> Result<()> {
    let r = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    with_ctx(emu, |ctx| {
        let els = transform_els(&rect_els(r[0], r[1], r[2], r[3]), &ctx.gs.ctm);
        ctx.gs.clip.push((els, false));
    })
}

/// `CGContextClip` / `CGContextEOClip`: the current path becomes part of the clip.
fn clip_to_path(emu: &mut Emu, even_odd: bool) -> Result<()> {
    with_ctx(emu, |ctx| {
        let path = std::mem::take(&mut ctx.path);
        ctx.gs.clip.push((path, even_odd));
    })
}

fn read_affine(emu: &Emu, p: u64) -> Result<[f32; 6]> {
    let mut t = [0.0; 6];
    for (i, v) in t.iter_mut().enumerate() {
        *v = f64::from_bits(emu.read_u64(p + i as u64 * 8)?) as f32;
    }
    Ok(t)
}

fn concat_ctm(emu: &mut Emu, m: [f32; 6]) -> Result<()> {
    with_ctx(emu, |ctx| ctx.gs.ctm = concat(&m, &ctx.gs.ctm))
}

// ---- CGPath (a list of elements per path object) ----

fn path_els(emu: &Emu, p: u64) -> Vec<El> {
    emu.render.paths.get(&p).cloned().unwrap_or_default()
}

fn new_path(emu: &mut Emu, els: Vec<El>) -> Result<u64> {
    let p = emu.alloc(16)?;
    emu.render.paths.insert(p, els);
    Ok(p)
}

fn optional_affine(emu: &Emu, p: u64) -> Result<Option<[f32; 6]>> {
    if p == 0 { Ok(None) } else { read_affine(emu, p).map(Some) }
}

fn path_add(emu: &mut Emu, p: u64, els: Vec<El>, m: Option<[f32; 6]>) {
    let els = match m {
        Some(m) => transform_els(&els, &m),
        None => els,
    };
    emu.render.paths.entry(p).or_default().extend(els);
}

// ---- display pass and compositing ----

/// Run the guest's `-drawRect:` overrides, recording what each view draws.
fn display(emu: &mut Emu, v: u64) -> Result<()> {
    let vw = view(emu, v);
    if !vw.hidden {
        let isa = emu.read_u64(v)?;
        let sel = objc::intern(emu, "drawRect:")?;
        if let Some(Imp::Guest(_)) = objc::lookup(emu, isa, sel)? {
            let c = begin_context(emu)?;
            let b = vw.bounds;
            for (i, x) in b.iter().enumerate() {
                emu.cpu.q[i] = x.to_bits() as u128;
            }
            let r = objc::send(emu, v, "drawRect:", &[]);
            let ops = end_context(emu, c);
            r?;
            emu.render.content.insert(v, ops);
        }
    }
    for s in vw.subviews {
        display(emu, s)?;
    }
    Ok(())
}

fn ts(m: &[f32; 6]) -> Transform {
    Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5])
}

fn paint(color: [f32; 4], alpha: f32) -> Option<Paint<'static>> {
    let a = (color[3] * alpha).clamp(0.0, 1.0);
    if a <= 0.0 {
        return None;
    }
    let mut p = Paint::default();
    p.set_color(tiny_skia::Color::from_rgba(color[0].clamp(0.0, 1.0), color[1].clamp(0.0, 1.0), color[2].clamp(0.0, 1.0), a)?);
    p.anti_alias = true;
    Some(p)
}

struct Painter<'a> {
    pix: Pixmap,
    font: Option<&'a [u8]>,
    /// For characters the text font lacks (emoji): color glyphs (COLR).
    emoji: Option<&'a [u8]>,
}

impl Painter<'_> {
    fn fill(&mut self, els: &[El], color: [f32; 4], alpha: f32, even_odd: bool, t: &[f32; 6], mask: Option<&Mask>) {
        if let (Some(path), Some(p)) = (build(els), paint(color, alpha)) {
            let rule = if even_odd { FillRule::EvenOdd } else { FillRule::Winding };
            self.pix.fill_path(&path, &p, rule, ts(t), mask);
        }
    }

    fn stroke(&mut self, els: &[El], color: [f32; 4], alpha: f32, width: f32, t: &[f32; 6], mask: Option<&Mask>) {
        if let (Some(path), Some(p)) = (build(els), paint(color, alpha)) {
            let s = Stroke { width, ..Default::default() };
            self.pix.stroke_path(&path, &p, &s, ts(t), mask);
        }
    }

    fn glyphs(&mut self, glyphs: &[Glyph], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let Some(data) = self.font else { return };
        let Ok(mut face) = ttf_parser::Face::parse(data, 0) else { return };
        let upem = face.units_per_em() as f32;
        let mut current: Option<(u16, u16)> = None;
        for g in glyphs {
            let wght = crate::textkit::ot_weight(g.weight).round() as u16;
            let opsz = g.size.clamp(14.0, 32.0).round() as u16;
            if current != Some((wght, opsz)) {
                face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), wght as f32);
                face.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), opsz as f32);
                current = Some((wght, opsz));
            }
            let Some(gid) = face.glyph_index(g.ch) else {
                self.emoji_glyph(g, alpha, t, mask);
                continue;
            };
            let mut b = Outline(PathBuilder::new());
            if face.outline_glyph(gid, &mut b).is_none() {
                continue;
            }
            let Some(path) = b.0.finish() else { continue };
            let s = g.size as f32 / upem;
            let place = [s, 0.0, 0.0, -s, g.x as f32, g.y as f32];
            let color = g.color.map(|v| v as f32);
            if let Some(p) = paint(color, alpha) {
                self.pix.fill_path(&path, &p, FillRule::Winding, ts(&concat(&place, t)), mask);
            }
        }
    }

    /// A color glyph from the emoji font, painted layer by layer.
    fn emoji_glyph(&mut self, g: &Glyph, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let Some(data) = self.emoji else { return };
        let Ok(face) = ttf_parser::Face::parse(data, 0) else { return };
        let Some(gid) = face.glyph_index(g.ch) else { return };
        let s = g.size as f32 / face.units_per_em() as f32;
        let place = [s, 0.0, 0.0, -s, g.x as f32, g.y as f32];
        let base = ts(&concat(&place, t));
        let mut painter = ColrPainter { face: &face, pix: &mut self.pix, base, transforms: Vec::new(), clips: Vec::new(), outline: None, alpha, mask };
        let fg = ttf_parser::RgbaColor::new(0, 0, 0, 255);
        if face.is_color_glyph(gid) {
            face.paint_color_glyph(gid, 0, fg, &mut painter);
        } else if let Some(path) = painter.glyph_path(gid) {
            // A plain outline in the emoji font: draw it in the text color.
            if let Some(p) = paint(g.color.map(|v| v as f32), alpha) {
                self.pix.fill_path(&path, &p, FillRule::Winding, base, mask);
            }
        }
    }

    fn ops(&mut self, ops: &[Op], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        for op in ops {
            let clip = match op {
                Op::Fill { clip, .. } | Op::Stroke { clip, .. } | Op::Glyphs { clip, .. } => clip,
            };
            let mut clipped: Option<Mask> = None;
            for (els, even_odd) in clip {
                clipped = self.intersect_rule(clipped.as_ref().or(mask), els, *even_odd, t);
            }
            let m = if clip.is_empty() { mask } else { clipped.as_ref() };
            match op {
                Op::Fill { path, color, even_odd, .. } => self.fill(path, *color, alpha, *even_odd, t, m),
                Op::Stroke { path, color, width, .. } => self.stroke(path, *color, alpha, *width, t, m),
                Op::Glyphs { glyphs, ctm, .. } => self.glyphs(glyphs, alpha, &concat(ctm, t), m),
            }
        }
    }

    /// `mask` ∩ the path's area.
    fn intersect(&self, mask: Option<&Mask>, els: &[El], t: &[f32; 6]) -> Option<Mask> {
        self.intersect_rule(mask, els, false, t)
    }

    fn intersect_rule(&self, mask: Option<&Mask>, els: &[El], even_odd: bool, t: &[f32; 6]) -> Option<Mask> {
        let path = build(els)?;
        let mut m = match mask {
            Some(m) => m.clone(),
            None => {
                let mut m = Mask::new(self.pix.width(), self.pix.height())?;
                m.fill_path(&rect_path(self.pix.width() as f32, self.pix.height() as f32)?, FillRule::Winding, false, Transform::identity());
                m
            }
        };
        m.intersect_path(&path, if even_odd { FillRule::EvenOdd } else { FillRule::Winding }, true, ts(t));
        Some(m)
    }
}

fn rect_path(w: f32, h: f32) -> Option<tiny_skia::Path> {
    build(&rect_els(0.0, 0.0, w, h))
}

/// Paints COLR (v0 and v1) color glyphs: layers of outlines filled with solid
/// colors or gradients, under clips and transforms. Layer compositing modes
/// are treated as plain source-over.
struct ColrPainter<'f, 'p, 'm> {
    face: &'f ttf_parser::Face<'f>,
    pix: &'p mut Pixmap,
    /// Glyph space (font units, y up) to device.
    base: Transform,
    transforms: Vec<Transform>,
    /// Clip paths in device space.
    clips: Vec<tiny_skia::Path>,
    /// The last outlined glyph (font units).
    outline: Option<tiny_skia::Path>,
    alpha: f32,
    mask: Option<&'m Mask>,
}

impl ColrPainter<'_, '_, '_> {
    fn current(&self) -> Transform {
        self.transforms.last().copied().unwrap_or(self.base)
    }

    fn glyph_path(&self, gid: ttf_parser::GlyphId) -> Option<tiny_skia::Path> {
        let mut b = Outline(PathBuilder::new());
        self.face.outline_glyph(gid, &mut b)?;
        b.0.finish()
    }

    fn color(&self, c: ttf_parser::RgbaColor) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(c.red, c.green, c.blue, (c.alpha as f32 * self.alpha.clamp(0.0, 1.0)) as u8)
    }

    fn stops(&self, it: impl Iterator<Item = ttf_parser::colr::ColorStop>) -> Vec<tiny_skia::GradientStop> {
        let mut v: Vec<_> = it.collect();
        v.sort_by(|a, b| a.stop_offset.total_cmp(&b.stop_offset));
        v.iter().map(|s| tiny_skia::GradientStop::new(s.stop_offset, self.color(s.color))).collect()
    }

    /// The outer mask ∩ every clip, or None when nothing clips.
    fn clip_mask(&self) -> Option<Mask> {
        if self.clips.is_empty() {
            return self.mask.cloned();
        }
        let (w, h) = (self.pix.width(), self.pix.height());
        let mut m = match self.mask {
            Some(m) => m.clone(),
            None => {
                let mut m = Mask::new(w, h)?;
                m.fill_path(&rect_path(w as f32, h as f32)?, FillRule::Winding, false, Transform::identity());
                m
            }
        };
        for c in &self.clips {
            m.intersect_path(c, FillRule::Winding, true, Transform::identity());
        }
        Some(m)
    }
}

impl<'a> ttf_parser::colr::Painter<'a> for ColrPainter<'_, '_, '_> {
    fn outline_glyph(&mut self, glyph_id: ttf_parser::GlyphId) {
        self.outline = self.glyph_path(glyph_id);
    }

    fn paint(&mut self, p: ttf_parser::colr::Paint<'a>) {
        use ttf_parser::colr::Paint as P;
        let t = self.current();
        let shader = match p {
            P::Solid(c) => tiny_skia::Shader::SolidColor(self.color(c)),
            P::LinearGradient(g) => {
                let stops = self.stops(g.stops(0, &[]));
                tiny_skia::LinearGradient::new((g.x0, g.y0).into(), (g.x1, g.y1).into(), stops, tiny_skia::SpreadMode::Pad, t)
                    .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::TRANSPARENT))
            }
            P::RadialGradient(g) => {
                let stops = self.stops(g.stops(0, &[]));
                tiny_skia::RadialGradient::new((g.x0, g.y0).into(), g.r0.max(0.0), (g.x1, g.y1).into(), g.r1.max(0.0), stops, tiny_skia::SpreadMode::Pad, t)
                    .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::TRANSPARENT))
            }
            // Sweep gradients aren't in tiny-skia: their first color stands in.
            P::SweepGradient(g) => {
                let c = g.stops(0, &[]).min_by(|a, b| a.stop_offset.total_cmp(&b.stop_offset)).map_or(tiny_skia::Color::TRANSPARENT, |s| self.color(s.color));
                tiny_skia::Shader::SolidColor(c)
            }
        };
        let paint = Paint { shader, anti_alias: true, ..Default::default() };
        let mask = self.clip_mask();
        // Fill the clip region: the whole canvas, under the clips' mask.
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, self.pix.width() as f32, self.pix.height() as f32) {
            self.pix.fill_rect(r, &paint, Transform::identity(), mask.as_ref());
        }
    }

    fn push_clip(&mut self) {
        let t = self.current();
        if let Some(p) = self.outline.clone().and_then(|p| p.transform(t)) {
            self.clips.push(p);
        }
    }

    fn push_clip_box(&mut self, b: ttf_parser::colr::ClipBox) {
        let t = self.current();
        if let Some(p) = tiny_skia::Rect::from_ltrb(b.x_min, b.y_min, b.x_max, b.y_max).map(PathBuilder::from_rect).and_then(|p| p.transform(t)) {
            self.clips.push(p);
        }
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn push_layer(&mut self, _mode: ttf_parser::colr::CompositeMode) {}
    fn pop_layer(&mut self) {}

    fn push_transform(&mut self, t: ttf_parser::Transform) {
        let next = self.current().pre_concat(Transform::from_row(t.a, t.b, t.c, t.d, t.e, t.f));
        self.transforms.push(next);
    }

    fn pop_transform(&mut self) {
        self.transforms.pop();
    }
}

struct Outline(PathBuilder);

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// A layer's own transform about its anchor point, in its superlayer's coordinates.
fn placement(frame: [f64; 4], ly: &Layer) -> [f32; 6] {
    let (w, h) = (frame[2] as f32, frame[3] as f32);
    let (ax, ay) = (ly.anchor[0] as f32 * w, ly.anchor[1] as f32 * h);
    let a = calayer::affine_of(&ly.transform).map(|v| v as f32);
    // translate(-anchor) · transform · translate(frame origin + anchor)
    let to_anchor = [1.0, 0.0, 0.0, 1.0, -ax, -ay];
    let back = [1.0, 0.0, 0.0, 1.0, frame[0] as f32 + ax, frame[1] as f32 + ay];
    concat(&concat(&to_anchor, &a), &back)
}

/// A layer's background, in its corner shape.
#[allow(clippy::too_many_arguments)]
fn draw_layer_box(p: &mut Painter, ly: &Layer, bg: u64, colors: &HashMap<u64, [f64; 4]>, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let shape = rounded_rect(w, h, ly.corner_radius as f32);
    if let Some(c) = colors.get(&bg) {
        p.fill(&shape, c.map(|v| v as f32), alpha, false, t, mask);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_border(p: &mut Painter, ly: &Layer, colors: &HashMap<u64, [f64; 4]>, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    if ly.border_width <= 0.0 {
        return;
    }
    let bw = ly.border_width as f32;
    let c = colors.get(&ly.border_color).map_or([0.0, 0.0, 0.0, 1.0], |c| c.map(|v| v as f32));
    let inset = transform_els(&rounded_rect(w - bw, h - bw, (ly.corner_radius as f32 - bw / 2.0).max(0.0)), &[1.0, 0.0, 0.0, 1.0, bw / 2.0, bw / 2.0]);
    p.stroke(&inset, c, alpha, bw, t, mask);
}

fn draw_sublayers(emu: &mut Emu, p: &mut Painter, layer: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    for s in calayer::layer(emu, layer).sublayers {
        if calayer::backing_view(emu, s).is_some() {
            continue;
        }
        let ly = calayer::layer(emu, s);
        if ly.hidden || ly.opacity <= 0.0 {
            continue;
        }
        let frame = calayer::layer_frame(emu, s);
        let lt = concat(&placement(frame, &ly), t);
        let a = alpha * ly.opacity;
        let (w, h) = (frame[2] as f32, frame[3] as f32);
        draw_layer_box(p, &ly, ly.background, &emu.ui.views.colors, w, h, a, &lt, mask);
        if let Some(img) = emu.render.images.get(&ly.contents).cloned() {
            draw_contents(p, &img, ly.contents_center, w, h, a, &lt, mask);
        }
        let inner = [1.0, 0.0, 0.0, 1.0, -ly.bounds[0] as f32, -ly.bounds[1] as f32];
        draw_sublayers(emu, p, s, a, &concat(&inner, &lt), mask);
        draw_border(p, &ly, &emu.ui.views.colors, w, h, a, &lt, mask);
    }
}

fn draw_view(emu: &mut Emu, p: &mut Painter, v: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let vw = view(emu, v);
    let a = alpha * vw.alpha as f32;
    if vw.hidden || a <= 0.0 {
        return;
    }
    let frame = frame_of(emu, v);
    let ly = if vw.layer != 0 { calayer::layer(emu, vw.layer) } else { Layer::default() };
    let vt = concat(&placement(frame, &ly), t);
    let (w, h) = (frame[2] as f32, frame[3] as f32);
    draw_layer_box(p, &ly, vw.background, &emu.ui.views.colors, w, h, a, &vt, mask);
    if let Some(img) = emu.render.images.get(&ly.contents).cloned() {
        draw_contents(p, &img, ly.contents_center, w, h, a, &vt, mask);
    }
    let inner = concat(&[1.0, 0.0, 0.0, 1.0, -vw.bounds[0] as f32, -vw.bounds[1] as f32], &vt);
    let clipped = if vw.clips { p.intersect(mask, &rounded_rect(w, h, ly.corner_radius as f32), &vt) } else { None };
    let m = if vw.clips { clipped.as_ref() } else { mask };
    if let Some(ops) = emu.render.content.get(&v).cloned() {
        p.ops(&ops, a, &inner, m);
    }
    if vw.layer != 0 {
        draw_sublayers(emu, p, vw.layer, a, &inner, m);
    }
    if let Ok(Some((title, back))) = crate::navigation::bar_titles(emu, v) {
        draw_bar(emu, p, &title, back.is_some(), w, h, a, &inner, m);
    }
    for s in vw.subviews {
        draw_view(emu, p, s, a, &inner, m);
    }
    draw_border(p, &ly, &emu.ui.views.colors, w, h, a, &vt, mask);
}

/// A navigation bar, iOS 26 style: no background, a centered semibold title,
/// and a round back button with a chevron.
#[allow(clippy::too_many_arguments)]
fn draw_bar(emu: &mut Emu, p: &mut Painter, title: &str, back: bool, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    const SIZE: f64 = 17.0;
    let (glyphs, width) = crate::textkit::line_glyphs(emu, title, SIZE, 0.3, [0.0, 0.0, 0.0, 1.0], 0.0, 0.0);
    let (x, baseline) = ((w as f64 - width) / 2.0, h as f64 / 2.0 + SIZE * 0.35);
    let placed: Vec<Glyph> = glyphs.into_iter().map(|g| Glyph { x: g.x + x, y: baseline, ..g }).collect();
    p.glyphs(&placed, alpha, t, mask);
    if back {
        // A 44-pt glass circle 16 pt in, with a chevron.
        let (d, cx, cy) = (44.0f32, 16.0 + 22.0, h / 2.0);
        let mut circle = Vec::new();
        arc_els(&mut circle, cx, cy, d / 2.0, 0.0, std::f32::consts::TAU, false);
        circle.push(El::Close);
        p.fill(&circle, [0.96, 0.96, 0.97, 1.0], alpha, false, t, mask);
        p.stroke(&circle, [0.0, 0.0, 0.0, 0.06], alpha, 0.5, t, mask);
        let chevron = [El::Move(cx + 3.5, cy - 8.0), El::Line(cx - 4.5, cy), El::Line(cx + 3.5, cy + 8.0)];
        let s = Stroke { width: 2.6, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
        if let (Some(path), Some(paint)) = (build(&chevron), paint([0.0, 0.0, 0.0, 1.0], alpha)) {
            p.pix.stroke_path(&path, &paint, &s, ts(t), mask);
        }
    }
}

/// Draw recorded ops into a new transparent bitmap of `size` points at `scale`.
pub(crate) fn rasterize(emu: &mut Emu, ops: &[Op], size: [f64; 2], scale: f64) -> Result<Pixmap> {
    let (w, h) = (((size[0] * scale).ceil() as u32).max(1), ((size[1] * scale).ceil() as u32).max(1));
    let pix = Pixmap::new(w, h).ok_or_else(|| Stop::Unsupported(format!("can't make a {w}×{h} bitmap")))?;
    let (font, emoji) = (crate::textkit::font_bytes(emu), crate::textkit::emoji_bytes(emu));
    let mut p = Painter { pix, font: font.as_deref().map(|v| v.as_slice()), emoji: emoji.as_deref().map(|v| v.as_slice()) };
    let s = scale as f32;
    p.ops(ops, 1.0, &[s, 0.0, 0.0, s, 0.0, 0.0], None);
    Ok(p.pix)
}

#[allow(clippy::too_many_arguments)]
/// Draw a layer's `contents` image into its bounds, nine-slicing by `contentsCenter`.
fn draw_contents(p: &mut Painter, img: &Image, center: [f64; 4], w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let (pw, ph) = (img.pix.width() as f32, img.pix.height() as f32);
    let unit = center == [0.0, 0.0, 1.0, 1.0];
    // Source cuts in pixels, destination cuts in points.
    let (sx, sy) = if unit { ([0.0, 0.0, pw, pw], [0.0, 0.0, ph, ph]) } else {
        let (x0, x1) = (center[0] as f32 * pw, (center[0] + center[2]) as f32 * pw);
        let (y0, y1) = (center[1] as f32 * ph, (center[1] + center[3]) as f32 * ph);
        ([0.0, x0, x1, pw], [0.0, y0, y1, ph])
    };
    let sc = img.scale as f32;
    let (dx, dy) = if unit { ([0.0, 0.0, w, w], [0.0, 0.0, h, h]) } else {
        ([0.0, sx[1] / sc, w - (pw - sx[2]) / sc, w], [0.0, sy[1] / sc, h - (ph - sy[2]) / sc, h])
    };
    let mut paint = tiny_skia::PixmapPaint { opacity: alpha.clamp(0.0, 1.0), ..Default::default() };
    paint.quality = tiny_skia::FilterQuality::Bilinear;
    for i in 0..3 {
        for j in 0..3 {
            let (sw, sh) = (sx[i + 1] - sx[i], sy[j + 1] - sy[j]);
            let (dw, dh) = (dx[i + 1] - dx[i], dy[j + 1] - dy[j]);
            if sw < 0.5 || sh < 0.5 || dw <= 0.0 || dh <= 0.0 {
                continue;
            }
            let rect = tiny_skia::IntRect::from_xywh(sx[i] as i32, sy[j] as i32, sw.round() as u32, sh.round() as u32);
            let Some(part) = rect.and_then(|r| img.pix.clone_rect(r)) else { continue };
            let place = [dw / part.width() as f32, 0.0, 0.0, dh / part.height() as f32, dx[i], dy[j]];
            p.pix.draw_pixmap(0, 0, part.as_ref(), &paint, ts(&concat(&place, t)), mask);
        }
    }
}

/// Render the visible windows at `scale` pixels per point.
pub fn render(emu: &mut Emu, scale: f32) -> Result<Pixmap> {
    for w in emu.ui.windows.clone() {
        display(emu, w)?;
    }
    let (sw, sh) = ((crate::uikit::SCREEN_W as f32 * scale) as u32, (crate::uikit::SCREEN_H as f32 * scale) as u32);
    let mut pix = Pixmap::new(sw, sh).ok_or_else(|| Stop::Unsupported("can't make a pixmap".into()))?;
    pix.fill(tiny_skia::Color::BLACK);
    let (font, emoji) = (crate::textkit::font_bytes(emu), crate::textkit::emoji_bytes(emu));
    let mut p = Painter { pix, font: font.as_deref().map(|v| v.as_slice()), emoji: emoji.as_deref().map(|v| v.as_slice()) };
    let t = [scale, 0.0, 0.0, scale, 0.0, 0.0];
    for w in emu.ui.windows.clone() {
        draw_view(emu, &mut p, w, 1.0, &t, None);
    }
    Ok(p.pix)
}

/// PNG bytes of [`render`].
pub fn screenshot_png(emu: &mut Emu, scale: f32) -> Result<Vec<u8>> {
    let pix = render(emu, scale)?;
    pix.encode_png().map_err(|e| Stop::Unsupported(format!("PNG encoding: {e}")))
}

// ---- host functions ----

fn nothing(_: &mut Emu) -> Result<()> {
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_UIGraphicsGetCurrentContext", current_context),
    ("_UIGraphicsPushContext", |e| {
        let c = e.arg(0);
        e.render.stack.push(c);
        Ok(())
    }),
    ("_UIGraphicsPopContext", |e| {
        e.render.stack.pop();
        Ok(())
    }),
    ("_CGContextSaveGState", |e| {
        with_ctx(e, |c| {
            let g = c.gs.clone();
            c.saved.push(g);
        })
    }),
    ("_CGContextRestoreGState", |e| {
        with_ctx(e, |c| {
            if let Some(g) = c.saved.pop() {
                c.gs = g;
            }
        })
    }),
    ("_CGContextRelease", nothing),
    ("_CGContextRetain", |e| {
        let c = e.arg(0);
        e.ret(c);
        Ok(())
    }),
    ("_CGContextSetShouldAntialias", nothing),
    ("_CGContextSetAllowsAntialiasing", nothing),
    ("_CGContextSetShouldSmoothFonts", nothing),
    ("_CGContextSetLineCap", nothing),
    ("_CGContextSetLineJoin", nothing),
    ("_CGContextSetLineDash", nothing),
    ("_CGContextSetLineWidth", |e| {
        let w = d(e, 0);
        with_ctx(e, |c| c.gs.line_width = w)
    }),
    ("_CGContextSetFillColorWithColor", |e| {
        let col = rgba(e, e.arg(1));
        with_ctx(e, |c| c.gs.fill = col)
    }),
    ("_CGContextSetStrokeColorWithColor", |e| {
        let col = rgba(e, e.arg(1));
        with_ctx(e, |c| c.gs.stroke = col)
    }),
    ("_CGContextSetRGBFillColor", |e| {
        let col = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        with_ctx(e, |c| c.gs.fill = col)
    }),
    ("_CGContextSetRGBStrokeColor", |e| {
        let col = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        with_ctx(e, |c| c.gs.stroke = col)
    }),
    ("_CGContextConcatCTM", |e| {
        let m = read_affine(e, e.arg(1))?;
        concat_ctm(e, m)
    }),
    ("_CGContextTranslateCTM", |e| {
        let (x, y) = (d(e, 0), d(e, 1));
        concat_ctm(e, [1.0, 0.0, 0.0, 1.0, x, y])
    }),
    ("_CGContextScaleCTM", |e| {
        let (x, y) = (d(e, 0), d(e, 1));
        concat_ctm(e, [x, 0.0, 0.0, y, 0.0, 0.0])
    }),
    ("_CGContextClipToRect", clip_to_rect),
    ("_CGContextClip", |e| clip_to_path(e, false)),
    ("_CGContextEOClip", |e| clip_to_path(e, true)),
    ("_CGContextBeginPath", |e| with_ctx(e, |c| c.path.clear())),
    ("_CGContextClosePath", |e| with_ctx(e, |c| c.path.push(El::Close))),
    ("_CGContextMoveToPoint", |e| {
        let (x, y) = (d(e, 0), d(e, 1));
        with_ctx(e, |c| add_path_els(c, &[El::Move(x, y)]))
    }),
    ("_CGContextAddLineToPoint", |e| {
        let (x, y) = (d(e, 0), d(e, 1));
        with_ctx(e, |c| add_path_els(c, &[El::Line(x, y)]))
    }),
    ("_CGContextAddCurveToPoint", |e| {
        let v: Vec<f32> = (0..6).map(|i| d(e, i)).collect();
        with_ctx(e, |c| add_path_els(c, &[El::Cubic(v[0], v[1], v[2], v[3], v[4], v[5])]))
    }),
    ("_CGContextAddLines", |e| {
        let (pts, n) = (e.arg(1), e.arg(2));
        let mut els = Vec::new();
        for i in 0..n {
            let x = f64::from_bits(e.read_u64(pts + i * 16)?) as f32;
            let y = f64::from_bits(e.read_u64(pts + i * 16 + 8)?) as f32;
            els.push(if i == 0 { El::Move(x, y) } else { El::Line(x, y) });
        }
        with_ctx(e, |c| add_path_els(c, &els))
    }),
    ("_CGContextAddRect", |e| {
        let r = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        with_ctx(e, |c| add_path_els(c, &rect_els(r[0], r[1], r[2], r[3])))
    }),
    ("_CGContextAddPath", |e| {
        let els = path_els(e, e.arg(1));
        with_ctx(e, |c| add_path_els(c, &els))
    }),
    ("_CGContextFillPath", |e| fill(e, false)),
    ("_CGContextEOFillPath", |e| fill(e, true)),
    ("_CGContextStrokePath", |e| {
        with_ctx(e, |ctx| {
            let path = std::mem::take(&mut ctx.path);
            let width = ctx.gs.line_width * ctx.gs.ctm[0].abs().max(ctx.gs.ctm[3].abs());
            ctx.ops.push(Op::Stroke { path, color: ctx.gs.stroke, width, clip: ctx.gs.clip.clone() });
        })
    }),
    ("_CGContextFillRect", |e| {
        let r = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        with_ctx(e, |c| {
            add_path_els(c, &rect_els(r[0], r[1], r[2], r[3]));
            let path = std::mem::take(&mut c.path);
            c.ops.push(Op::Fill { path, color: c.gs.fill, even_odd: false, clip: c.gs.clip.clone() });
        })
    }),
    ("_CGPathCreateMutable", |e| {
        let p = new_path(e, Vec::new())?;
        e.ret(p);
        Ok(())
    }),
    ("_CGPathCreateWithRect", |e| {
        let r = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        let m = optional_affine(e, e.arg(0))?;
        let els = rect_els(r[0], r[1], r[2], r[3]);
        let p = new_path(e, m.map_or(els.clone(), |m| transform_els(&els, &m)))?;
        e.ret(p);
        Ok(())
    }),
    ("_CGPathAddRect", |e| {
        let (p, m) = (e.arg(0), optional_affine(e, e.arg(1))?);
        let r = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        path_add(e, p, rect_els(r[0], r[1], r[2], r[3]), m);
        Ok(())
    }),
    ("_CGPathAddPath", |e| {
        let (p, m, other) = (e.arg(0), optional_affine(e, e.arg(1))?, e.arg(2));
        let els = path_els(e, other);
        path_add(e, p, els, m);
        Ok(())
    }),
    ("_CGPathAddArc", |e| {
        let (p, m, cw) = (e.arg(0), optional_affine(e, e.arg(1))?, e.arg(2) & 1 != 0);
        let (x, y, r, a0, a1) = (d(e, 0), d(e, 1), d(e, 2), d(e, 3), d(e, 4));
        let mut els = path_els(e, p);
        let start = els.len();
        arc_els(&mut els, x, y, r, a0, a1, cw);
        let new: Vec<El> = els.split_off(start);
        path_add(e, p, new, m);
        Ok(())
    }),
    ("_CGPathCloseSubpath", |e| {
        let p = e.arg(0);
        path_add(e, p, vec![El::Close], None);
        Ok(())
    }),
    ("_CGPathRelease", nothing),
    ("_CGPathRetain", |e| {
        let p = e.arg(0);
        e.ret(p);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_order() {
        // Scale by 2 then move by 10.
        let m = concat(&[2.0, 0.0, 0.0, 2.0, 0.0, 0.0], &[1.0, 0.0, 0.0, 1.0, 10.0, 0.0]);
        assert_eq!(apply(&m, 1.0, 1.0), (12.0, 2.0));
    }

    #[test]
    fn arcs_end_where_they_should() {
        let mut els = Vec::new();
        arc_els(&mut els, 0.0, 0.0, 10.0, 0.0, std::f32::consts::PI, false);
        let Some(El::Cubic(.., x, y)) = els.last().copied() else { panic!() };
        assert!((x + 10.0).abs() < 1e-4 && y.abs() < 1e-4);
    }
}
