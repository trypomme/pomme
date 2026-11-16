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
use crate::uiview::view;
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
    /// The glyph in its face, as shaped (None: `ch`'s glyph, looked up).
    pub gid: Option<u16>,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub weight: f64,
    /// The face it's in (coretext's; 0: the system font).
    pub face: usize,
    pub color: [f64; 4],
    /// A synthetic oblique (italic where the face has none): how far the
    /// glyph leans right per unit of height; 0 upright.
    pub skew: f32,
    /// A shadow under it (NSShadow).
    pub shadow: Option<TextShadow>,
}

/// A text shadow (NSShadow): moved by `offset` points (y down), blurred by
/// `blur` (Core Graphics' blur, a Gaussian of half that deviation), in `color`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadow {
    pub offset: [f64; 2],
    pub blur: f64,
    pub color: [f64; 4],
}

/// Synthetic italic's lean: iOS slants the system font 12° where it draws it oblique.
pub const ITALIC_SKEW: f32 = 0.2126;

/// Clip paths in force (in the context's base coordinates; even-odd flag), all intersected.
type Clip = Vec<(Vec<El>, bool)>;

#[derive(Clone, Debug)]
pub(crate) enum Op {
    Fill { path: Vec<El>, color: [f32; 4], even_odd: bool, clip: Clip },
    Stroke { path: Vec<El>, color: [f32; 4], width: f32, line: Line, clip: Clip },
    Glyphs { glyphs: Vec<Glyph>, ctm: [f32; 6], clip: Clip },
    /// CTFontDrawGlyphs: glyph ids, each with its font-units → context-base transform.
    FontGlyphs { font: FontRef, glyphs: Vec<(u16, [f32; 6])>, color: [f32; 4], clip: Clip },
    /// An image: its pixels through `m` (pixel space → context base).
    Image { pix: std::rc::Rc<Pixmap>, m: [f32; 6], alpha: f32, clip: Clip },
}

/// How a stroke's ends and corners are drawn: CGLineCap (butt 0, round 1,
/// square 2) and CGLineJoin (miter 0, round 1, bevel 2).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Line {
    pub cap: u8,
    pub join: u8,
    /// CGContextSetLineDash: the on/off lengths (up to 8; none: solid) and
    /// where in them the line starts, in the units the stroke is drawn in.
    pub dash: [f32; 8],
    pub dash_len: u8,
    pub dash_phase: f32,
}

impl Line {
    /// The same style with its dash lengths scaled by `k` (user space to the context's base).
    fn scaled(self, k: f32) -> Line {
        Line { dash: self.dash.map(|v| v * k), dash_phase: self.dash_phase * k, ..self }
    }

    fn stroke(self, width: f32) -> Stroke {
        // An odd number of lengths repeats, as Core Graphics reads them.
        let n = self.dash_len as usize;
        let mut lengths: Vec<f32> = self.dash[..n].to_vec();
        if n % 2 == 1 {
            lengths.extend_from_within(..);
        }
        let dash = if lengths.iter().sum::<f32>() > 0.0 { tiny_skia::StrokeDash::new(lengths, self.dash_phase) } else { None };
        Stroke {
            dash,
            width,
            line_cap: match self.cap {
                1 => tiny_skia::LineCap::Round,
                2 => tiny_skia::LineCap::Square,
                _ => tiny_skia::LineCap::Butt,
            },
            line_join: match self.join {
                1 => tiny_skia::LineJoin::Round,
                2 => tiny_skia::LineJoin::Bevel,
                _ => tiny_skia::LineJoin::Miter,
            },
            ..Default::default()
        }
    }
}

/// A font file and the variation it's drawn at.
#[derive(Clone)]
pub(crate) struct FontRef {
    pub bytes: std::rc::Rc<Vec<u8>>,
    pub index: u32,
    pub color: bool,
    pub vars: Vec<([u8; 4], f32)>,
}

impl std::fmt::Debug for FontRef {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FontRef({} bytes, {:?})", self.bytes.len(), self.vars)
    }
}

/// Collects a glyph outline as path elements (quadratics raised to cubics).
#[derive(Default)]
pub(crate) struct ElBuilder {
    pub els: Vec<El>,
    last: (f32, f32),
}

impl ttf_parser::OutlineBuilder for ElBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.els.push(El::Move(x, y));
        self.last = (x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.els.push(El::Line(x, y));
        self.last = (x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.last;
        let c1 = (x0 + 2.0 / 3.0 * (x1 - x0), y0 + 2.0 / 3.0 * (y1 - y0));
        let c2 = (x + 2.0 / 3.0 * (x1 - x), y + 2.0 / 3.0 * (y1 - y));
        self.els.push(El::Cubic(c1.0, c1.1, c2.0, c2.1, x, y));
        self.last = (x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.els.push(El::Cubic(x1, y1, x2, y2, x, y));
        self.last = (x, y);
    }
    fn close(&mut self) {
        self.els.push(El::Close);
    }
}

/// A CGBitmapContext's pixels in guest memory.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bitmap {
    pub data: u64,
    pub w: u32,
    pub h: u32,
    pub bpr: u64,
    /// Bytes per pixel (1 or 4).
    pub bpp: u32,
    /// CGBitmapInfo.
    pub info: u32,
}

#[derive(Clone, Debug)]
struct GState {
    ctm: [f32; 6],
    fill: [f32; 4],
    stroke: [f32; 4],
    line_width: f32,
    line: Line,
    clip: Clip,
}

impl Default for GState {
    fn default() -> Self {
        GState { ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], fill: [0.0, 0.0, 0.0, 1.0], stroke: [0.0, 0.0, 0.0, 1.0], line_width: 1.0, line: Line::default(), clip: Vec::new() }
    }
}

#[derive(Clone, Default)]
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
    /// UIImageOrientation: how the pixels are turned to show upright (0, up: as they are).
    pub orientation: u8,
    /// How it's drawn (template, tinted) and what it is (a symbol).
    pub look: ImageLook,
}

/// How an image is drawn, beyond its pixels.
#[derive(Clone, Debug, Default)]
pub struct ImageLook {
    /// UIImageRenderingMode: automatic (0), alwaysOriginal (1), alwaysTemplate (2).
    pub rendering_mode: u8,
    /// A template image's own color (`imageWithTintColor:`), a UIColor; 0: the view's tint.
    pub tint: u64,
    /// Its baseline, from the bottom (a symbol's), in points.
    pub baseline: Option<f64>,
    /// The symbol it is (`systemImageNamed:`), and its configuration.
    pub symbol: Option<crate::symbols::Symbol>,
    /// Drawn mirrored in a right-to-left view (`imageFlippedForRightToLeftLayoutDirection`).
    pub flips_rtl: bool,
}

impl ImageLook {
    /// Whether it's drawn mirrored in a right-to-left view: flipped for it,
    /// or a symbol that points forward or backward (`chevron.forward` points
    /// left in right to left, `chevron.right` doesn't: fixtures/rtlcontrols).
    pub fn mirrors_in_rtl(&self) -> bool {
        self.flips_rtl || self.symbol.as_ref().is_some_and(|s| s.name.split('.').any(|p| p == "forward" || p == "backward"))
    }

    /// Whether it's drawn as a template: its alpha in a tint color (a symbol
    /// is, unless it's made always original).
    pub fn is_template(&self) -> bool {
        self.rendering_mode == 2 || (self.rendering_mode == 0 && self.symbol.is_some())
    }
}

#[derive(Clone, Default)]
pub struct State {
    pub images: HashMap<u64, Image>,
    contexts: HashMap<u64, Ctx>,
    /// UIGraphicsPushContext stack; the top is the current context.
    pub(crate) stack: Vec<u64>,
    paths: HashMap<u64, Vec<El>>,
    bitmaps: HashMap<u64, Bitmap>,
    glyph_dumps: usize,
    /// What each view drew in its last `drawRect:`, and at what size.
    pub(crate) content: HashMap<u64, (Vec<Op>, [f64; 2])>,
    /// Blurred shadows of `shadowPath`s, which only move from frame to frame
    /// (a push's cards): keyed by everything that shapes them, with where the
    /// blur sits relative to the layer's origin.
    shadows: HashMap<ShadowKey, (std::rc::Rc<Pixmap>, f32, f32)>,
}

/// A path's shape as a hash (FNV-1a over its elements): a path mutated in place is a new shape.
fn path_hash(els: &[El]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |x: u32| {
        h ^= x as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    for e in els {
        match *e {
            El::Move(x, y) => [0, x.to_bits(), y.to_bits()].into_iter().for_each(&mut mix),
            El::Line(x, y) => [1, x.to_bits(), y.to_bits()].into_iter().for_each(&mut mix),
            El::Cubic(a, b, c, d, x, y) => [2, a.to_bits(), b.to_bits(), c.to_bits(), d.to_bits(), x.to_bits(), y.to_bits()].into_iter().for_each(&mut mix),
            El::Close => mix(3),
        }
    }
    h
}

/// What a path shadow's pixels depend on: the path's shape, the layer's size and
/// linear transform (scale, rotation), the blur, colour and strength (all as bits).
type ShadowKey = (u64, [u32; 2], [u32; 4], u32, [u64; 4], u32);

fn apply(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// `m1` then `m2`.
pub(crate) fn concat(m1: &[f32; 6], m2: &[f32; 6]) -> [f32; 6] {
    let [a, b, c, d, tx, ty] = *m1;
    let [a2, b2, c2, d2, tx2, ty2] = *m2;
    [a * a2 + b * c2, a * b2 + b * d2, c * a2 + d * c2, c * b2 + d * d2, tx * a2 + ty * c2 + tx2, tx * b2 + ty * d2 + ty2]
}

pub(crate) fn transform_els(els: &[El], m: &[f32; 6]) -> Vec<El> {
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

pub(crate) fn rect_els(x: f32, y: f32, w: f32, h: f32) -> Vec<El> {
    vec![El::Move(x, y), El::Line(x + w, y), El::Line(x + w, y + h), El::Line(x, y + h), El::Close]
}

/// Circular arc as cubic Béziers (≤ 90° each), starting with a line or move to its start.
pub(crate) fn arc_els(out: &mut Vec<El>, cx: f32, cy: f32, r: f32, start: f32, end: f32, clockwise: bool) {
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

/// A rectangle with rounded corners, as a layer's `cornerRadius`,
/// `maskedCorners` (1 top-left, 2 top-right, 4 bottom-left, 8 bottom-right)
/// and `cornerCurve` shape it. Continuous corners are Apple's: the curve eases
/// into the straight edge over 1.528 × the radius (this reconstruction of
/// the path matches iOS 26's sheet corners to 0.2 pt).
pub(crate) fn corner_shape(w: f32, h: f32, r: f32, mask: u64, continuous: bool) -> Vec<El> {
    let on = |bit: u64| if mask & bit != 0 { r } else { 0.0 };
    corner_shape_radii(w, h, [on(1), on(2), on(4), on(8)], continuous)
}

/// As [`corner_shape`], each corner with its own radius (top-left, top-right,
/// bottom-left, bottom-right; 0: square).
pub(crate) fn corner_shape_radii(w: f32, h: f32, radii: [f32; 4], continuous: bool) -> Vec<El> {
    // Continuous corners too big for their curve to fit are round, as Core
    // Animation draws them: a radius of half the side is a circle's.
    let continuous = continuous && radii.iter().all(|&r| r * CONTINUOUS_REACH <= w.min(h) / 2.0);
    let (radii, reach) = clamp_radii(w, h, radii, continuous);
    if radii.iter().all(|&r| r == 0.0) {
        return rect_els(0.0, 0.0, w, h);
    }
    let mut out = Vec::new();
    // Start on the top edge, where the top-left corner leaves it.
    out.push(El::Move(reach * radii[0], 0.0));
    for &(c, u, v, r) in &corners_of(w, h, radii) {
        if r == 0.0 {
            out.push(El::Line(c.0, c.1));
            continue;
        }
        corner_curve(&mut out, c, u, v, r, reach, continuous);
    }
    out.push(El::Close);
    out
}

/// How far along each edge a continuous corner reaches, per unit of radius.
const CONTINUOUS_REACH: f32 = 1.528_664_8;

/// The radii as drawn (no corner reaching past the middle of an edge), and
/// how far along its edges a corner reaches, per unit of radius.
fn clamp_radii(w: f32, h: f32, radii: [f32; 4], continuous: bool) -> ([f32; 4], f32) {
    let reach = if continuous { CONTINUOUS_REACH } else { 1.0 };
    (radii.map(|r| r.max(0.0).min(w / 2.0 / reach).min(h / 2.0 / reach)), reach)
}

/// A corner: its point, the direction arriving at it, the direction leaving it, its radius.
type Corner = ((f32, f32), (f32, f32), (f32, f32), f32);

/// The corners clockwise from top-right (radii: top-left, top-right, bottom-left, bottom-right).
fn corners_of(w: f32, h: f32, radii: [f32; 4]) -> [Corner; 4] {
    [
        ((w, 0.0), (1.0, 0.0), (0.0, 1.0), radii[1]),
        ((w, h), (0.0, 1.0), (-1.0, 0.0), radii[3]),
        ((0.0, h), (-1.0, 0.0), (0.0, -1.0), radii[2]),
        ((0.0, 0.0), (0.0, -1.0), (1.0, 0.0), radii[0]),
    ]
}

/// A rounded corner from where it leaves the arriving edge (a line to there)
/// to where it meets the leaving one.
fn corner_curve(out: &mut Vec<El>, c: (f32, f32), u: (f32, f32), v: (f32, f32), r: f32, reach: f32, continuous: bool) {
    // A corner as cubics in (a, b): a back along the edge arriving at the
    // corner, b along the edge leaving it, in units of the radius.
    const CONTINUOUS: [[(f32, f32); 3]; 3] = [
        [(1.088_493_2, 0.0), (0.868_406_9, 0.0), (0.631_496, 0.074_911)],
        [(0.372_823_9, 0.169_059), (0.169_059, 0.372_823_9), (0.074_911, 0.631_496)],
        [(0.0, 0.868_406_9), (0.0, 1.088_493_2), (0.0, 1.528_664_8)],
    ];
    const K: f32 = 0.447_715; // 1 − 0.552285, the circle's cubic handles
    const CIRCULAR: [[(f32, f32); 3]; 1] = [[(K, 0.0), (0.0, K), (0.0, 1.0)]];
    let at = |a: f32, b: f32| (c.0 - a * r * u.0 + b * r * v.0, c.1 - a * r * u.1 + b * r * v.1);
    let start = at(reach, 0.0);
    out.push(El::Line(start.0, start.1));
    let segs: &[[(f32, f32); 3]] = if continuous { &CONTINUOUS } else { &CIRCULAR };
    for seg in segs {
        let (p1, p2, p3) = (at(seg[0].0, seg[0].1), at(seg[1].0, seg[1].1), at(seg[2].0, seg[2].1));
        out.push(El::Cubic(p1.0, p1.1, p2.0, p2.1, p3.0, p3.1));
    }
}

/// The display's shape held this way, as an SVG path in points (its corners:
/// continuous on Face ID iPhones, as moving screens' cards are, so the two
/// coincide), for a host that clips the screen itself (the browser page).
pub fn display_svg_path(d: &crate::device::Device, o: crate::device::Orientation) -> String {
    let (w, h) = d.screen(o);
    let shape = corner_shape(w as f32, h as f32, d.corner_radius as f32, 15, !d.home_button);
    let mut out = String::new();
    for e in shape {
        out += &match e {
            El::Move(x, y) => format!("M{x:.3} {y:.3}"),
            El::Line(x, y) => format!("L{x:.3} {y:.3}"),
            El::Cubic(a, b, c, d, e, f) => format!("C{a:.3} {b:.3} {c:.3} {d:.3} {e:.3} {f:.3}"),
            El::Close => "Z".into(),
        };
    }
    out
}

/// A path that's a rectangle with continuous corners of radius `r` (0:
/// square), for pomme's own layers.
pub(crate) fn new_card_path(emu: &mut Emu, size: [f64; 2], r: f64) -> Result<u64> {
    let id = emu.alloc(16)?;
    emu.render.paths.insert(id, corner_shape(size[0] as f32, size[1] as f32, r as f32, 15, true));
    Ok(id)
}

/// A layer's corner radii (top-left, top-right, bottom-left, bottom-right).
fn layer_radii(ly: &Layer) -> [f32; 4] {
    match ly.corner_radii {
        Some(r) => r.map(|v| v as f32),
        None => {
            let r = ly.corner_radius as f32;
            [1, 2, 4, 8].map(|bit| if ly.masked_corners & bit != 0 { r } else { 0.0 })
        }
    }
}

/// A layer's own shape, `w` × `h`.
fn layer_shape(ly: &Layer, w: f32, h: f32) -> Vec<El> {
    match ly.corner_radii {
        Some(r) => corner_shape_radii(w, h, r.map(|v| v as f32), ly.continuous_corners),
        None => corner_shape(w, h, ly.corner_radius as f32, ly.masked_corners, ly.continuous_corners),
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
    flush_bitmap(emu, c)
}

// ---- bitmap contexts: drawing lands in guest memory right away ----

/// (r, g, b, a) byte offsets in a 4-byte pixel, and whether alpha is ignored.
pub(crate) fn pixel_layout(info: u32) -> ([usize; 4], bool) {
    let alpha = info & 0x1f;
    let little = info & 0x7000 == 0x2000;
    let first = matches!(alpha, 2 | 4 | 6); // PremultipliedFirst, First, NoneSkipFirst
    let skip = matches!(alpha, 0 | 5 | 6);
    // In big-endian order: ARGB (first) or RGBA (last); little-endian reverses.
    let big = if first { [1, 2, 3, 0] } else { [0, 1, 2, 3] };
    (if little { big.map(|i| 3 - i) } else { big }, skip)
}

/// A bitmap context's pixels as they are in guest memory.
fn read_bitmap(emu: &Emu, b: &Bitmap) -> Result<Option<Pixmap>> {
    let Some(mut pix) = Pixmap::new(b.w, b.h) else { return Ok(None) };
    let alpha_only = b.bpp == 1 && b.info & 0x1f == 7;
    let (order, skip) = pixel_layout(b.info);
    for y in 0..b.h {
        let row = emu.p.memory.read(b.data + y as u64 * b.bpr, (b.w * b.bpp) as usize)?.to_vec();
        for x in 0..b.w as usize {
            let px = if b.bpp == 1 {
                let v = row[x];
                if alpha_only { tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, v) } else { tiny_skia::PremultipliedColorU8::from_rgba(v, v, v, 255) }
            } else {
                let p = &row[x * 4..x * 4 + 4];
                let a = if skip { 255 } else { p[order[3]] };
                tiny_skia::PremultipliedColorU8::from_rgba(p[order[0]].min(a), p[order[1]].min(a), p[order[2]].min(a), a)
            };
            if let Some(px) = px {
                pix.pixels_mut()[y as usize * b.w as usize + x] = px;
            }
        }
    }
    Ok(Some(pix))
}

/// Draw an image into context `c` filling `rect` (user space). CoreGraphics
/// draws it with its top at the rect's max y (y-up); UIKit's drawing
/// (`-[UIImage drawInRect:]`) with its top at the rect's min y.
pub(crate) fn draw_image(emu: &mut Emu, c: u64, pix: std::rc::Rc<Pixmap>, rect: [f64; 4], uikit: bool, alpha: f32) -> Result<()> {
    let (pw, ph) = (pix.width() as f32, pix.height() as f32);
    let r = rect.map(|v| v as f32);
    let place = if uikit { [r[2] / pw, 0.0, 0.0, r[3] / ph, r[0], r[1]] } else { [r[2] / pw, 0.0, 0.0, -r[3] / ph, r[0], r[1] + r[3]] };
    if let Some(ctx) = ctx_mut(emu, c) {
        let m = concat(&place, &ctx.gs.ctm);
        let clip = ctx.gs.clip.clone();
        ctx.ops.push(Op::Image { pix, m, alpha, clip });
    }
    flush_bitmap(emu, c)
}

fn flush_bitmap(emu: &mut Emu, c: u64) -> Result<()> {
    let Some(b) = emu.render.bitmaps.get(&c).copied() else { return Ok(()) };
    let ops = match emu.render.contexts.get_mut(&c) {
        Some(x) if !x.ops.is_empty() => std::mem::take(&mut x.ops),
        _ => return Ok(()),
    };
    let Some(pix) = read_bitmap(emu, &b)? else { return Ok(()) };
    let alpha_only = b.bpp == 1 && b.info & 0x1f == 7;
    let (order, skip) = pixel_layout(b.info);
    let (font, emoji) = (crate::textkit::font_bytes(emu), crate::textkit::emoji_bytes(emu));
    let mut p = Painter { pix, font: font.as_deref().map(|v| v.as_slice()), emoji: emoji.as_deref().map(|v| v.as_slice()), faces: crate::coretext::draw_faces(emu) };
    p.ops(&ops, 1.0, &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0], None);
    for y in 0..b.h {
        let mut row = vec![0u8; (b.w * b.bpp) as usize];
        for x in 0..b.w as usize {
            let c = p.pix.pixels()[y as usize * b.w as usize + x];
            if b.bpp == 1 {
                row[x] = if alpha_only {
                    c.alpha()
                } else {
                    // Gray: luminance over black.
                    ((c.red() as u32 * 77 + c.green() as u32 * 150 + c.blue() as u32 * 29) >> 8) as u8
                };
            } else {
                let px = &mut row[x * 4..x * 4 + 4];
                px[order[0]] = c.red();
                px[order[1]] = c.green();
                px[order[2]] = c.blue();
                px[order[3]] = if skip { 255 } else { c.alpha() };
            }
        }
        emu.p.memory.write(b.data + y as u64 * b.bpr, &row)?;
    }
    Ok(())
}

/// `CGBitmapContextCreate(data, width, height, bitsPerComponent, bytesPerRow, space, bitmapInfo)`.
fn bitmap_create(emu: &mut Emu) -> Result<()> {
    let (data, w, h, bpc, bpr, info) = (emu.arg(0), emu.arg(1) as u32, emu.arg(2) as u32, emu.arg(3), emu.arg(4), emu.arg(6) as u32);
    if w == 0 || h == 0 || bpc != 8 {
        emu.ret(0);
        return Ok(());
    }
    // One component: alpha-only (kCGImageAlphaOnly) or gray with no alpha.
    let bpp = if info & 0x1f == 7 || (bpr != 0 && bpr < w as u64 * 4 && bpr >= w as u64) { 1 } else { 4 };
    let bpr = if bpr == 0 { w as u64 * bpp as u64 } else { bpr };
    let data = if data == 0 {
        let d = emu.alloc(bpr * h as u64)?;
        emu.p.memory.write(d, &vec![0; (bpr * h as u64) as usize])?;
        d
    } else {
        data
    };
    let cls = objc::host_class(emu, "CGContext")?;
    let c = objc::create_instance(emu, cls, 0)?;
    let mut ctx = Ctx::default();
    // CoreGraphics' origin is bottom-left.
    ctx.gs.ctm = [1.0, 0.0, 0.0, -1.0, 0.0, h as f32];
    emu.render.contexts.insert(c, ctx);
    emu.render.bitmaps.insert(c, Bitmap { data, w, h, bpr, bpp, info });
    emu.ret(c);
    Ok(())
}

fn bitmap_get(emu: &mut Emu, f: fn(&Bitmap) -> u64) -> Result<()> {
    let v = emu.render.bitmaps.get(&emu.arg(0)).map_or(0, f);
    emu.ret(v);
    Ok(())
}

/// `CTFontDrawGlyphs` into a context: `glyphs` are (id, x, y) in user space.
pub(crate) fn draw_font_glyphs(emu: &mut Emu, c: u64, face: &crate::coretext::Face, font: &crate::coretext::Font, glyphs: &[(u16, f64, f64)]) -> Result<()> {
    let Ok(parsed) = ttf_parser::Face::parse(&face.bytes, face.index) else { return Ok(()) };
    let s = (font.size / parsed.units_per_em() as f64) as f32;
    let m = font.matrix.map(|v| v as f32);
    let vars = if face.variable_weight {
        vec![(*b"wght", crate::textkit::ot_weight(font.weight) as f32), (*b"opsz", font.size.clamp(14.0, 32.0) as f32)]
    } else {
        // A named instance (or a fallback face's weight): its place on the axes.
        face.coords.clone()
    };
    let fr = FontRef { bytes: face.bytes.clone(), index: face.index, color: face.color, vars };
    let Some(ctx) = ctx_mut(emu, c) else { return Ok(()) };
    let glyph = [m[0] * s, m[1] * s, m[2] * s, m[3] * s, m[4], m[5]];
    let list = glyphs.iter().map(|&(g, x, y)| (g, concat(&concat(&glyph, &[1.0, 0.0, 0.0, 1.0, x as f32, y as f32]), &ctx.gs.ctm))).collect();
    ctx.ops.push(Op::FontGlyphs { font: fr, glyphs: list, color: ctx.gs.fill, clip: ctx.gs.clip.clone() });
    flush_bitmap(emu, c)?;
    // Debugging aid: POMME_DUMP_GLYPHS=dir writes each bitmap context after glyphs land.
    if let (Ok(dir), Some(b)) = (std::env::var("POMME_DUMP_GLYPHS"), emu.render.bitmaps.get(&c).copied()) {
        let mut rgba = Vec::new();
        for y in 0..b.h {
            let row = emu.p.memory.read(b.data + y as u64 * b.bpr, (b.w * b.bpp) as usize)?.to_vec();
            for x in 0..b.w as usize {
                if b.bpp == 1 {
                    rgba.extend_from_slice(&[row[x], row[x], row[x], 255]);
                } else {
                    rgba.extend_from_slice(&[row[x * 4], row[x * 4 + 1], row[x * 4 + 2], 255]);
                }
            }
        }
        let n = emu.render.glyph_dumps;
        emu.render.glyph_dumps += 1;
        if let Some(pix) = tiny_skia::IntSize::from_wh(b.w, b.h).and_then(|s| Pixmap::from_vec(rgba, s)) {
            let _ = pix.save_png(format!("{dir}/glyph-{n:03}.png"));
        }
        println!("[glyphs] {n}: {:?} in {}×{} ctx, ctm {:?}", glyphs, b.w, b.h, ctx_mut(emu, c).map(|x| x.gs.ctm));
    }
    Ok(())
}

/// `CGPathApply(path, info, function)`: the function gets each CGPathElement {type, points}.
fn path_apply(emu: &mut Emu) -> Result<()> {
    let (p, info, f) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let els = path_els(emu, p);
    let el = emu.alloc(16)?;
    let pts = emu.alloc(48)?;
    emu.write_u64(el + 8, pts)?;
    for e in els {
        let (ty, coords): (u32, Vec<f32>) = match e {
            El::Move(x, y) => (0, vec![x, y]),
            El::Line(x, y) => (1, vec![x, y]),
            El::Cubic(a, b, c, d, x, y) => (3, vec![a, b, c, d, x, y]),
            El::Close => (4, vec![]),
        };
        emu.p.memory.write_u32(el, ty)?;
        for (i, v) in coords.iter().enumerate() {
            emu.write_u64(pts + i as u64 * 8, (*v as f64).to_bits())?;
        }
        emu.call(f, &[info, el])?;
    }
    emu.heap.free(&emu.p.memory, el)?;
    emu.heap.free(&emu.p.memory, pts)?;
    Ok(())
}

pub(crate) fn path_bounds(els: &[El]) -> [f64; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut add = |x: f32, y: f32| {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    };
    for e in els {
        match *e {
            El::Move(x, y) | El::Line(x, y) => add(x, y),
            El::Cubic(a, b, c, d, x, y) => {
                add(a, b);
                add(c, d);
                add(x, y);
            }
            El::Close => {}
        }
    }
    if x0 > x1 {
        // CGRectNull
        return [f64::INFINITY, f64::INFINITY, 0.0, 0.0];
    }
    [x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64]
}

fn ret_rect(emu: &mut Emu, r: [f64; 4]) -> Result<()> {
    for (i, v) in r.iter().enumerate() {
        emu.cpu.q[i] = v.to_bits() as u128;
    }
    Ok(())
}

fn last_point(els: &[El]) -> (f32, f32) {
    let mut start = (0.0, 0.0);
    let mut cur = (0.0, 0.0);
    for e in els {
        match *e {
            El::Move(x, y) => {
                start = (x, y);
                cur = (x, y);
            }
            El::Line(x, y) | El::Cubic(.., x, y) => cur = (x, y),
            El::Close => cur = start,
        }
    }
    cur
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

/// The current UIKit graphics context (`UIGraphicsGetCurrentContext`), if any.
fn with_current(emu: &mut Emu, f: impl FnOnce(&mut Ctx)) -> Result<()> {
    let Some(c) = emu.render.stack.last().copied() else { return Ok(()) };
    if let Some(ctx) = ctx_mut(emu, c) {
        f(ctx);
    }
    flush_bitmap(emu, c)
}

/// An ellipse in a rectangle, as four cubics (from its right, through its bottom).
pub(crate) fn ellipse_els(x: f32, y: f32, w: f32, h: f32) -> Vec<El> {
    const K: f32 = 0.552_284_8;
    let (rx, ry, cx, cy) = (w / 2.0, h / 2.0, x + w / 2.0, y + h / 2.0);
    let (kx, ky) = (rx * K, ry * K);
    vec![
        El::Move(cx + rx, cy),
        El::Cubic(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
        El::Cubic(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
        El::Cubic(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
        El::Cubic(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
        El::Close,
    ]
}

/// Stroke a path (in user space) in the current UIKit context, with its own
/// width and line style and the context's stroke color (UIBezierPath's `stroke`).
pub(crate) fn stroke_in_current(emu: &mut Emu, els: &[El], width: f32, line: Line) -> Result<()> {
    with_current(emu, |c| {
        let path = transform_els(els, &c.gs.ctm);
        let width = width * c.gs.ctm[0].abs().max(c.gs.ctm[3].abs());
        c.ops.push(Op::Stroke { path, color: c.gs.stroke, width, line, clip: c.gs.clip.clone() });
    })
}

/// Fill a path (in user space) in the current UIKit context with its fill color.
pub(crate) fn fill_in_current(emu: &mut Emu, els: &[El], even_odd: bool) -> Result<()> {
    with_current(emu, |c| {
        let path = transform_els(els, &c.gs.ctm);
        c.ops.push(Op::Fill { path, color: c.gs.fill, even_odd, clip: c.gs.clip.clone() });
    })
}

/// Fill rectangles (x, y, width, height in user space) in `color` in the
/// current UIKit context: text decorations (underlines, strikethroughs).
pub(crate) fn fill_rects_in_current(emu: &mut Emu, rects: &[[f64; 4]], color: [f64; 4]) -> Result<()> {
    if rects.is_empty() {
        return Ok(());
    }
    with_current(emu, |c| {
        let mut path = Vec::new();
        for r in rects {
            path.extend(transform_els(&rect_els(r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32), &c.gs.ctm));
        }
        c.ops.push(Op::Fill { path, color: color.map(|v| v as f32), even_odd: false, clip: c.gs.clip.clone() });
    })
}

/// Intersect the current UIKit context's clip with a path (in user space).
pub(crate) fn clip_in_current(emu: &mut Emu, els: &[El], even_odd: bool) -> Result<()> {
    with_current(emu, |c| {
        let path = transform_els(els, &c.gs.ctm);
        c.gs.clip.push((path, even_odd));
    })
}

/// `UIRectFill(rect)`: fill with the current context's fill color.
fn ui_rect_fill(emu: &mut Emu) -> Result<()> {
    let r = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    with_current(emu, |c| {
        let els = transform_els(&rect_els(r[0], r[1], r[2], r[3]), &c.gs.ctm);
        c.ops.push(Op::Fill { path: els, color: c.gs.fill, even_odd: false, clip: c.gs.clip.clone() });
    })
}

/// UIColor's `set`, `setFill` and `setStroke`: the current context's colors.
pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIColor", false, "setFill", |e| {
        let col = rgba(e, e.arg(0));
        with_current(e, |c| c.gs.fill = col)
    }),
    ("UIColor", false, "setStroke", |e| {
        let col = rgba(e, e.arg(0));
        with_current(e, |c| c.gs.stroke = col)
    }),
    ("UIColor", false, "set", |e| {
        let col = rgba(e, e.arg(0));
        with_current(e, |c| {
            c.gs.fill = col;
            c.gs.stroke = col;
        })
    }),
];

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

pub(crate) fn path_els(emu: &Emu, p: u64) -> Vec<El> {
    emu.render.paths.get(&p).cloned().unwrap_or_default()
}

/// Replace a path's elements (a mutable path's, as UIBezierPath keeps one).
pub(crate) fn set_path_els(emu: &mut Emu, p: u64, els: Vec<El>) {
    emu.render.paths.insert(p, els);
}

pub(crate) fn new_path(emu: &mut Emu, els: Vec<El>) -> Result<u64> {
    let cls = objc::host_class(emu, "CGPath")?;
    let p = objc::create_instance(emu, cls, 0)?;
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

/// Run the guest's `-drawRect:` overrides where they're due, recording what
/// each view draws. As on iOS, a view draws when it first shows, when it's
/// marked with `setNeedsDisplay`, and when its size changes; otherwise what it
/// drew last is shown again. (iOS stretches the old bitmap after a size change
/// until the view redraws, unless its content mode is Redraw; pomme records
/// drawing as shapes, so it redraws at the new size instead.)
fn display(emu: &mut Emu, v: u64) -> Result<()> {
    let vw = view(emu, v);
    let size = [vw.bounds[2], vw.bounds[3]];
    let due = vw.needs_display || emu.render.content.get(&v).is_none_or(|c| c.1 != size);
    if !vw.hidden && due {
        crate::uiview::view_entry(emu, v).needs_display = false;
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
            emu.render.content.insert(v, (ops, size));
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
    /// CoreText's faces (the app's fonts), for glyphs set in one.
    faces: crate::coretext::DrawFaces,
}

impl Painter<'_> {
    /// Make a `w` × `h` rectangle transparent (whatever was drawn there goes).
    fn clear(&mut self, w: f32, h: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) else { return };
        let mut paint = tiny_skia::Paint { blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() };
        paint.anti_alias = false;
        self.pix.fill_rect(rect, &paint, ts(t), mask);
    }

    fn fill(&mut self, els: &[El], color: [f32; 4], alpha: f32, even_odd: bool, t: &[f32; 6], mask: Option<&Mask>) {
        if let (Some(path), Some(p)) = (build(els), paint(color, alpha)) {
            let rule = if even_odd { FillRule::EvenOdd } else { FillRule::Winding };
            self.pix.fill_path(&path, &p, rule, ts(t), mask);
        }
    }

    /// Fill a `w` × `h` box with these corners. Unrotated and big, as most
    /// layers' backgrounds are, its inside is filled as rectangles (fast) and
    /// only the corners as paths, cut at whole pixels so the pieces meet
    /// without seams; the same pixels as filling its shape, at a fraction of
    /// the cost (anti-aliased paths are rasterised at 4× resolution).
    #[allow(clippy::too_many_arguments)]
    fn fill_box(&mut self, w: f32, h: f32, radii: [f32; 4], continuous: bool, color: [f32; 4], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let [sx, ky, kx, sy, tx, ty] = *t;
        let axis_aligned = ky == 0.0 && kx == 0.0 && sx > 0.0 && sy > 0.0;
        // Small boxes: the path is as quick.
        if !axis_aligned || w * sx * h * sy < 64.0 * 64.0 || w <= 0.0 || h <= 0.0 {
            let (radii, _) = clamp_radii(w, h, radii, continuous);
            self.fill(&corner_shape_radii(w, h, radii, continuous), color, alpha, false, t, mask);
            return;
        }
        let Some(paint) = paint(color, alpha) else { return };
        let (radii, reach) = clamp_radii(w, h, radii, continuous);
        // In device pixels: the box, and how far each corner reaches along its edges.
        let (x0, y0, x1, y1) = (tx, ty, tx + w * sx, ty + h * sy);
        let rx = radii.map(|r| r * reach * sx);
        let ry = radii.map(|r| r * reach * sy);
        // Where the inside starts: past every corner on that side, on a whole pixel.
        let left = (x0 + rx[0].max(rx[2])).ceil().min(x1);
        let right = (x1 - rx[1].max(rx[3])).floor().max(left);
        let top = (y0 + ry[0].max(ry[1])).ceil().min(y1);
        let bottom = (y1 - ry[2].max(ry[3])).floor().max(top);
        let rect = |pix: &mut Pixmap, x: f32, y: f32, x2: f32, y2: f32| {
            if let Some(r) = tiny_skia::Rect::from_ltrb(x, y, x2, y2) {
                pix.fill_rect(r, &paint, Transform::identity(), mask);
            }
        };
        // The middle, full height; the sides between the corners.
        rect(&mut self.pix, left, y0, right, y1);
        rect(&mut self.pix, x0, top, left, bottom);
        rect(&mut self.pix, right, top, x1, bottom);
        // Each corner's box: the corner, and what's inside it up to the inside's edges.
        let device = |(x, y): (f32, f32)| (x0 + x * sx, y0 + y * sy);
        for (i, &(c, u, v, r)) in corners_of(w, h, radii).iter().enumerate() {
            // (corners_of goes top-right, bottom-right, bottom-left, top-left)
            let (bx, by) = match i {
                0 => (right, top),
                1 => (right, bottom),
                2 => (left, bottom),
                _ => (left, top),
            };
            let cd = device(c);
            let (box_l, box_r) = (cd.0.min(bx), cd.0.max(bx));
            let (box_t, box_b) = (cd.1.min(by), cd.1.max(by));
            if box_r <= box_l || box_b <= box_t {
                continue;
            }
            if r == 0.0 {
                rect(&mut self.pix, box_l, box_t, box_r, box_b);
                continue;
            }
            // The corner's box corner on the arriving edge, the curve, the
            // box corner on the leaving edge, and the inner corner.
            let on_edge = |d: (f32, f32)| (if d.0 != 0.0 { bx } else { cd.0 }, if d.1 != 0.0 { by } else { cd.1 });
            let arrive = on_edge(u);
            let leave = on_edge(v);
            let mut els = vec![El::Move(arrive.0, arrive.1)];
            let mut curve = Vec::new();
            corner_curve(&mut curve, c, u, v, r, reach, continuous);
            els.extend(curve.into_iter().map(|e| match e {
                El::Line(x, y) => {
                    let (x, y) = device((x, y));
                    El::Line(x, y)
                }
                El::Cubic(a, b, c2, d, e2, f) => {
                    let ((a, b), (c2, d), (e2, f)) = (device((a, b)), device((c2, d)), device((e2, f)));
                    El::Cubic(a, b, c2, d, e2, f)
                }
                other => other,
            }));
            els.push(El::Line(leave.0, leave.1));
            els.push(El::Line(bx, by));
            els.push(El::Close);
            if let Some(path) = build(&els) {
                self.pix.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), mask);
            }
        }
    }

    fn stroke(&mut self, els: &[El], color: [f32; 4], alpha: f32, width: f32, t: &[f32; 6], mask: Option<&Mask>) {
        self.stroke_line(els, color, alpha, width, Line::default(), t, mask);
    }

    #[allow(clippy::too_many_arguments)]
    fn stroke_line(&mut self, els: &[El], color: [f32; 4], alpha: f32, width: f32, line: Line, t: &[f32; 6], mask: Option<&Mask>) {
        if let (Some(path), Some(p)) = (build(els), paint(color, alpha)) {
            let s = line.stroke(width);
            self.pix.stroke_path(&path, &p, &s, ts(t), mask);
        }
    }

    /// Take the faces coretext has now, if `glyphs` use one registered since
    /// this painter began (a fallback font, found while laying the text out).
    fn sync_faces(&mut self, emu: &mut Emu, glyphs: &[Glyph]) {
        if glyphs.iter().any(|g| g.face >= self.faces.len()) {
            self.faces = crate::coretext::draw_faces(emu);
        }
    }

    fn glyphs(&mut self, glyphs: &[Glyph], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        // Text shadows first, under the text.
        let mut shadows: Vec<TextShadow> = Vec::new();
        for s in glyphs.iter().filter_map(|g| g.shadow) {
            if !shadows.contains(&s) {
                shadows.push(s);
            }
        }
        for s in shadows {
            let these: Vec<Glyph> = glyphs.iter().filter(|g| g.shadow == Some(s)).map(|g| Glyph { shadow: None, color: [0.0, 0.0, 0.0, 1.0], ..g.clone() }).collect();
            self.glyph_shadow(&these, s, alpha, t, mask);
        }
        let Some(data) = self.font else { return };
        let Ok(mut face) = ttf_parser::Face::parse(data, 0) else { return };
        let upem = face.units_per_em() as f32;
        let mut current: Option<(u16, u16)> = None;
        let faces = self.faces.clone();
        let mut app: Option<(usize, ttf_parser::Face)> = None;
        for g in glyphs {
            // In an app's face (at its named instance); what it lacks, in the system font.
            if g.face != 0 {
                if app.as_ref().map(|a| a.0) != Some(g.face) {
                    app = faces.get(g.face).and_then(|(bytes, index, coords)| {
                        let mut f = ttf_parser::Face::parse(bytes, *index).ok()?;
                        for (tag, v) in coords {
                            f.set_variation(ttf_parser::Tag::from_bytes(tag), *v);
                        }
                        Some((g.face, f))
                    });
                }
                if let Some((_, f)) = &app
                    && let Some(gid) = g.gid.map(ttf_parser::GlyphId).or_else(|| f.glyph_index(g.ch))
                {
                    let font = faces.get(g.face).map_or((0, 0), |(bytes, index, coords)| {
                        let vars = coords.iter().fold(*index as u64, |h, (tag, v)| h.rotate_left(13) ^ u32::from_be_bytes(*tag) as u64 ^ (v.to_bits() as u64) << 7);
                        (bytes.as_ptr() as usize ^ vars as usize, bytes.len())
                    });
                    self.outline_glyph(f, font, gid, g, alpha, t, mask);
                    continue;
                }
            }
            let wght = crate::textkit::ot_weight(g.weight).round() as u16;
            let opsz = g.size.clamp(14.0, 32.0).round() as u16;
            if current != Some((wght, opsz)) {
                face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), wght as f32);
                face.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), opsz as f32);
                current = Some((wght, opsz));
            }
            let Some(gid) = g.gid.map(ttf_parser::GlyphId).or_else(|| face.glyph_index(g.ch)) else {
                self.emoji_glyph(g, alpha, t, mask);
                continue;
            };
            let font = (data.as_ptr() as usize ^ ((wght as usize) << 16 | opsz as usize), data.len());
            self.outline_glyph(&face, font, gid, g, alpha, t, mask);
        }
        let _ = upem;
    }

    /// A text shadow: the glyphs drawn (in black) into a buffer around them,
    /// moved by the shadow's offset, their alpha blurred and painted in the
    /// shadow's color under the text.
    fn glyph_shadow(&mut self, glyphs: &[Glyph], s: TextShadow, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let k = (t[0] * t[0] + t[1] * t[1]).sqrt();
        let sigma = (s.blur / 2.0) as f32 * k;
        let moved: Vec<Glyph> = glyphs.iter().map(|g| Glyph { x: g.x + s.offset[0], y: g.y + s.offset[1], ..g.clone() }).collect();
        // The glyphs' boxes on screen (an em wide, from an em above the baseline to a third below), with room for the blur.
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for g in &moved {
            let (x, y, e) = (g.x as f32, g.y as f32, g.size as f32);
            for (px, py) in [(x - e * 0.3, y - e * 1.2), (x + e * 1.3, y + e * 0.4)] {
                let (sx, sy) = apply(t, px, py);
                b = [b[0].min(sx), b[1].min(sy), b[2].max(sx), b[3].max(sy)];
            }
        }
        let margin = (3.0 * sigma).ceil() + 2.0;
        let (pw, ph) = (self.pix.width() as f32, self.pix.height() as f32);
        let x0 = (b[0] - margin).floor().max(0.0);
        let y0 = (b[1] - margin).floor().max(0.0);
        let x1 = (b[2] + margin).ceil().min(pw);
        let y1 = (b[3] + margin).ceil().min(ph);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let Some(pix) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else { return };
        let mut off = Painter { pix, font: self.font, emoji: self.emoji, faces: self.faces.clone() };
        off.glyphs(&moved, 1.0, &concat(t, &[1.0, 0.0, 0.0, 1.0, -x0, -y0]), None);
        let Some(sh) = blurred_shadow(&off.pix, sigma, s.color, alpha * s.color[3] as f32) else { return };
        self.pix.draw_pixmap(x0 as i32, y0 as i32, sh.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), mask);
    }

    /// An outline glyph in its color: from the glyph cache when the
    /// transform allows, else rasterised in place. `font` names the face and
    /// its variation (for the cache).
    #[allow(clippy::too_many_arguments)]
    fn outline_glyph(&mut self, face: &ttf_parser::Face, font: (usize, usize), gid: ttf_parser::GlyphId, g: &Glyph, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let color = g.color.map(|v| v as f32);
        let s = g.size as f32 / face.units_per_em() as f32;
        let place = [s, 0.0, s * g.skew, -s, g.x as f32, g.y as f32];
        // (A slanted glyph isn't in the glyph cache: drawn in place.)
        if g.skew != 0.0 {
            let mut b = Outline(PathBuilder::new());
            if face.outline_glyph(gid, &mut b).is_some()
                && let (Some(path), Some(p)) = (b.0.finish(), paint(color, alpha))
            {
                self.pix.fill_path(&path, &p, FillRule::Winding, ts(&concat(&place, t)), mask);
            }
            return;
        }
        let outline = || {
            let mut b = Outline(PathBuilder::new());
            face.outline_glyph(gid, &mut b)?;
            b.0.finish()
        };
        let key_color = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        let drawn = glyph_cache::draw(&mut self.pix, (font, gid.0, key_color), face.glyph_bounding_box(gid), s, (g.x as f32, g.y as f32), t, alpha, mask, |pix, base| {
            if let (Some(path), Some(p)) = (outline(), paint(color, 1.0)) {
                pix.fill_path(&path, &p, FillRule::Winding, base, None);
            }
        });
        if !drawn
            && let (Some(path), Some(p)) = (outline(), paint(color, alpha))
        {
            self.pix.fill_path(&path, &p, FillRule::Winding, ts(&concat(&place, t)), mask);
        }
    }

    fn font_glyphs(&mut self, font: &FontRef, glyphs: &[(u16, [f32; 6])], color: [f32; 4], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
        let Ok(mut face) = ttf_parser::Face::parse(&font.bytes, font.index) else { return };
        for (tag, v) in &font.vars {
            face.set_variation(ttf_parser::Tag::from_bytes(tag), *v);
        }
        for (g, place) in glyphs {
            let gid = ttf_parser::GlyphId(*g);
            let base = ts(&concat(place, t));
            if font.color && face.is_color_glyph(gid) {
                let mut painter = ColrPainter { face: &face, pix: &mut self.pix, base, transforms: Vec::new(), clips: Vec::new(), outline: None, alpha, mask };
                face.paint_color_glyph(gid, 0, ttf_parser::RgbaColor::new(0, 0, 0, 255), &mut painter);
                continue;
            }
            let mut b = Outline(PathBuilder::new());
            if face.outline_glyph(gid, &mut b).is_none() {
                continue;
            }
            if let (Some(path), Some(p)) = (b.0.finish(), paint(color, alpha)) {
                self.pix.fill_path(&path, &p, FillRule::Winding, base, mask);
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
        let fg = ttf_parser::RgbaColor::new(0, 0, 0, 255);
        if face.is_color_glyph(gid) {
            // Painted once (layer by layer, at full opacity), then copied.
            let font = (data.as_ptr() as usize, data.len());
            let drawn = glyph_cache::draw(&mut self.pix, (font, gid.0, [0; 4]), face.glyph_bounding_box(gid), s, (g.x as f32, g.y as f32), t, alpha, mask, |pix, base| {
                let mut painter = ColrPainter { face: &face, pix, base, transforms: Vec::new(), clips: Vec::new(), outline: None, alpha: 1.0, mask: None };
                face.paint_color_glyph(gid, 0, fg, &mut painter);
            });
            if drawn {
                return;
            }
        }
        let mut painter = ColrPainter { face: &face, pix: &mut self.pix, base, transforms: Vec::new(), clips: Vec::new(), outline: None, alpha, mask };
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
                Op::Fill { clip, .. } | Op::Stroke { clip, .. } | Op::Glyphs { clip, .. } | Op::FontGlyphs { clip, .. } | Op::Image { clip, .. } => clip,
            };
            let mut clipped: Option<Mask> = None;
            for (els, even_odd) in clip {
                clipped = self.intersect_rule(clipped.as_ref().or(mask), els, *even_odd, t);
            }
            let m = if clip.is_empty() { mask } else { clipped.as_ref() };
            match op {
                Op::Fill { path, color, even_odd, .. } => self.fill(path, *color, alpha, *even_odd, t, m),
                Op::Stroke { path, color, width, line, .. } => self.stroke_line(path, *color, alpha, *width, *line, t, m),
                Op::Glyphs { glyphs, ctm, .. } => self.glyphs(glyphs, alpha, &concat(ctm, t), m),
                Op::FontGlyphs { font, glyphs, color, .. } => self.font_glyphs(font, glyphs, *color, alpha, t, m),
                Op::Image { pix, m: place, alpha: a, .. } => {
                    let paint = tiny_skia::PixmapPaint { opacity: (alpha * a).clamp(0.0, 1.0), quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
                    self.pix.draw_pixmap(0, 0, (**pix).as_ref(), &paint, ts(&concat(place, t)), m);
                }
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
/// Rasterised glyphs, kept between frames: text that's drawn again (every
/// frame of an animation) is copied, not rasterised again. A glyph is kept
/// per face and variation, glyph, color, size in pixels and position within a
/// pixel (to a quarter), so the copy has the pixels rasterising would.
mod glyph_cache {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    /// The face (bytes and variation), the glyph, its color (RGBA, 0–255).
    pub(super) type Glyph = ((usize, usize), u16, [u8; 4]);

    #[derive(Hash, PartialEq, Eq)]
    struct Key {
        glyph: Glyph,
        /// Pixels per em × 64.
        size: u32,
        /// Where its origin falls within a pixel, in quarters.
        phase: (u8, u8),
    }

    struct Raster {
        pix: Option<Pixmap>,
        /// Its top-left, from the pixel its origin is in.
        left: i32,
        top: i32,
    }

    /// Enough for the text of several screens; past it, start again.
    const MAX: usize = 8192;
    /// Bigger glyphs are drawn directly (they're rare, and big to keep).
    const MAX_PX: f32 = 256.0;

    thread_local! {
        static CACHE: RefCell<HashMap<Key, Rc<Raster>>> = RefCell::new(HashMap::new());
    }

    /// Draw a glyph (`scale`: font units to user space; `origin`: its origin
    /// in user space; `t`: user space to device) from the cache, rasterising
    /// it with `paint` (glyph space → the raster's pixels) the first time.
    /// False when it can't be cached (a rotated or skewed transform, a big
    /// glyph): the caller draws it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(pix: &mut Pixmap, glyph: Glyph, bbox: Option<ttf_parser::Rect>, scale: f32, origin: (f32, f32), t: &[f32; 6], alpha: f32, mask: Option<&Mask>, paint: impl FnOnce(&mut Pixmap, Transform)) -> bool {
        let [sx, ky, kx, sy, tx, ty] = *t;
        if ky != 0.0 || kx != 0.0 || sx <= 0.0 || (sx - sy).abs() > 1e-4 {
            return false;
        }
        let px_per_unit = scale * sx;
        if px_per_unit * 1000.0 > MAX_PX * 4.0 {
            return false;
        }
        // The origin on the device, split into a whole pixel and a quarter-pixel phase.
        let (dx, dy) = (sx * origin.0 + tx, sy * origin.1 + ty);
        let quarter = |v: f32| ((v - v.floor()) * 4.0).round() as i32;
        let (mut ix, mut iy) = (dx.floor() as i32, dy.floor() as i32);
        let (mut qx, mut qy) = (quarter(dx), quarter(dy));
        if qx == 4 {
            (ix, qx) = (ix + 1, 0);
        }
        if qy == 4 {
            (iy, qy) = (iy + 1, 0);
        }
        let key = Key { glyph, size: (px_per_unit * 64_000.0).round() as u32, phase: (qx as u8, qy as u8) };
        let raster = CACHE.with(|c| c.borrow().get(&key).cloned());
        let raster = match raster {
            Some(r) => r,
            None => {
                let (fx, fy) = (qx as f32 / 4.0, qy as f32 / 4.0);
                // The glyph's box on the device (font units are y-up), with a pixel to spare.
                let (x0, y0, x1, y1) = match bbox {
                    Some(b) => (b.x_min as f32, b.y_min as f32, b.x_max as f32, b.y_max as f32),
                    // (No box: generously round the origin, for faces of 1000 to 2048 units per em.)
                    None => (-600.0, -800.0, 3000.0, 2600.0),
                };
                let left = (fx + x0 * px_per_unit).floor() as i32 - 1;
                let top = (fy - y1 * px_per_unit).floor() as i32 - 1;
                let right = (fx + x1 * px_per_unit).ceil() as i32 + 1;
                let bottom = (fy - y0 * px_per_unit).ceil() as i32 + 1;
                let mut out = Pixmap::new((right - left).max(1) as u32, (bottom - top).max(1) as u32);
                if let Some(p) = out.as_mut() {
                    paint(p, Transform::from_row(px_per_unit, 0.0, 0.0, -px_per_unit, fx - left as f32, fy - top as f32));
                }
                let r = Rc::new(Raster { pix: out, left, top });
                CACHE.with(|c| {
                    let mut c = c.borrow_mut();
                    if c.len() >= MAX {
                        c.clear();
                    }
                    c.insert(key, r.clone());
                });
                r
            }
        };
        if let Some(g) = &raster.pix {
            let paint = tiny_skia::PixmapPaint { opacity: alpha.clamp(0.0, 1.0), quality: tiny_skia::FilterQuality::Nearest, ..Default::default() };
            pix.draw_pixmap(ix + raster.left, iy + raster.top, g.as_ref(), &paint, Transform::identity(), mask);
        }
        true
    }
}

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
/// A layer's background: `bg` is the colour (an animated one), or its colour object's.
#[allow(clippy::too_many_arguments)]
fn draw_layer_box(p: &mut Painter, ly: &Layer, bg: Option<[f64; 4]>, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    if let Some(c) = bg {
        p.fill_box(w, h, layer_radii(ly), ly.continuous_corners, c.map(|v| v as f32), alpha, t, mask);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_border(p: &mut Painter, ly: &Layer, colors: &HashMap<u64, [f64; 4]>, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    if ly.border_width <= 0.0 {
        return;
    }
    let bw = ly.border_width as f32;
    let c = colors.get(&ly.border_color).map_or([0.0, 0.0, 0.0, 1.0], |c| c.map(|v| v as f32));
    let inner = corner_shape(w - bw, h - bw, (ly.corner_radius as f32 - bw / 2.0).max(0.0), ly.masked_corners, ly.continuous_corners);
    let inset = transform_els(&inner, &[1.0, 0.0, 0.0, 1.0, bw / 2.0, bw / 2.0]);
    p.stroke(&inset, c, alpha, bw, t, mask);
}

fn draw_sublayers(emu: &mut Emu, p: &mut Painter, layer: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let ly = calayer::layer(emu, layer);
    // A CAReplicatorLayer draws its sublayers `instanceCount` times, each copy
    // moved by `instanceTransform` from the one before.
    let copies = ly.instance_count.clamp(1, 1024);
    let step = calayer::affine_of(&ly.instance_transform).map(|v| v as f32);
    let mut each = *t;
    for _ in 0..copies {
        for &s in &ly.sublayers {
            if calayer::backing_view(emu, s).is_some() {
                continue;
            }
            draw_layer(emu, p, s, alpha, &each, mask, true);
        }
        each = concat(&step, &each);
    }
}

/// A layer's `compositingFilter` (React Native's `mixBlendMode`, and its
/// `brightness` filter): Core Animation's blend filter names, as a blend mode.
fn blend_of(emu: &mut Emu, l: u64) -> Option<tiny_skia::BlendMode> {
    use tiny_skia::BlendMode::*;
    let f = objc::bag_get(emu, l, "compositingFilter").filter(|&f| f != 0)?;
    let name = crate::foundation::nsstring(emu, f).ok()?;
    Some(match name.as_str() {
        "multiplyBlendMode" => Multiply,
        "screenBlendMode" => Screen,
        "overlayBlendMode" => Overlay,
        "darkenBlendMode" => Darken,
        "lightenBlendMode" => Lighten,
        "colorDodgeBlendMode" => ColorDodge,
        "colorBurnBlendMode" => ColorBurn,
        "hardLightBlendMode" => HardLight,
        "softLightBlendMode" => SoftLight,
        "differenceBlendMode" => Difference,
        "exclusionBlendMode" => Exclusion,
        "hueBlendMode" => Hue,
        "saturationBlendMode" => Saturation,
        "colorBlendMode" => Color,
        "luminosityBlendMode" => Luminosity,
        "linearDodgeBlendMode" | "plusL" => Plus,
        _ => return None,
    })
}

/// A rectangle (x, y, width, height) through `t`, as the device box around it (x0, y0, x1, y1).
fn device_box(t: &[f32; 6], r: [f32; 4]) -> [f32; 4] {
    let pts = [(r[0], r[1]), (r[0] + r[2], r[1]), (r[0], r[1] + r[3]), (r[0] + r[2], r[1] + r[3])].map(|(x, y)| apply(t, x, y));
    let xs = pts.map(|q| q.0);
    let ys = pts.map(|q| q.1);
    [xs.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max), ys.iter().copied().fold(f32::MIN, f32::max)]
}

/// Where a mask layer can let anything through, on the device: its frame,
/// and a shape layer's path (a shape layer draws outside its bounds).
fn mask_box(emu: &mut Emu, m: u64, inner: &[f32; 6]) -> [f32; 4] {
    let ml = calayer::layer(emu, m);
    let frame = [(ml.position[0] - ml.anchor[0] * ml.bounds[2]) as f32, (ml.position[1] - ml.anchor[1] * ml.bounds[3]) as f32, ml.bounds[2] as f32, ml.bounds[3] as f32];
    let mut b = device_box(inner, frame);
    if let Some(els) = shape_path(emu, m) {
        let mt = concat(&[1.0, 0.0, 0.0, 1.0, frame[0], frame[1]], inner);
        for el in transform_els(&els, &mt) {
            let pts: &[(f32, f32)] = &match el {
                El::Move(x, y) | El::Line(x, y) => vec![(x, y)],
                El::Cubic(a, b2, c, d, x, y) => vec![(a, b2), (c, d), (x, y)],
                El::Close => vec![],
            };
            for &(x, y) in pts {
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
        }
    }
    b
}

/// Draw something as one group (offscreen, then composited), as Core
/// Animation draws a layer with a mask or a compositing filter: `draw` paints
/// it (given the transform from device space to the group's buffer); then
/// the mask layer's alpha (drawn through `mask_layer`'s transform) cuts it,
/// and it's blended onto what's below at `alpha`. `region`: the device box
/// it can cover (x0, y0, x1, y1).
#[allow(clippy::too_many_arguments)]
fn draw_group(emu: &mut Emu, p: &mut Painter, region: [f32; 4], mask_layer: Option<(u64, [f32; 6])>, blend: Option<tiny_skia::BlendMode>, alpha: f32, clip: Option<&Mask>, draw: impl FnOnce(&mut Emu, &mut Painter, &[f32; 6])) {
    let (pw, ph) = (p.pix.width() as f32, p.pix.height() as f32);
    let (x0, y0) = (region[0].floor().max(0.0), region[1].floor().max(0.0));
    let (x1, y1) = (region[2].ceil().min(pw), region[3].ceil().min(ph));
    if x1 <= x0 || y1 <= y0 || alpha <= 0.0 {
        return;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let Some(pix) = Pixmap::new(w, h) else { return };
    let mut off = Painter { pix, font: p.font, emoji: p.emoji, faces: p.faces.clone() };
    let shift = [1.0, 0.0, 0.0, 1.0, -x0, -y0];
    draw(emu, &mut off, &shift);
    if let Some((m, mt)) = mask_layer {
        let Some(mpix) = Pixmap::new(w, h) else { return };
        let mut mp = Painter { pix: mpix, font: p.font, emoji: p.emoji, faces: p.faces.clone() };
        draw_layer(emu, &mut mp, m, 1.0, &concat(&mt, &shift), None, false);
        for (px, mpx) in off.pix.pixels_mut().iter_mut().zip(mp.pix.pixels()) {
            let a = mpx.alpha() as u32;
            if a == 255 {
                continue;
            }
            let k = |v: u8| ((v as u32 * a + 127) / 255) as u8;
            *px = tiny_skia::PremultipliedColorU8::from_rgba(k(px.red()), k(px.green()), k(px.blue()), k(px.alpha())).unwrap_or(*px);
        }
    }
    let paint = tiny_skia::PixmapPaint { opacity: alpha, blend_mode: blend.unwrap_or(tiny_skia::BlendMode::SourceOver), ..Default::default() };
    p.pix.draw_pixmap(x0 as i32, y0 as i32, off.pix.as_ref(), &paint, Transform::identity(), clip);
}

/// A CAShapeLayer's path (in its own coordinates), if it's a shape layer with one.
fn shape_path(emu: &mut Emu, l: u64) -> Option<Vec<El>> {
    let path = objc::bag_get(emu, l, "path").filter(|&p| p != 0)?;
    emu.render.paths.get(&path).cloned()
}

/// What a CAShapeLayer draws: its path filled (`fillColor`, opaque black
/// unless set; nil: none; `fillRule` even-odd or non-zero) and stroked
/// (`strokeColor`, nil unless set, `lineWidth` 1).
fn draw_shape(emu: &mut Emu, p: &mut Painter, l: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let Some(els) = shape_path(emu, l) else { return };
    let fill = match objc::bag_get(emu, l, "fillColor") {
        None => Some([0.0, 0.0, 0.0, 1.0]),
        Some(0) => None,
        Some(c) => Some(emu.ui.views.colors.get(&c).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])),
    };
    let even_odd = objc::bag_get(emu, l, "fillRule").filter(|&r| r != 0).and_then(|r| crate::foundation::nsstring(emu, r).ok()).is_some_and(|r| r == "even-odd");
    if let Some(c) = fill {
        p.fill(&els, c.map(|v| v as f32), alpha, even_odd, t, mask);
    }
    if let Some(c) = objc::bag_get(emu, l, "strokeColor").filter(|&c| c != 0).and_then(|c| emu.ui.views.colors.get(&c).copied()) {
        let width = objc::bag_get_f64(emu, l, "lineWidth").unwrap_or(1.0) as f32;
        p.stroke(&els, c.map(|v| v as f32), alpha, width, t, mask);
    }
}

/// What a CAGradientLayer draws in its bounds (in its corner shape): its
/// `colors` at `locations` (evenly spaced unless set), from `startPoint` to
/// `endPoint` (unit square), axial, or radial (`type` radial: centered on
/// the start point, out to the end point on each axis).
#[allow(clippy::too_many_arguments)]
fn draw_gradient(emu: &mut Emu, p: &mut Painter, l: u64, ly: &Layer, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let Some(colors) = objc::bag_get(emu, l, "colors").filter(|&c| c != 0) else { return };
    let count = |emu: &mut Emu, a: u64| objc::send(emu, a, "count", &[]).unwrap_or(0);
    let n = count(emu, colors);
    if n == 0 {
        return;
    }
    let locations = objc::bag_get(emu, l, "locations").filter(|&a| a != 0);
    let mut stops = Vec::new();
    for i in 0..n {
        let c = objc::send(emu, colors, "objectAtIndex:", &[i]).unwrap_or(0);
        let rgba = emu.ui.views.colors.get(&c).copied().unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let at = match locations.filter(|&a| count(emu, a) == n) {
            Some(a) => {
                let num = objc::send(emu, a, "objectAtIndex:", &[i]).unwrap_or(0);
                crate::foundation::number_value(emu, num).map(|(_, f)| f).unwrap_or(0.0)
            }
            None if n == 1 => 0.0,
            None => i as f64 / (n - 1) as f64,
        };
        let c = tiny_skia::Color::from_rgba(rgba[0] as f32, rgba[1] as f32, rgba[2] as f32, rgba[3] as f32).unwrap_or(tiny_skia::Color::TRANSPARENT);
        stops.push(tiny_skia::GradientStop::new(at as f32, c));
    }
    let [sp, ep] = ly.gradient_points.map(|q| tiny_skia::Point::from_xy(q[0] as f32 * w, q[1] as f32 * h));
    let radial = objc::bag_get(emu, l, "type").filter(|&k| k != 0).and_then(|k| crate::foundation::nsstring(emu, k).ok()).is_some_and(|k| k == "radial");
    let shader = if radial {
        let (rx, ry) = ((ep.x - sp.x).abs().max(0.01), (ep.y - sp.y).abs().max(0.01));
        // A circle of radius rx, squashed vertically to ry about the center.
        let squash = Transform::from_row(1.0, 0.0, 0.0, ry / rx, 0.0, sp.y - sp.y * ry / rx);
        tiny_skia::RadialGradient::new(sp, 0.0, sp, rx, stops, tiny_skia::SpreadMode::Pad, squash)
    } else {
        tiny_skia::LinearGradient::new(sp, ep, stops, tiny_skia::SpreadMode::Pad, Transform::identity())
    };
    let Some(shader) = shader else { return };
    let Some(path) = build(&layer_shape(ly, w, h)) else { return };
    let mut paint = Paint { shader, anti_alias: true, ..Default::default() };
    paint.shader.apply_opacity(alpha);
    p.pix.fill_path(&path, &paint, FillRule::Winding, ts(t), mask);
}

/// A standalone layer and its sublayers; `cast`: its shadow too. A layer
/// with a mask or a compositing filter is drawn as a group.
fn draw_layer(emu: &mut Emu, p: &mut Painter, s: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>, cast: bool) {
    let ly = calayer::layer(emu, s);
    let blend = blend_of(emu, s);
    // A plain color layer multiplied over what's below with a component over
    // 1 (React Native's `brightness` above 100%: an extended-range color)
    // brightens it, which 8-bit blending can't: scale the pixels beneath.
    if !ly.hidden && blend == Some(tiny_skia::BlendMode::Multiply) && ly.mask == 0 && ly.sublayers.is_empty() && ly.contents == 0 {
        let c = emu.ui.views.colors.get(&ly.background).copied();
        if let Some(c) = c.filter(|c| c[..3].iter().any(|&v| v > 1.0)) {
            let [_, _, lw, lh] = ly.bounds;
            let frame = [ly.position[0] - ly.anchor[0] * lw, ly.position[1] - ly.anchor[1] * lh, lw, lh];
            let lt = concat(&placement(frame, &ly), t);
            if let Some(cover) = p.intersect(mask, &layer_shape(&ly, lw as f32, lh as f32), &lt) {
                let k = (alpha * ly.opacity * c[3] as f32).clamp(0.0, 1.0);
                for (px, &m) in p.pix.pixels_mut().iter_mut().zip(cover.data()) {
                    if m == 0 {
                        continue;
                    }
                    let f = |v: u8, ch: f64| {
                        let scale = 1.0 + (ch as f32 - 1.0) * k * m as f32 / 255.0;
                        ((v as f32 * scale).round() as u32).min(px.alpha() as u32) as u8
                    };
                    *px = tiny_skia::PremultipliedColorU8::from_rgba(f(px.red(), c[0]), f(px.green(), c[1]), f(px.blue(), c[2]), px.alpha()).unwrap_or(*px);
                }
            }
            return;
        }
    }
    if !ly.hidden && (ly.mask != 0 || blend.is_some()) {
        let [_, _, lw, lh] = ly.bounds;
        let frame = [ly.position[0] - ly.anchor[0] * lw, ly.position[1] - ly.anchor[1] * lh, lw, lh];
        let lt = concat(&placement(frame, &ly), t);
        let inner = concat(&[1.0, 0.0, 0.0, 1.0, -ly.bounds[0] as f32, -ly.bounds[1] as f32], &lt);
        let region = if ly.mask != 0 {
            mask_box(emu, ly.mask, &inner)
        } else {
            let b = device_box(&lt, [0.0, 0.0, lw as f32, lh as f32]);
            [b[0] - 1.0, b[1] - 1.0, b[2] + 1.0, b[3] + 1.0]
        };
        let mask_layer = (ly.mask != 0).then_some((ly.mask, inner));
        draw_group(emu, p, region, mask_layer, blend, alpha, mask, |emu, off, shift| draw_layer_body(emu, off, s, 1.0, &concat(t, shift), None, cast));
        return;
    }
    draw_layer_body(emu, p, s, alpha, t, mask, cast)
}

/// A layer as it draws itself: shadow, background, shape or gradient,
/// contents, sublayers, border.
fn draw_layer_body(emu: &mut Emu, p: &mut Painter, s: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>, cast: bool) {
    {
        let mut ly = calayer::layer(emu, s);
        // What's on screen now: the model with running animations applied.
        let animated_bg = crate::animation::present_layer(emu, s, &mut ly);
        if ly.hidden || ly.opacity <= 0.0 {
            return;
        }
        let [_, _, lw, lh] = ly.bounds;
        let frame = [ly.position[0] - ly.anchor[0] * lw, ly.position[1] - ly.anchor[1] * lh, lw, lh];
        let lt = concat(&placement(frame, &ly), t);
        let a = alpha * ly.opacity;
        let (w, h) = (frame[2] as f32, frame[3] as f32);
        if cast && casts_shadow(&ly) {
            shadow(emu, p, &ly, w, h, &lt, alpha, mask, |emu, off, t2| draw_layer_body(emu, off, s, 1.0, t2, None, false), t);
        }
        let bg = animated_bg.or_else(|| emu.ui.views.colors.get(&ly.background).copied());
        draw_layer_box(p, &ly, bg, w, h, a, &lt, mask);
        draw_gradient(emu, p, s, &ly, w, h, a, &lt, mask);
        draw_shape(emu, p, s, a, &lt, mask);
        if let Some(img) = emu.render.images.get(&ly.contents).cloned() {
            let r = contents_rect(emu, &ly, &img, w, h);
            let clip = if ly.masks { p.intersect(mask, &layer_shape(&ly, w, h), &lt) } else { None };
            let m = if ly.masks { clip.as_ref() } else { mask };
            draw_contents(p, &img, ly.contents_center, r[2], r[3], a, &concat(&[1.0, 0.0, 0.0, 1.0, r[0], r[1]], &lt), m);
        }
        let inner = [1.0, 0.0, 0.0, 1.0, -ly.bounds[0] as f32, -ly.bounds[1] as f32];
        draw_sublayers(emu, p, s, a, &concat(&inner, &lt), mask);
        draw_border(p, &ly, &emu.ui.views.colors, w, h, a, &lt, mask);
    }
}

/// Whether a layer has a shadow to draw.
fn casts_shadow(ly: &Layer) -> bool {
    ly.shadow_opacity > 0.0
}

/// A layer's shadow, as Core Animation draws it (measured in the iOS 26
/// Simulator, scripts/measure/shadows): what the layer draws (or its
/// `shadowPath`), its alpha blurred by a Gaussian whose standard deviation is
/// `shadowRadius`, in `shadowColor` (black by default) at `shadowOpacity`
/// (and the layer's opacity), moved by `shadowOffset`, under the layer.
/// `source` draws the layer without its shadow, given the parent transform
/// moved into the offscreen buffer; `lt` is the layer's own transform, `t` its parent's.
#[allow(clippy::too_many_arguments)]
fn shadow(emu: &mut Emu, p: &mut Painter, ly: &Layer, w: f32, h: f32, lt: &[f32; 6], alpha: f32, mask: Option<&Mask>, source: impl FnOnce(&mut Emu, &mut Painter, &[f32; 6]), t: &[f32; 6]) {
    // Pixels per point here, and the blur and offset in pixels.
    let k = (lt[0] * lt[0] + lt[1] * lt[1]).sqrt();
    let sigma = ly.shadow_radius.max(0.0) as f32 * k;
    let (ox, oy) = (ly.shadow_offset[0] as f32, ly.shadow_offset[1] as f32);
    let (dx, dy) = (lt[0] * ox + lt[2] * oy, lt[1] * ox + lt[3] * oy);
    let color = emu.ui.views.colors.get(&ly.shadow_color).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]);
    // A shadow path's shadow only moves with the layer: blur it once.
    let path = (ly.shadow_path != 0).then(|| emu.render.paths.get(&ly.shadow_path).cloned()).flatten();
    if let Some(els) = path {
        let strength = ly.shadow_opacity * ly.opacity * alpha * color[3] as f32;
        let key: ShadowKey = (path_hash(&els), [w.to_bits(), h.to_bits()], [lt[0].to_bits(), lt[1].to_bits(), lt[2].to_bits(), lt[3].to_bits()], sigma.to_bits(), color.map(f64::to_bits), strength.to_bits());
        if !emu.render.shadows.contains_key(&key) {
            // Around the shape at the origin (the layer's translation comes in when it's drawn):
            // the layer's box and the path's (a path can reach past the bounds: a spread, an offset).
            let at0 = [lt[0], lt[1], lt[2], lt[3], 0.0, 0.0];
            let mut pb = [0.0f32, 0.0, w, h];
            for el in &els {
                let pts: &[(f32, f32)] = &match *el {
                    El::Move(x, y) | El::Line(x, y) => vec![(x, y)],
                    El::Cubic(a, b2, c, d, x, y) => vec![(a, b2), (c, d), (x, y)],
                    El::Close => vec![],
                };
                for &(x, y) in pts {
                    pb = [pb[0].min(x), pb[1].min(y), pb[2].max(x), pb[3].max(y)];
                }
            }
            let pts = [(pb[0], pb[1]), (pb[2], pb[1]), (pb[0], pb[3]), (pb[2], pb[3])].map(|(x, y)| apply(&at0, x, y));
            let margin = (3.0 * sigma).ceil() + 1.0;
            let x0 = (pts.iter().map(|q| q.0).fold(f32::MAX, f32::min) - margin).floor();
            let y0 = (pts.iter().map(|q| q.1).fold(f32::MAX, f32::min) - margin).floor();
            let x1 = (pts.iter().map(|q| q.0).fold(f32::MIN, f32::max) + margin).ceil();
            let y1 = (pts.iter().map(|q| q.1).fold(f32::MIN, f32::max) + margin).ceil();
            let (bw, bh) = ((x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32);
            let Some(pix) = Pixmap::new(bw, bh) else { return };
            let mut off = Painter { pix, font: p.font, emoji: p.emoji, faces: p.faces.clone() };
            off.fill(&els, [0.0, 0.0, 0.0, 1.0], 1.0, false, &concat(&at0, &[1.0, 0.0, 0.0, 1.0, -x0, -y0]), None);
            let Some(sh) = blurred_shadow(&off.pix, sigma, color, strength) else { return };
            if emu.render.shadows.len() > 32 {
                emu.render.shadows.clear();
            }
            emu.render.shadows.insert(key, (std::rc::Rc::new(sh), x0, y0));
        }
        let (sh, x0, y0) = emu.render.shadows[&key].clone();
        let paint = tiny_skia::PixmapPaint::default();
        p.pix.draw_pixmap(0, 0, sh.as_ref().as_ref(), &paint, Transform::from_translate(lt[4] + x0 + dx, lt[5] + y0 + dy), mask);
        return;
    }
    // The layer's box on screen, with room for the blur.
    let pts = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].map(|(x, y)| apply(lt, x, y));
    let margin = (3.0 * sigma).ceil() + 1.0;
    let (pw, ph) = (p.pix.width() as f32, p.pix.height() as f32);
    let x0 = (pts.iter().map(|q| q.0).fold(f32::MAX, f32::min) - margin).floor().max(-dx.abs() - margin).max(-pw);
    let y0 = (pts.iter().map(|q| q.1).fold(f32::MAX, f32::min) - margin).floor().max(-dy.abs() - margin).max(-ph);
    let x1 = (pts.iter().map(|q| q.0).fold(f32::MIN, f32::max) + margin).ceil().min(pw + dx.abs() + margin).min(2.0 * pw);
    let y1 = (pts.iter().map(|q| q.1).fold(f32::MIN, f32::max) + margin).ceil().min(ph + dy.abs() + margin).min(2.0 * ph);
    let (bw, bh) = ((x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32);
    let Some(pix) = Pixmap::new(bw, bh) else { return };
    let mut off = Painter { pix, font: p.font, emoji: p.emoji, faces: p.faces.clone() };
    let shift = [1.0, 0.0, 0.0, 1.0, -x0, -y0];
    source(emu, &mut off, &concat(t, &shift));
    // The source already carries the layer's own opacity.
    let strength = ly.shadow_opacity * alpha * color[3] as f32;
    let Some(sh) = blurred_shadow(&off.pix, sigma, color, strength) else { return };
    let paint = tiny_skia::PixmapPaint::default();
    p.pix.draw_pixmap(0, 0, sh.as_ref(), &paint, Transform::from_translate(x0 + dx, y0 + dy), mask);
}

/// Liquid Glass (glass.rs) over what's drawn so far, in `shape`; as measured
/// on the iOS 26 Simulator (tests/fixtures/glass, light appearance). The
/// backdrop blurred (σ 2.5 pt; clear glass 1.75); regular glass mixes it 70%
/// toward white (0.977) and makes it 1.8× as saturated (black reads 175, white
/// 251, orange peach); clear glass mixes 7.5% and lifts it a little; a tint is
/// the tint's colour, darker over darker backdrops (×0.855 over black to ×0.99
/// over white). Regular and tinted glass cast a soft shadow: the shape 6 pt
/// lower, blurred by 16 pt, at 11%. In the dark appearance regular glass
/// mixes 78% toward 0.175 and is 2.6× as saturated (black reads 35, white
/// 90), a tint is itself (a little lighter over light), the shadow 7.3%.
#[allow(clippy::too_many_arguments)]
fn draw_glass(p: &mut Painter, g: &crate::glass::Glass, shape: &[El], w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let k = (t[0] * t[0] + t[1] * t[1]).sqrt();
    let clear = g.style == crate::glass::CLEAR;
    let sigma = if clear { 1.75 } else { 2.5 } * k;
    let pts = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].map(|(x, y)| apply(t, x, y));
    let margin = (3.0 * sigma).ceil() + 1.0;
    let x0 = (pts.iter().map(|q| q.0).fold(f32::MAX, f32::min) - margin).floor() as i32;
    let y0 = (pts.iter().map(|q| q.1).fold(f32::MAX, f32::min) - margin).floor() as i32;
    let x1 = (pts.iter().map(|q| q.0).fold(f32::MIN, f32::max) + margin).ceil() as i32;
    let y1 = (pts.iter().map(|q| q.1).fold(f32::MIN, f32::max) + margin).ceil() as i32;
    let (pw, ph) = (p.pix.width() as i32, p.pix.height() as i32);
    let (bw, bh) = (x1 - x0, y1 - y0);
    if bw <= 0 || bh <= 0 || x1 <= 0 || y1 <= 0 || x0 >= pw || y0 >= ph {
        return;
    }
    // The backdrop (beyond the screen's edges, its edge pixels), as colour.
    let n = (bw * bh) as usize;
    let mut ch = [vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]];
    for y in 0..bh {
        for x in 0..bw {
            let (sx, sy) = ((x0 + x).clamp(0, pw - 1) as u32, (y0 + y).clamp(0, ph - 1) as u32);
            let c = p.pix.pixel(sx, sy).map(|c| c.demultiply());
            let i = (y * bw + x) as usize;
            // (Nothing drawn there: white, as an empty window shows.)
            let (r, gg, b) = match c {
                Some(c) if c.alpha() > 0 => (c.red() as f32 / 255.0, c.green() as f32 / 255.0, c.blue() as f32 / 255.0),
                _ => (1.0, 1.0, 1.0),
            };
            ch[0][i] = r;
            ch[1][i] = gg;
            ch[2][i] = b;
        }
    }
    for c in &mut ch {
        gaussian_blur(c, bw as usize, bh as usize, sigma);
    }
    if !clear {
        let off_y = 6.0 * k;
        let ssigma = 16.0 * k;
        let sm = (3.0 * ssigma).ceil() + 1.0;
        let (sx0, sy0) = ((x0 as f32 - sm).floor(), (y0 as f32 - sm).floor());
        let (sw, sh) = ((bw as f32 + 2.0 * sm) as u32, (bh as f32 + 2.0 * sm + off_y) as u32);
        if let Some(pix) = Pixmap::new(sw, sh) {
            let mut off = Painter { pix, font: p.font, emoji: p.emoji, faces: p.faces.clone() };
            off.fill(shape, [0.0, 0.0, 0.0, 1.0], 1.0, false, &concat(t, &[1.0, 0.0, 0.0, 1.0, -sx0, -sy0 + off_y]), None);
            let strength = if g.dark { 0.073 } else { 0.11 };
            if let Some(sh) = blurred_shadow(&off.pix, ssigma, [0.0, 0.0, 0.0, 1.0], strength * alpha) {
                p.pix.draw_pixmap(sx0 as i32, sy0 as i32, sh.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), mask);
            }
        }
    }
    let Some(mut out) = Pixmap::new(bw as u32, bh as u32) else { return };
    let tint = g.tint.map(|c| c.map(|v| v as f32));
    for (i, px) in out.pixels_mut().iter_mut().enumerate() {
        let c = [ch[0][i], ch[1][i], ch[2][i]];
        let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let mut o = if clear {
            c.map(|v| 1.0 - (1.0 - (0.925 * v + 0.075)).max(0.0).powf(1.15))
        } else {
            // (Dark: 78% toward 0.175, 2.6× as saturated.)
            let (mix, toward, sat) = if g.dark { (0.784, 0.175, 2.6) } else { (0.7, 0.977, 1.8) };
            let m = c.map(|v| (1.0 - mix) * v + mix * toward);
            let l = 0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2];
            m.map(|v| l + sat * (v - l))
        };
        if let Some(tc) = tint {
            for j in 0..3 {
                // (Dark: the tint, a little lighter over lighter backdrops.)
                let t = if g.dark { tc[j] + 0.043 * luma * (1.0 - tc[j]) } else { tc[j] * (0.855 + 0.135 * luma) };
                o[j] = o[j] * (1.0 - tc[3]) + t * tc[3];
            }
        }
        let u = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        *px = tiny_skia::PremultipliedColorU8::from_rgba(u(o[0]), u(o[1]), u(o[2]), 255).unwrap_or(*px);
    }
    let Some(clip) = p.intersect(mask, shape, t) else { return };
    let paint = tiny_skia::PixmapPaint { opacity: alpha, ..Default::default() };
    p.pix.draw_pixmap(x0, y0, out.as_ref(), &paint, Transform::identity(), Some(&clip));
}

/// A shadow from a source's alpha: blurred by `sigma` pixels, in `color` at `strength`.
fn blurred_shadow(source: &Pixmap, sigma: f32, color: [f64; 4], strength: f32) -> Option<Pixmap> {
    let (bw, bh) = (source.width(), source.height());
    let mut a: Vec<f32> = source.pixels().iter().map(|c| c.alpha() as f32 / 255.0).collect();
    gaussian_blur(&mut a, bw as usize, bh as usize, sigma);
    let mut sh = Pixmap::new(bw, bh)?;
    for (px, &v) in sh.pixels_mut().iter_mut().zip(&a) {
        let al = (v * strength).clamp(0.0, 1.0);
        if al > 0.0 {
            let c = |x: f64| (x as f32 * al * 255.0).round() as u8;
            *px = tiny_skia::PremultipliedColorU8::from_rgba(c(color[0]), c(color[1]), c(color[2]), (al * 255.0).round() as u8).unwrap_or(*px);
        }
    }
    Some(sh)
}

/// Blur `a` (w × h, row-major) by a Gaussian of standard deviation `sigma`
/// (pixels): three box blurs each way, the usual close approximation.
fn gaussian_blur(a: &mut [f32], w: usize, h: usize, sigma: f32) {
    if sigma < 0.3 {
        return;
    }
    // Box sizes whose three passes give this sigma (W. Jarosz, "Fast image convolutions").
    let n = 3.0f32;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round() as i32;
    let sizes: Vec<i32> = (0..3).map(|i| if i < m { wl } else { wu }).collect();
    let mut tmp = vec![0.0f32; a.len()];
    for &size in &sizes {
        let r = ((size - 1) / 2) as usize;
        box_pass(a, &mut tmp, w, h, r, true);
        box_pass(&tmp, a, w, h, r, false);
    }
}

/// One box blur of radius `r` along rows (`horizontal`) or columns, `src` → `dst`; outside is 0.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = 0.0f32;
        for i in 0..=r.min(len - 1) {
            sum += src[at(line, i)];
        }
        for i in 0..len {
            dst[at(line, i)] = sum * norm;
            if i + r + 1 < len {
                sum += src[at(line, i + r + 1)];
            }
            if i >= r {
                sum -= src[at(line, i - r)];
            }
        }
    }
}

fn draw_view(emu: &mut Emu, p: &mut Painter, v: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    draw_view_cast(emu, p, v, alpha, t, mask, true)
}

/// A view and its subviews; `cast`: its shadow too. A view whose layer has a
/// mask or a compositing filter is drawn as a group.
fn draw_view_cast(emu: &mut Emu, p: &mut Painter, v: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>, cast: bool) {
    let vw = view(emu, v);
    if vw.layer != 0 && !vw.hidden {
        let ly = calayer::layer(emu, vw.layer);
        let blend = blend_of(emu, vw.layer);
        if ly.mask != 0 || blend.is_some() {
            let frame = [vw.center[0] - vw.bounds[2] / 2.0, vw.center[1] - vw.bounds[3] / 2.0, vw.bounds[2], vw.bounds[3]];
            let vt = concat(&placement(frame, &ly), t);
            let inner = concat(&[1.0, 0.0, 0.0, 1.0, -vw.bounds[0] as f32, -vw.bounds[1] as f32], &vt);
            let region = if ly.mask != 0 {
                mask_box(emu, ly.mask, &inner)
            } else {
                // (Room for its shadow.)
                let b = device_box(&vt, [0.0, 0.0, frame[2] as f32, frame[3] as f32]);
                let k = (vt[0] * vt[0] + vt[1] * vt[1]).sqrt();
                let m = if casts_shadow(&ly) { (ly.shadow_radius.abs() * 3.0 + ly.shadow_offset[0].abs().max(ly.shadow_offset[1].abs())) as f32 * k } else { 0.0 } + 1.0;
                [b[0] - m, b[1] - m, b[2] + m, b[3] + m]
            };
            let mask_layer = (ly.mask != 0).then_some((ly.mask, inner));
            draw_group(emu, p, region, mask_layer, blend, alpha, mask, |emu, off, shift| draw_view_body(emu, off, v, 1.0, &concat(t, shift), None, cast));
            return;
        }
    }
    draw_view_body(emu, p, v, alpha, t, mask, cast)
}

/// A view as it draws itself, and its subviews.
fn draw_view_body(emu: &mut Emu, p: &mut Painter, v: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>, cast: bool) {
    let mut vw = view(emu, v);
    let mut ly = if vw.layer != 0 { calayer::layer(emu, vw.layer) } else { Layer::default() };
    // What's on screen now: the model with running animations applied.
    let animated_bg = crate::animation::present_view(emu, v, &mut vw, &mut ly);
    // A screen moving in a push or pop is a card with the display's corners.
    if let Some(r) = crate::navigation::card_corner(emu, v) {
        (ly.corner_radius, ly.continuous_corners, ly.masked_corners, vw.clips) = (r, true, 15, true);
    }
    let a = alpha * vw.alpha as f32;
    if vw.hidden || a <= 0.0 {
        return;
    }
    let frame = [vw.center[0] - vw.bounds[2] / 2.0, vw.center[1] - vw.bounds[3] / 2.0, vw.bounds[2], vw.bounds[3]];
    let vt = concat(&placement(frame, &ly), t);
    let (w, h) = (frame[2] as f32, frame[3] as f32);
    // iOS 26's corner configuration (glass.rs): each corner's radius, continuous
    // (a capsule's ends round).
    if let Some(r) = crate::glass::corner_radii(emu, v, frame[2], frame[3]) {
        ly.corner_radii = Some(r);
        ly.continuous_corners = !crate::glass::is_capsule(emu, v);
    }
    let bg = animated_bg.or_else(|| emu.ui.views.colors.get(&vw.background).copied());
    if cast && casts_shadow(&ly) {
        shadow(emu, p, &ly, w, h, &vt, alpha, mask, |emu, off, t2| draw_view_body(emu, off, v, 1.0, t2, None, false), t);
    }
    draw_layer_box(p, &ly, bg, w, h, a, &vt, mask);
    if let Some(g) = crate::glass::glass(emu, v) {
        draw_glass(p, &g, &layer_shape(&ly, w, h), w, h, a, &vt, mask);
    }
    // A web page its engine shows itself, under the screen: a hole, through what's below.
    if crate::webkit::is_hole(emu, v) {
        p.clear(w, h, &vt, mask);
    }
    let inner = concat(&[1.0, 0.0, 0.0, 1.0, -vw.bounds[0] as f32, -vw.bounds[1] as f32], &vt);
    // A text view is a scroll view, which clips to its bounds.
    let text_view = crate::textinput::is_text_view(emu, v);
    let clips = vw.clips || ly.masks || text_view;
    let clipped = if clips { p.intersect(mask, &layer_shape(&ly, w, h), &vt) } else { None };
    let m = if clips { clipped.as_ref() } else { mask };
    // The layer's own contents are clipped with it too (masksToBounds); a
    // template image in its own color, else the view's tint.
    if let Some(mut img) = emu.render.images.get(&ly.contents).cloned() {
        if img.look.is_template() {
            let c = if img.look.tint != 0 { img.look.tint } else { crate::uiview::tint_color(emu, v).unwrap_or(0) };
            let color = rgba(emu, c);
            img = tinted(&img, color);
        }
        let r = contents_rect(emu, &ly, &img, w, h);
        let at = concat(&[1.0, 0.0, 0.0, 1.0, r[0], r[1]], &vt);
        let at = if img.look.mirrors_in_rtl() && crate::direction::view_rtl(emu, v).unwrap_or(false) { concat(&[-1.0, 0.0, 0.0, 1.0, r[2], 0.0], &at) } else { at };
        draw_contents(p, &img, ly.contents_center, r[2], r[3], a, &at, m);
    }
    if let Some((ops, _)) = emu.render.content.get(&v).cloned() {
        p.ops(&ops, a, &inner, m);
    }
    if vw.layer != 0 {
        draw_sublayers(emu, p, vw.layer, a, &inner, m);
    }
    if crate::textinput::is_text_field(emu, v) {
        let _ = draw_text_field(emu, p, v, vw.bounds, a, &inner, m);
    } else if text_view {
        let _ = draw_text_view(emu, p, v, a, &inner, m);
    } else if crate::label::is_label(emu, v) {
        let _ = draw_label(emu, p, v, vw.bounds, a, &inner, m);
    }
    if let Ok(parts) = crate::navigation::bar_parts(emu, v) {
        let above = crate::uiview::window_origin(emu, v)[1] as f32;
        for (look, dx, back, covered_from) in parts {
            // Underneath: only what the upper card leaves showing, left of its edge.
            let clip = covered_from.map(|x| p.intersect(m, &rect_els(-w, -above, w + x.max(-w as f64) as f32, h + above), &inner));
            let mm = match &clip {
                Some(c) => c.as_ref(),
                None => m,
            };
            draw_bar(emu, p, &look, w, h, above, a, &inner, mm, dx as f32, back.map(|b| b as f32));
        }
    }
    if let Ok(Some(look)) = crate::tabbar::look(emu, v) {
        draw_tab_bar(emu, p, &look, w, h, a, &inner, m);
    }
    if let Ok(Some(list)) = crate::tabbar::more_list(emu, v) {
        draw_more_list(emu, p, &list, w, a, &inner, m);
    }
    for s in vw.subviews {
        draw_view(emu, p, s, a, &inner, m);
    }
    draw_border(p, &ly, &emu.ui.views.colors, w, h, a, &vt, mask);
}

/// A UITextField's text (or placeholder): one line, vertically centered in its
/// text rect (`textRectForBounds:`, which apps override for padding).
fn draw_text_field(emu: &mut Emu, p: &mut Painter, v: u64, bounds: [f64; 4], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) -> Result<()> {
    let Some((text, font, color)) = crate::textinput::appearance(emu, v)? else { return Ok(()) };
    let sel = if crate::textinput::is_editing(emu, v) { "editingRectForBounds:" } else { "textRectForBounds:" };
    let r = objc::send_fp(emu, v, sel, &[], &bounds)?;
    let baseline = r[1] + r[3] / 2.0 + font.size * 0.35;
    // (Where the line starts: its alignment, natural by the text's direction.)
    let (shaped, _, x0) = crate::textinput::field_line(emu, v, &text, &font, r[2])?;
    let glyphs: Vec<Glyph> = shaped
        .into_iter()
        .filter(|(g, _)| !g.ch.is_whitespace())
        .map(|(g, pen)| Glyph { ch: g.ch, gid: g.gid, x: r[0] + x0 + pen + g.offset[0], y: baseline + g.offset[1], size: font.size, weight: font.weight, face: g.face, color, skew: font.skew(), shadow: None })
        .collect();
    p.sync_faces(emu, &glyphs);
    p.glyphs(&glyphs, alpha, t, mask);
    Ok(())
}

/// A UITextView's text: wrapped lines in its text container, in its content
/// (`t` already scrolls by the bounds' origin).
fn draw_text_view(emu: &mut Emu, p: &mut Painter, v: u64, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) -> Result<()> {
    let laid = crate::textview::lay_out(emu, v)?;
    for l in &laid.lines {
        let dx = match laid.alignment {
            1 => (laid.width - l.width) / 2.0,
            2 => laid.width - l.width,
            _ => 0.0,
        };
        let baseline = laid.origin.1 + l.y + l.ascent;
        let rtl = crate::direction::view_rtl(emu, v)?;
        let glyphs = crate::textkit::laid_line_glyphs(emu, &laid.text, l, &laid.font, laid.color, laid.origin.0 + dx, baseline, rtl);
        p.sync_faces(emu, &glyphs);
        p.glyphs(&glyphs, alpha, t, mask);
    }
    Ok(())
}

/// A UILabel's text: wrapped at its width, centered vertically in its bounds.
fn draw_label(emu: &mut Emu, p: &mut Painter, v: u64, bounds: [f64; 4], alpha: f32, t: &[f32; 6], mask: Option<&Mask>) -> Result<()> {
    let laid = crate::label::lay_out(emu, v, bounds[2])?;
    let top = bounds[1] + (bounds[3] - laid.height()) / 2.0;
    for l in &laid.lines {
        let dx = match laid.alignment {
            1 => (bounds[2] - l.width) / 2.0,
            2 => bounds[2] - l.width,
            _ => 0.0,
        };
        let rtl = crate::direction::view_rtl(emu, v)?;
        let glyphs = crate::textkit::laid_line_glyphs(emu, &laid.text, l, &laid.font, laid.color, bounds[0] + dx, top + l.y + l.ascent, rtl);
        p.sync_faces(emu, &glyphs);
        p.glyphs(&glyphs, alpha, t, mask);
    }
    Ok(())
}

/// The software keyboard while it's up: iOS 26's light keyboard panel (its
/// colour and rounded top corners, sampled from the Simulator) along the bottom
/// edge. The keys aren't drawn.
fn draw_keyboard(p: &mut Painter, screen: (f64, f64), height: f64, visible: f64, t: &[f32; 6]) {
    const RADIUS: f32 = 28.0;
    let [x, _, w, h] = crate::textinput::keyboard_frame(screen, height, true).map(|v| v as f32);
    // Sliding: its top is `visible` points above the bottom edge.
    let y = (screen.1 - visible) as f32;
    // Round the top corners only: the bottom ones fall below the screen.
    let panel = transform_els(&rounded_rect(w, h + RADIUS, RADIUS), &[1.0, 0.0, 0.0, 1.0, x, y]);
    p.fill(&panel, [226.0 / 255.0, 228.0 / 255.0, 232.0 / 255.0, 1.0], 1.0, false, t, None);
}

/// A navigation bar. iOS 26: no background, a centered title and a round
/// glass back button. Before: the configured background (reaching up under
/// the status bar, `above` points), a hairline, and a tinted chevron + label.
#[allow(clippy::too_many_arguments)]
/// A navigation bar as `look` describes it: its title moved `title_dx` (with
/// its screen, during a push or pop), and its back button at `back_alpha`
/// (None: just the title, for the screen leaving).
#[allow(clippy::too_many_arguments)]
/// The More tab's list (tabbar.rs says what's in it), as UIKit's plain
/// table shows it: a separator above each row and after the last, from the
/// title's start (to 16 pt short of the edge under Liquid Glass); each row's
/// image in the tint centred 25.7 pt in, its title in 17 pt at 52, its badge
/// as secondary text before a chevron (measured on the iOS 26 Simulator).
#[allow(clippy::too_many_arguments)]
fn draw_more_list(emu: &mut Emu, p: &mut Painter, list: &crate::tabbar::MoreList, w: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let separator = if list.glass { [0.91, 0.91, 0.91, 1.0] } else { [0.776, 0.776, 0.784, 1.0] };
    let right = if list.glass { w - 16.0 } else { w };
    let line = |p: &mut Painter, y: f32| {
        p.fill(&rect_els(52.0, y, right - 52.0, 1.0 / 3.0), separator, alpha, false, t, mask);
    };
    let label = crate::appearance::label(emu);
    let secondary = crate::appearance::system(emu, "secondaryLabelColor");
    let tertiary = crate::appearance::system(emu, "tertiaryLabelColor");
    for (i, row) in list.rows.iter().enumerate() {
        let [_, y, _, h] = row.frame.map(|v| v as f32);
        if i == 0 {
            line(p, y);
        }
        line(p, y + h - 1.0 / 3.0);
        let mid = (y + h / 2.0) as f64;
        if let Some(mut img) = emu.render.images.get(&row.image).cloned() {
            if img.look.is_template() {
                img = tinted(&img, list.tint.map(|v| v as f32));
            }
            let [iw, ih] = img.size.map(|v| v as f32);
            draw_contents(p, &img, [0.0, 0.0, 1.0, 1.0], iw, ih, alpha, &concat(&[1.0, 0.0, 0.0, 1.0, 25.7 - iw / 2.0, mid as f32 - ih / 2.0], t), mask);
        }
        let baseline = mid + 17.0 * 0.35;
        let (glyphs, _) = crate::textkit::line_glyphs(emu, &row.title, 17.0, 0.0, label, 52.0, baseline);
        p.sync_faces(emu, &glyphs);
        p.glyphs(&glyphs, alpha, t, mask);
        // The chevron: tertiary, 369.3 pt from the left on a 390-pt screen.
        let cx = right - 4.7 - if list.glass { 0.0 } else { 16.0 };
        let chevron = [El::Move(cx - 2.6, mid as f32 - 5.3), El::Line(cx + 2.6, mid as f32), El::Line(cx - 2.6, mid as f32 + 5.3)];
        let s = Stroke { width: 1.8, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
        if let (Some(path), Some(paint)) = (build(&chevron), paint(tertiary.map(|v| v as f32), alpha)) {
            p.pix.stroke_path(&path, &paint, &s, ts(t), mask);
        }
        if let Some(badge) = &row.badge {
            let (_, bw) = crate::textkit::line_glyphs(emu, badge, 17.0, 0.0, secondary, 0.0, 0.0);
            let (glyphs, _) = crate::textkit::line_glyphs(emu, badge, 17.0, 0.0, secondary, cx as f64 - 14.0 - bw, baseline);
            p.sync_faces(emu, &glyphs);
            p.glyphs(&glyphs, alpha, t, mask);
        }
    }
}

/// A tab bar (tabbar.rs says where everything goes): under Liquid Glass, a
/// floating capsule with a soft shadow and a lens under the selected tab;
/// before, the bar's background (if its appearance has one) and hairline.
/// Then each tab's image (a template, in the tab's color), title and badge.
#[allow(clippy::too_many_arguments)]
fn draw_tab_bar(emu: &mut Emu, p: &mut Painter, look: &crate::tabbar::Look, w: f32, h: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    let dark = look.dark;
    let capsule = |r: [f64; 4], grow: f32, dy: f32| {
        let [x, y, rw, rh] = r.map(|v| v as f32);
        let (rw, rh) = (rw + 2.0 * grow, rh + 2.0 * grow);
        transform_els(&rounded_rect(rw, rh, rh / 2.0), &[1.0, 0.0, 0.0, 1.0, x - grow, y - grow + dy])
    };
    if let Some(bg) = look.background {
        p.fill_box(w, h, [0.0; 4], false, bg.map(|v| v as f32), alpha, t, mask);
    }
    if let Some(line) = look.hairline {
        // One device pixel (1/3 pt) along the top edge, above the bar.
        p.fill(&rect_els(0.0, -1.0 / 3.0, w, 1.0 / 3.0), line.map(|v| v as f32), alpha, false, t, mask);
    }
    // The capsule, and the search tab's circle if there's one.
    for platter in look.platter.iter().chain(&look.search_platter).copied() {
        // The shadow, soft and mostly below: rings fading out (measured: about
        // 4% darker just below the capsule, nothing 10 pt out; none to see in the dark).
        if !dark {
            for grow in [2.0f32, 4.0, 6.0, 8.0, 10.0] {
                p.fill(&capsule(platter, grow, 3.0), [0.0, 0.0, 0.0, 0.009], alpha, false, t, mask);
            }
        }
        // (Measured: the capsule 253 white, 19 in the dark; the lens 235, 53 in the dark.)
        let fill = if dark { [0.075, 0.075, 0.075, 1.0] } else { [0.992, 0.992, 0.992, 1.0] };
        p.fill(&capsule(platter, 0.0, 0.0), fill, alpha, false, t, mask);
    }
    if look.platter.is_some() {
        // (The search tab, selected, shows its tint, no lens.)
        if let Some(sel) = look.buttons.iter().find(|b| b.selected && !b.detached) {
            let lens = if dark { [0.208, 0.208, 0.208, 1.0] } else { [0.922, 0.922, 0.922, 1.0] };
            p.fill(&capsule(sel.frame, 0.0, 0.0), lens, alpha, false, t, mask);
        }
    }
    let _ = h;
    for b in &look.buttons {
        if let Some(mut img) = emu.render.images.get(&b.image).cloned() {
            if img.look.is_template() {
                img = tinted(&img, b.color.map(|v| v as f32));
            }
            let [x, y, iw, ih] = b.image_frame.map(|v| v as f32);
            draw_contents(p, &img, [0.0, 0.0, 1.0, 1.0], iw, ih, alpha, &concat(&[1.0, 0.0, 0.0, 1.0, x, y], t), mask);
        }
        if !b.title.is_empty() && !b.detached {
            let [x, y, _, th] = b.title_frame;
            // (The baseline: four fifths of the way down the title's line.)
            let (glyphs, _) = crate::textkit::line_glyphs(emu, &b.title, b.title_size, b.title_weight, b.title_color, x, y + th * 0.8);
            p.sync_faces(emu, &glyphs);
            p.glyphs(&glyphs, alpha, t, mask);
        }
        if let Some(badge) = &b.badge {
            p.fill(&capsule(badge.frame, 0.0, 0.0), badge.color.map(|v| v as f32), alpha, false, t, mask);
            let (_, tw) = crate::textkit::line_glyphs(emu, &badge.text, badge.size, badge.weight, [1.0; 4], 0.0, 0.0);
            let [bx, by, bw, bh] = badge.frame;
            let (glyphs, _) = crate::textkit::line_glyphs(emu, &badge.text, badge.size, badge.weight, [1.0, 1.0, 1.0, 1.0], bx + (bw - tw) / 2.0, by + bh / 2.0 + badge.size * 0.35);
            p.sync_faces(emu, &glyphs);
            p.glyphs(&glyphs, alpha, t, mask);
        }
    }
}

fn draw_bar(emu: &mut Emu, p: &mut Painter, look: &crate::navigation::BarLook, w: f32, h: f32, above: f32, alpha: f32, t: &[f32; 6], mask: Option<&Mask>, title_dx: f32, back_alpha: Option<f32>) {
    use crate::navigation::{BACK_LABEL_X, TITLE_SIZE, TITLE_WEIGHT};
    const TINT: [f64; 4] = [0.0, 0.478, 1.0, 1.0]; // systemBlue
    // The bar's usual row (the back button, a small title): its top, above
    // the large title's space.
    let row = (look.row as f32).min(h);
    let baseline = row as f64 / 2.0 + TITLE_SIZE * 0.35;
    if back_alpha.is_none() {
        let (glyphs, width) = crate::textkit::line_glyphs(emu, &look.title, TITLE_SIZE, TITLE_WEIGHT, crate::appearance::label(emu), 0.0, baseline);
        let x = (w as f64 - width) / 2.0 + title_dx as f64;
        let placed: Vec<Glyph> = glyphs.into_iter().map(|g| Glyph { x: g.x + x, ..g }).collect();
        p.sync_faces(emu, &placed);
        p.glyphs(&placed, alpha, t, mask);
        return;
    }
    let back_alpha = back_alpha.unwrap_or(1.0);
    if let Some(bg) = look.background {
        p.fill(&rect_els(0.0, -above, w, h + above), bg.map(|v| v as f32), alpha, false, t, mask);
    }
    if let Some(sh) = look.shadow {
        // One device pixel (1/3 pt) along the bottom edge.
        p.fill(&rect_els(0.0, h, w, 1.0 / 3.0), sh.map(|v| v as f32), alpha, false, t, mask);
    }
    if look.large {
        // The title large, at the leading edge, under the bar's usual row
        // (which shows none), moving with the bar's bottom as it collapses.
        use crate::navigation::{LARGE_TITLE_SIZE, LARGE_TITLE_WEIGHT, LARGE_TITLE_X};
        let large_baseline = h as f64 - crate::navigation::large_title_metrics(emu).1;
        let (glyphs, _) = crate::textkit::line_glyphs(emu, &look.title, LARGE_TITLE_SIZE, LARGE_TITLE_WEIGHT, crate::appearance::label(emu), 0.0, large_baseline);
        let x = look.leading + LARGE_TITLE_X + title_dx as f64;
        let placed: Vec<Glyph> = glyphs.into_iter().map(|g| Glyph { x: g.x + x, ..g }).collect();
        p.sync_faces(emu, &placed);
        p.glyphs(&placed, alpha, t, mask);
    } else {
        let (glyphs, width) = crate::textkit::line_glyphs(emu, &look.title, TITLE_SIZE, TITLE_WEIGHT, crate::appearance::label(emu), 0.0, baseline);
        let x = (w as f64 - width) / 2.0 + title_dx as f64;
        let placed: Vec<Glyph> = glyphs.into_iter().map(|g| Glyph { x: g.x + x, ..g }).collect();
        p.sync_faces(emu, &placed);
        p.glyphs(&placed, alpha, t, mask);
    }
    draw_bar_items(emu, p, look, alpha, t, mask);
    let Some(back) = &look.back else { return };
    let alpha = alpha * back_alpha;
    // The back button sits inside the bar's leading safe-area inset; in
    // right to left, mirrored across the bar (its text drawn unmirrored).
    let bar_t = *t;
    let t = &if look.rtl { concat(&[-1.0, 0.0, 0.0, 1.0, w as f32 - look.leading as f32, 0.0], t) } else { concat(&[1.0, 0.0, 0.0, 1.0, look.leading as f32, 0.0], t) };
    let round = Stroke { width: 2.6, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    let chevron = |p: &mut Painter, cx: f32, cy: f32, half: f32, depth: f32, color: [f64; 4], width: f32| {
        let els = [El::Move(cx + depth / 2.0, cy - half), El::Line(cx - depth / 2.0, cy), El::Line(cx + depth / 2.0, cy + half)];
        let s = Stroke { width, ..round.clone() };
        if let (Some(path), Some(paint)) = (build(&els), paint(color.map(|v| v as f32), alpha)) {
            p.pix.stroke_path(&path, &paint, &s, ts(t), mask);
        }
    };
    if look.glass {
        // A 44-pt glass circle 16 pt in, with a chevron.
        let (d, cx, cy) = (44.0f32, 16.0 + 22.0, row / 2.0);
        let mut circle = Vec::new();
        arc_els(&mut circle, cx, cy, d / 2.0, 0.0, std::f32::consts::TAU, false);
        circle.push(El::Close);
        p.fill(&circle, [0.96, 0.96, 0.97, 1.0], alpha, false, t, mask);
        p.stroke(&circle, [0.0, 0.0, 0.0, 0.06], alpha, 0.5, t, mask);
        chevron(p, cx - 0.5, cy, 8.0, 8.0, [0.0, 0.0, 0.0, 1.0], 2.6);
    } else {
        chevron(p, 8.0 + 6.0, row / 2.0, 10.5, 10.0, TINT, 3.0);
        if let Some(label) = &back.label {
            let (glyphs, width) = crate::textkit::line_glyphs(emu, label, TITLE_SIZE, 0.0, TINT, BACK_LABEL_X, baseline);
            let glyphs = if look.rtl {
                let x = w as f64 - look.leading - BACK_LABEL_X - width;
                glyphs.into_iter().map(|g| Glyph { x: g.x - BACK_LABEL_X + x, ..g }).collect()
            } else {
                glyphs
            };
            p.sync_faces(emu, &glyphs);
            p.glyphs(&glyphs, alpha, if look.rtl { &bar_t } else { t }, mask);
        }
    }
}

/// A navigation bar's button items (baritems.rs): each on its glass capsule
/// (blue when prominent) under Liquid Glass, its symbol, image or title in
/// the content colour (light cyan on blue), or before iOS 26 in the tint.
fn draw_bar_items(emu: &mut Emu, p: &mut Painter, look: &crate::navigation::BarLook, alpha: f32, t: &[f32; 6], mask: Option<&Mask>) {
    use crate::baritems::{Content, GLASS_CONTENT, GLASS_FILL, PROMINENT_CONTENT, PROMINENT_FILL, TINT};
    for item in &look.items {
        let a = if item.enabled { alpha } else { alpha * 0.35 };
        if let Some(c) = item.capsule {
            let els = transform_els(&rounded_rect(c[2] as f32, c[3] as f32, (c[3] / 2.0) as f32), &[1.0, 0.0, 0.0, 1.0, c[0] as f32, c[1] as f32]);
            p.fill(&els, (if item.prominent { PROMINENT_FILL } else { GLASS_FILL }).map(|v| v as f32), a, false, t, mask);
            p.stroke(&els, [0.0, 0.0, 0.0, 0.06], a, 0.5, t, mask);
        }
        let color = if item.prominent { PROMINENT_CONTENT } else if look.glass { GLASS_CONTENT } else { TINT };
        let f = item.content_frame;
        let at = concat(&[1.0, 0.0, 0.0, 1.0, f[0] as f32, f[1] as f32], t);
        match &item.content {
            Content::Symbol(name, size) => {
                let scale = emu.device.scale;
                if let Some(pix) = crate::symbols::draw(emu, name, &Default::default(), *size, scale) {
                    let img = Image { pix: std::rc::Rc::new(pix), size: *size, scale, caps: [0.0; 4], orientation: 0, look: Default::default() };
                    let img = tinted(&img, color.map(|v| v as f32));
                    draw_contents(p, &img, [0.0; 4], f[2] as f32, f[3] as f32, a, &at, mask);
                }
            }
            Content::Image(id, _) => {
                if let Some(img) = emu.render.images.get(id).cloned() {
                    let img = if img.look.is_template() { tinted(&img, color.map(|v| v as f32)) } else { img };
                    draw_contents(p, &img, [0.0; 4], f[2] as f32, f[3] as f32, a, &at, mask);
                }
            }
            Content::Title(text, weight) => {
                let baseline = look.row / 2.0 + crate::navigation::TITLE_SIZE * 0.35;
                let (glyphs, _) = crate::textkit::line_glyphs(emu, text, crate::navigation::TITLE_SIZE, *weight, color, f[0], baseline);
                p.sync_faces(emu, &glyphs);
                p.glyphs(&glyphs, a, t, mask);
            }
            // (The app's view draws itself.)
            Content::Custom(_) => {}
        }
    }
}

/// Draw recorded ops into a new transparent bitmap of `size` points at `scale`.
pub(crate) fn rasterize(emu: &mut Emu, ops: &[Op], size: [f64; 2], scale: f64) -> Result<Pixmap> {
    let (w, h) = (((size[0] * scale).ceil() as u32).max(1), ((size[1] * scale).ceil() as u32).max(1));
    let pix = Pixmap::new(w, h).ok_or_else(|| Stop::Unsupported(format!("can't make a {w}×{h} bitmap")))?;
    let (font, emoji) = (crate::textkit::font_bytes(emu), crate::textkit::emoji_bytes(emu));
    let mut p = Painter { pix, font: font.as_deref().map(|v| v.as_slice()), emoji: emoji.as_deref().map(|v| v.as_slice()), faces: crate::coretext::draw_faces(emu) };
    let s = scale as f32;
    p.ops(ops, 1.0, &[s, 0.0, 0.0, s, 0.0, 0.0], None);
    Ok(p.pix)
}

/// Where a layer's `contents` goes in its w×h bounds, by `contentsGravity`:
/// stretched (resize), fitted or filled keeping its aspect, or at its own size
/// (pixels / contentsScale) against an edge or corner. iOS layers are y-down
/// but the gravities are named y-up, so "bottom" is the top edge (which is
/// why UIKit maps UIViewContentModeTop to kCAGravityBottom).
fn contents_rect(emu: &mut Emu, ly: &Layer, img: &Image, w: f32, h: f32) -> [f32; 4] {
    let gravity = if ly.contents_gravity == 0 { "resize".to_string() } else { crate::foundation::nsstring(emu, ly.contents_gravity).unwrap_or_default() };
    let scale = if ly.contents_scale > 0.0 { ly.contents_scale as f32 } else { 1.0 };
    let (iw, ih) = (img.pix.width() as f32 / scale, img.pix.height() as f32 / scale);
    let aspect = |fill: bool| {
        if iw <= 0.0 || ih <= 0.0 {
            return [0.0, 0.0, w, h];
        }
        let k = if fill { (w / iw).max(h / ih) } else { (w / iw).min(h / ih) };
        let (dw, dh) = (iw * k, ih * k);
        [(w - dw) / 2.0, (h - dh) / 2.0, dw, dh]
    };
    let (hx, vy) = match gravity.as_str() {
        "resizeAspect" => return aspect(false),
        "resizeAspectFill" => return aspect(true),
        "center" => (0.5, 0.5),
        "top" => (0.5, 1.0),
        "bottom" => (0.5, 0.0),
        "left" => (0.0, 0.5),
        "right" => (1.0, 0.5),
        "topLeft" => (0.0, 1.0),
        "topRight" => (1.0, 1.0),
        "bottomLeft" => (0.0, 0.0),
        "bottomRight" => (1.0, 0.0),
        _ => return [0.0, 0.0, w, h], // resize
    };
    [(w - iw) * hx, (h - ih) * vy, iw, ih]
}

#[allow(clippy::too_many_arguments)]
/// Draw a layer's `contents` image into its bounds, nine-slicing by `contentsCenter`.
/// A template image in a color: its alpha, in that color.
fn tinted(img: &Image, color: [f32; 4]) -> Image {
    let mut pix = (*img.pix).clone();
    for px in pix.pixels_mut() {
        let a = px.alpha() as f32 / 255.0 * color[3];
        let c = |v: f32| (v * a * 255.0).round().clamp(0.0, 255.0) as u8;
        let a8 = (a * 255.0).round() as u8;
        *px = tiny_skia::PremultipliedColorU8::from_rgba(c(color[0]).min(a8), c(color[1]).min(a8), c(color[2]).min(a8), a8).unwrap_or(*px);
    }
    Image { pix: std::rc::Rc::new(pix), ..img.clone() }
}

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
///
/// Like Core Animation's commit, a frame lays out before it displays: views
/// update what their drawRect: reads in layoutSubviews (React Native's
/// paragraphs set their text view's drawing frame there, after asking it to
/// redraw), so drawing first would cache text laid out for a stale frame.
pub fn render(emu: &mut Emu, scale: f32) -> Result<Pixmap> {
    crate::uiview::layout_windows(emu)?;
    crate::webkit::place(emu)?;
    for w in emu.ui.windows.clone() {
        display(emu, w)?;
    }
    let screen = crate::orientation::screen(emu);
    let (sw, sh) = ((screen.0 as f32 * scale) as u32, (screen.1 as f32 * scale) as u32);
    let mut pix = Pixmap::new(sw, sh).ok_or_else(|| Stop::Unsupported("can't make a pixmap".into()))?;
    pix.fill(tiny_skia::Color::BLACK);
    let (font, emoji) = (crate::textkit::font_bytes(emu), crate::textkit::emoji_bytes(emu));
    let mut p = Painter { pix, font: font.as_deref().map(|v| v.as_slice()), emoji: emoji.as_deref().map(|v| v.as_slice()), faces: crate::coretext::draw_faces(emu) };
    let t = [scale, 0.0, 0.0, scale, 0.0, 0.0];
    for w in emu.ui.windows.clone() {
        draw_view(emu, &mut p, w, 1.0, &t, None);
    }
    let visible = crate::textinput::keyboard_visible(emu);
    if visible > 0.0 {
        draw_keyboard(&mut p, screen, emu.input.keyboard_height, visible, &t);
    }
    draw_status_bar(emu, &mut p, scale)?;
    // The housing is fixed to the hardware: draw it in the native frame.
    let native = crate::orientation::from_portrait(emu).map(|v| v as f32);
    draw_cutout(&mut p, emu.device, &concat(&native, &t));
    Ok(p.pix)
}

// ---- the status bar (see statusbar.rs) ----

/// Sampled from the simulator: unlit bars and Wi-Fi bands, the battery's
/// outline and cap, and a charging bolt are the content's colour at these opacities.
const STATUS_UNLIT: f32 = 0.2;
const STATUS_OUTLINE: f32 = 0.4;
const STATUS_BOLT: f32 = 0.3;
const STATUS_WEIGHT: f64 = 0.3; // semibold

/// The status bar over the app, where it shows (portrait, not hidden by the
/// app): the time, the signal, Wi-Fi or the data network's name, and the
/// battery, in the places measured for this phone.
fn draw_status_bar(emu: &mut Emu, p: &mut Painter, scale: f32) -> Result<()> {
    if crate::orientation::status_bar_frame(emu)[3] <= 0.0 {
        return Ok(());
    }
    let (hidden, light) = crate::statusbar::appearance(emu)?;
    if hidden {
        return Ok(());
    }
    let Some(layout) = crate::statusbar::layout_for(emu.device) else {
        return Ok(());
    };
    let shown = crate::statusbar::shown(emu, &layout);
    let fg: [f32; 4] = if light { [1.0; 4] } else { [0.0, 0.0, 0.0, 1.0] };
    let t = [scale, 0.0, 0.0, scale, 0.0, 0.0];
    // Text sized so its digits are as tall as measured.
    let cap = p.font.and_then(|f| ttf_parser::Face::parse(f, 0).ok()).and_then(|f| f.capital_height().map(|c| c as f64 / f.units_per_em() as f64)).unwrap_or(0.727);
    let text = |emu: &mut Emu, p: &mut Painter, s: &str, cx: f64, baseline: f64, height: f64| {
        let (glyphs, width) = crate::textkit::line_glyphs(emu, s, height / cap, STATUS_WEIGHT, fg.map(|v| v as f64), 0.0, baseline);
        let placed: Vec<Glyph> = glyphs.into_iter().map(|g| Glyph { x: g.x + cx - width / 2.0, ..g }).collect();
        p.sync_faces(emu, &placed);
        p.glyphs(&placed, 1.0, &t, None);
    };
    let (cx, baseline, digits) = layout.time;
    {
        // The time's colon sits between the digits' top and bottom (SF's
        // time colon: measured 1.67 pt above the baseline's on 11-pt digits);
        // a day period ("AM") is smaller (6.5 pt caps by 8.5-pt digits).
        let color = fg.map(|v| v as f64);
        let (clock, period) = shown.time.split_once(' ').map_or((shown.time.as_str(), None), |(c, p)| (c, Some(p)));
        let (mut glyphs, mut width) = crate::textkit::line_glyphs(emu, clock, digits / cap, STATUS_WEIGHT, color, 0.0, baseline);
        if let Some(period) = period {
            let size = digits * 6.5 / 8.5 / cap;
            let (more, w) = crate::textkit::line_glyphs(emu, &format!(" {period}"), size, STATUS_WEIGHT, color, width, baseline);
            glyphs.extend(more);
            width += w;
        }
        let lift = digits * 1.67 / 11.0;
        // (The plain colon, lifted: not Inter's contextual time colon, which
        // shaping picks between digits.)
        let placed: Vec<Glyph> = glyphs.into_iter().map(|g| if g.ch == ':' { Glyph { x: g.x + cx - width / 2.0, y: g.y - lift, gid: None, ..g } } else { Glyph { x: g.x + cx - width / 2.0, ..g } }).collect();
        p.sync_faces(emu, &placed);
        p.glyphs(&placed, 1.0, &t, None);
    }

    // The signal: bars (lit or not); with no service, dots as wide as a bar
    // and as tall; searching, a wave runs along the dots, raising each
    // towards its bar and darkening it (to 60%, measured).
    use crate::statusbar::{Data, Signal};
    let (x0, bottom, w, pitch, heights) = layout.bars;
    let wave = crate::statusbar::wave(emu, 4);
    for (i, &full) in heights.iter().enumerate() {
        let (h, alpha) = match shown.signal {
            Signal::Bars(n) => (full, if (i as u8) < n { 1.0 } else { STATUS_UNLIT }),
            Signal::Failed => (w, STATUS_UNLIT),
            Signal::Searching => (w + wave[i] * (full - w), STATUS_UNLIT + wave[i] as f32 * (0.6 - STATUS_UNLIT)),
            Signal::Hidden => continue,
        };
        let bar = transform_els(&rounded_rect(w as f32, h as f32, w as f32 * 0.4), &[1.0, 0.0, 0.0, 1.0, (x0 + i as f64 * pitch) as f32, (bottom - h) as f32]);
        p.fill(&bar, fg, alpha, false, &t, None);
    }
    match &shown.data {
        Data::Wifi(lit) => {
            let lit = |band: u8| if *lit >= band { 1.0 } else { STATUS_UNLIT };
            draw_wifi(p, layout.wifi, [lit(1), lit(2), lit(3)], fg, &t)
        }
        // Searching, a wave runs in from the outside.
        Data::WifiSearching => {
            let k = crate::statusbar::wave(emu, 3);
            let a = |i: usize| STATUS_UNLIT + k[i] as f32 * (1.0 - STATUS_UNLIT);
            draw_wifi(p, layout.wifi, [a(2), a(1), a(0)], fg, &t)
        }
        Data::Cellular(label) => {
            // Centred where the Wi-Fi fan goes, 78% of its height (measured "5G", "LTE").
            let [wx0, _, wx1, wy1] = layout.wifi;
            text(emu, p, label, (wx0 + wx1) / 2.0, wy1 - 0.33, (layout.wifi[3] - layout.wifi[1]) * 0.78);
        }
        Data::Hidden => {}
    }
    draw_battery(p, &layout, shown.battery, light, fg, scale);
    Ok(())
}

/// The Wi-Fi fan in its box: a wedge and two bands spreading up from the
/// bottom's middle, at these opacities (the wedge's, the middle band's, the outer's).
fn draw_wifi(p: &mut Painter, b: [f64; 4], alphas: [f32; 3], fg: [f32; 4], t: &[f32; 6]) {
    use std::f32::consts::FRAC_PI_2;
    // Measured on a 12-pt fan: bands 2.67 thick, their outer radii 12 and
    // 7.67, the wedge's 3.67; round-ended, 37.5° either side of upright.
    let s = (b[3] - b[1]) as f32 / 12.0;
    let (cx, apex) = (((b[0] + b[2]) / 2.0) as f32, b[3] as f32);
    let half = 37.5f32.to_radians();
    for (band, outer) in [(2usize, 12.0f32), (1, 7.67)] {
        let thick = 2.67 * s;
        let mut arc = Vec::new();
        arc_els(&mut arc, cx, apex, outer * s - thick / 2.0, -FRAC_PI_2 - half, -FRAC_PI_2 + half, false);
        if let (Some(path), Some(paint)) = (build(&arc), paint(fg, alphas[band])) {
            let stroke = Stroke { width: thick, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
            p.pix.stroke_path(&path, &paint, &stroke, ts(t), None);
        }
    }
    let r = 3.67 * s;
    let a = 42f32.to_radians();
    let mut wedge = vec![El::Move(cx, apex)];
    arc_els(&mut wedge, cx, apex, r, -FRAC_PI_2 - a, -FRAC_PI_2 + a, false);
    wedge.push(El::Close);
    p.fill(&wedge, fg, alphas[0], false, t, None);
}

/// The battery: its outline and cap, filled to its level (yellow in Low
/// Power Mode, else red at 20% and below, green while plugged in), with a bolt over it while plugged in,
/// cut out of what's under it by a 1-pt gap. Drawn on a layer of its own so
/// the gap shows the app beneath.
fn draw_battery(p: &mut Painter, l: &crate::statusbar::Layout, b: crate::statusbar::Battery, light: bool, fg: [f32; 4], scale: f32) {
    use crate::statusbar::Charge;
    let [x0, y0, x1, y1] = l.battery.map(|v| v as f32);
    let [cx0, cy0, cx1, cy1] = l.cap.map(|v| v as f32);
    let s = (y1 - y0) / 12.0;
    // The layer, on whole pixels.
    let (px, py) = (((x0 - 4.0 * s) * scale).floor(), ((y0 - 4.0 * s) * scale).floor());
    let (pw, ph) = (((cx1 + 4.0 * s) * scale - px).ceil() as u32, ((y1 + 4.0 * s) * scale - py).ceil() as u32);
    let Some(pix) = Pixmap::new(pw.max(1), ph.max(1)) else { return };
    let mut layer = Painter { pix, font: None, emoji: None, faces: Default::default() };
    let t = [scale, 0.0, 0.0, scale, -px, -py];
    let at = |x: f32, y: f32| [1.0, 0.0, 0.0, 1.0, x, y];
    // A 1-pt outline inside the body's edge (its corners 4 pt), and the cap.
    let body = transform_els(&rounded_rect(x1 - x0 - s, y1 - y0 - s, 4.0 * s - s / 2.0), &at(x0 + s / 2.0, y0 + s / 2.0));
    layer.stroke(&body, fg, STATUS_OUTLINE, s, &t, None);
    layer.fill(&transform_els(&rounded_rect(cx1 - cx0, cy1 - cy0, cx1 - cx0), &at(cx0, cy0)), fg, STATUS_OUTLINE, false, &t, None);
    // The level, 2 pt in.
    let plugged = b.charge != Charge::Unplugged;
    let low = b.level <= 0.2 && !b.low_power;
    // Low Power Mode fills it yellow, whatever the level.
    let color = if b.low_power {
        if light { [1.0, 0.839, 0.0, 1.0] } else { [1.0, 0.8, 0.0, 1.0] }
    } else if low {
        if light { [1.0, 0.259, 0.271, 1.0] } else { [1.0, 0.220, 0.235, 1.0] }
    } else if plugged {
        if light { [0.188, 0.820, 0.345, 1.0] } else { [0.204, 0.780, 0.349, 1.0] }
    } else {
        fg
    };
    let inner = (x1 - x0 - 4.0 * s) * b.level.clamp(0.0, 1.0) as f32;
    if inner > 0.0 {
        let fill = transform_els(&rounded_rect(inner, y1 - y0 - 4.0 * s, 2.0 * s), &at(x0 + 2.0 * s, y0 + 2.0 * s));
        layer.fill(&fill, color, 1.0, false, &t, None);
    }
    if plugged {
        // A bolt as tall as the body, a little right of its middle (measured: 6 × 12.33 pt).
        let (bw, bh) = (6.0 * s, 12.33 * s);
        let (bx, by) = ((x0 + x1) / 2.0 + 1.33 * s - bw / 2.0, (y0 + y1) / 2.0 - bh / 2.0);
        let pts = [(0.72, 0.0), (0.0, 0.58), (0.44, 0.58), (0.28, 1.0), (1.0, 0.40), (0.56, 0.40)];
        let mut bolt: Vec<El> = pts.iter().enumerate().map(|(i, &(u, v))| {
            let (x, y) = (bx + u * bw, by + v * bh);
            if i == 0 { El::Move(x, y) } else { El::Line(x, y) }
        }).collect();
        bolt.push(El::Close);
        if let Some(path) = build(&bolt) {
            let mut gap = Paint { blend_mode: tiny_skia::BlendMode::Clear, ..Default::default() };
            gap.anti_alias = true;
            let stroke = Stroke { width: 2.0 * s, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            layer.pix.stroke_path(&path, &gap, &stroke, ts(&t), None);
            layer.pix.fill_path(&path, &gap, FillRule::Winding, ts(&t), None);
        }
        // Over a red (low) level it's the content's colour; otherwise dimmed.
        layer.fill(&bolt, fg, if low { 1.0 } else { STATUS_BOLT }, false, &t, None);
    }
    p.pix.draw_pixmap(px as i32, py as i32, layer.pix.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
}

/// The sensor housing over the screen, as the Simulator draws it into its
/// screenshots: the Dynamic Island (a black capsule) or the notch.
fn draw_cutout(p: &mut Painter, d: &crate::device::Device, t: &[f32; 6]) {
    use crate::device::Cutout;
    let cx = d.width as f32 / 2.0;
    let black = [0.0, 0.0, 0.0, 1.0];
    match d.cutout {
        Cutout::None => {}
        Cutout::Island { top, width, height } => {
            let (w, h) = (width as f32, height as f32);
            let capsule = transform_els(&rounded_rect(w, h, h / 2.0), &[1.0, 0.0, 0.0, 1.0, cx - w / 2.0, top as f32]);
            p.fill(&capsule, black, 1.0, false, t, None);
        }
        Cutout::Notch { width, height } => {
            p.fill(&notch_els(cx, width as f32, height as f32), black, 1.0, false, t, None);
        }
    }
}

/// A notch hanging from the top edge: concave shoulders where it meets the
/// edge, straight sides, rounded bottom corners. `width` includes the shoulders.
fn notch_els(cx: f32, width: f32, height: f32) -> Vec<El> {
    use std::f32::consts::{FRAC_PI_2, PI};
    const SHOULDER: f32 = 6.0;
    let r = (height * 0.6).min(20.0);
    let (x0, x1) = (cx - width / 2.0 + SHOULDER, cx + width / 2.0 - SHOULDER);
    // A quarter circle's control-point distance.
    let k = SHOULDER * 0.5523;
    let mut out = vec![El::Move(x0 - SHOULDER, 0.0), El::Cubic(x0 - SHOULDER + k, 0.0, x0, SHOULDER - k, x0, SHOULDER), El::Line(x0, height - r)];
    arc_els(&mut out, x0 + r, height - r, r, PI, FRAC_PI_2, true);
    out.push(El::Line(x1 - r, height));
    arc_els(&mut out, x1 - r, height - r, r, FRAC_PI_2, 0.0, true);
    out.extend([El::Line(x1, SHOULDER), El::Cubic(x1, SHOULDER - k, x1 + SHOULDER - k, 0.0, x1 + SHOULDER, 0.0), El::Close]);
    out
}

/// The screen inside a phone body: the device's rounded display corners,
/// a dark bezel, the side buttons and (on SE-style phones) the home button.
/// Not a picture of Apple's hardware: a plain stand-in for previews.
pub fn render_shell(emu: &mut Emu, scale: f32) -> Result<Pixmap> {
    let screen = render(emu, scale)?;
    let o = crate::orientation::current(emu);
    Ok(shell(emu.device, o, &screen, scale)?.0)
}

/// A phone around `screen` (upright for the interface orientation `o`, at
/// `scale` pixels per point), turned as the phone is held. Also returns
/// where the screen is in the result, in pixels (x, y, width, height).
pub fn shell(d: &crate::device::Device, o: crate::device::Orientation, screen: &Pixmap, scale: f32) -> Result<(Pixmap, [f32; 4])> {
    let shell = Shell::new(d, o, scale)?;
    Ok((shell.compose(screen), shell.screen))
}

/// The phone around the screen, drawn once for an orientation and scale:
/// a frame goes into its screen with [`Shell::compose`] (the desktop window
/// does that for every frame, so the body isn't drawn again each time).
pub struct Shell {
    /// The body, buttons and an empty (dark) screen, turned as the phone is held.
    body: Pixmap,
    /// Where the screen is in `body` (px): x, y, width, height.
    pub screen: [f32; 4],
    /// The display's rounded corners, over all of `body`.
    mask: Option<Mask>,
}

impl Shell {
    pub fn new(d: &crate::device::Device, o: crate::device::Orientation, scale: f32) -> Result<Shell> {
        // The body is drawn upright in portrait; then the whole phone turns as it's held.
        // Bezel around the screen (pt), and room for the buttons at the sides.
        const BEZEL: f32 = 14.0;
        const BUTTONS: f32 = 4.0;
        let (chin, forehead) = if d.home_button { (88.0f32, 88.0f32) } else { (BEZEL, BEZEL) };
        let (sw, sh) = (d.width as f32, d.height as f32);
        let (bw, bh) = (sw + 2.0 * BEZEL, sh + forehead + chin);
        let total = (bw + 2.0 * BUTTONS, bh);
        let pix = Pixmap::new((total.0 * scale) as u32, (total.1 * scale) as u32).ok_or_else(|| Stop::Unsupported("can't make a pixmap".into()))?;
        let mut p = Painter { pix, font: None, emoji: None, faces: Default::default() };
        let t = [scale, 0.0, 0.0, scale, 0.0, 0.0];
        let body_r = if d.home_button { 56.0 } else { d.corner_radius as f32 + BEZEL };
        // iPhone corners are continuous curves, the display's and the body's.
        let continuous = !d.home_button;
        let body = transform_els(&corner_shape(bw, bh, body_r, 15, continuous), &[1.0, 0.0, 0.0, 1.0, BUTTONS, 0.0]);
        // Side buttons first, so the body covers their inner edges.
        let button = |p: &mut Painter, x: f32, y: f32, h: f32| {
            let b = transform_els(&rounded_rect(BUTTONS + 3.0, h, 2.0), &[1.0, 0.0, 0.0, 1.0, x, y]);
            p.fill(&b, [0.17, 0.17, 0.18, 1.0], 1.0, false, &t, None);
        };
        let top = forehead + if d.home_button { 0.0 } else { 100.0 };
        button(&mut p, 0.0, top, 32.0); // action / ring
        button(&mut p, 0.0, top + 56.0, 60.0); // volume up
        button(&mut p, 0.0, top + 128.0, 60.0); // volume down
        button(&mut p, total.0 - BUTTONS - 3.0, top + 50.0, 96.0); // side button
        p.fill(&body, [0.11, 0.11, 0.12, 1.0], 1.0, false, &t, None);
        p.stroke(&body, [0.35, 0.35, 0.37, 1.0], 1.0, 1.5, &t, None);
        let (ox, oy) = (BUTTONS + BEZEL, forehead);
        if d.home_button {
            let (cx, cy, r) = (total.0 / 2.0, bh - chin / 2.0, 22.0);
            let mut ring = Vec::new();
            arc_els(&mut ring, cx, cy, r, 0.0, std::f32::consts::TAU, false);
            ring.push(El::Close);
            p.stroke(&ring, [0.4, 0.4, 0.42, 1.0], 1.0, 2.0, &t, None);
        }
        let (w, h) = (p.pix.width(), p.pix.height());
        let (ow, oh) = if o.is_landscape() { (h, w) } else { (w, h) };
        let m = o.from_portrait(w as f64, h as f64);
        // Where the screen lands: its corners through the turn.
        let corners = [(ox * scale, oy * scale), ((ox + sw) * scale, (oy + sh) * scale)].map(|(x, y)| {
            let (x, y) = (x as f64, y as f64);
            ((m[0] * x + m[2] * y + m[4]) as f32, (m[1] * x + m[3] * y + m[5]) as f32)
        });
        let (x0, x1) = (corners[0].0.min(corners[1].0), corners[0].0.max(corners[1].0));
        let (y0, y1) = (corners[0].1.min(corners[1].1), corners[0].1.max(corners[1].1));
        let body = turn(&p.pix, m, ow, oh)?;
        // The display's corners where the screen landed (a rounded rectangle looks the same turned).
        // The display's corners (continuous, as moving screens' cards are, so the two coincide).
        let shape = transform_els(&corner_shape(x1 - x0, y1 - y0, d.corner_radius as f32 * scale, 15, !d.home_button), &[1.0, 0.0, 0.0, 1.0, x0, y0]);
        let mask = Painter { pix: Pixmap::new(ow, oh).ok_or_else(|| Stop::Unsupported("can't make a pixmap".into()))?, font: None, emoji: None, faces: Default::default() }.intersect(None, &shape, &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        Ok(Shell { body, screen: [x0, y0, x1 - x0, y1 - y0], mask })
    }

    /// The phone showing `screen` (upright for the orientation the shell was made for).
    pub fn compose(&self, screen: &Pixmap) -> Pixmap {
        let mut out = self.body.clone();
        let [x, y, _, _] = self.screen;
        let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Nearest, ..Default::default() };
        out.draw_pixmap(0, 0, screen.as_ref(), &paint, Transform::from_translate(x.round(), y.round()), self.mask.as_ref());
        out
    }

    /// The body with nothing on the screen (before the first frame).
    pub fn blank(&self) -> Pixmap {
        let mut out = self.body.clone();
        let [x, y, w, h] = self.screen;
        if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
            let mut paint = Paint::default();
            paint.set_color(tiny_skia::Color::BLACK);
            out.fill_rect(r, &paint, Transform::identity(), self.mask.as_ref());
        }
        out
    }
}

/// `src` drawn through the pixel-space map `m` into a new `w`×`h` bitmap.
fn turn(src: &Pixmap, m: [f64; 6], w: u32, h: u32) -> Result<Pixmap> {
    let mut out = Pixmap::new(w, h).ok_or_else(|| Stop::Unsupported("can't make a pixmap".into()))?;
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Nearest, ..Default::default() };
    out.draw_pixmap(0, 0, src.as_ref(), &paint, ts(&m.map(|v| v as f32)), None);
    Ok(out)
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
    ("_UIRectFill", ui_rect_fill),
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
    ("_CGContextSetLineCap", |e| {
        let cap = e.arg(1) as u8;
        with_ctx(e, |c| c.gs.line.cap = cap)
    }),
    ("_CGContextSetLineJoin", |e| {
        let join = e.arg(1) as u8;
        with_ctx(e, |c| c.gs.line.join = join)
    }),
    // (phase d0, lengths x1, count x2; no lengths: solid.)
    ("_CGContextSetLineDash", |e| {
        let (phase, lengths, count) = (d(e, 0), e.arg(1), (e.arg(2) as usize).min(8));
        let mut dash = [0.0f32; 8];
        for (i, v) in dash.iter_mut().enumerate().take(if lengths == 0 { 0 } else { count }) {
            *v = f64::from_bits(e.read_u64(lengths + i as u64 * 8)?) as f32;
        }
        let len = if lengths == 0 { 0 } else { count as u8 };
        with_ctx(e, |c| {
            c.gs.line.dash = dash;
            c.gs.line.dash_len = len;
            c.gs.line.dash_phase = phase;
        })
    }),
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
            let k = ctx.gs.ctm[0].abs().max(ctx.gs.ctm[3].abs());
            let width = ctx.gs.line_width * k;
            ctx.ops.push(Op::Stroke { path, color: ctx.gs.stroke, width, line: ctx.gs.line.scaled(k), clip: ctx.gs.clip.clone() });
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
    ("_CGPathApply", path_apply),
    ("_CGPathGetBoundingBox", |e| {
        let r = path_bounds(&path_els(e, e.arg(0)));
        ret_rect(e, r)
    }),
    ("_CGPathGetPathBoundingBox", |e| {
        let r = path_bounds(&path_els(e, e.arg(0)));
        ret_rect(e, r)
    }),
    ("_CGPathIsEmpty", |e| {
        let empty = path_els(e, e.arg(0)).is_empty();
        e.ret(empty as u64);
        Ok(())
    }),
    ("_CGPathGetCurrentPoint", |e| {
        let (x, y) = last_point(&path_els(e, e.arg(0)));
        ret_rect(e, [x as f64, y as f64, 0.0, 0.0])
    }),
    ("_CGPathCreateCopy", |e| {
        let els = path_els(e, e.arg(0));
        let p = new_path(e, els)?;
        e.ret(p);
        Ok(())
    }),
    ("_CGPathCreateMutableCopy", |e| {
        let els = path_els(e, e.arg(0));
        let p = new_path(e, els)?;
        e.ret(p);
        Ok(())
    }),
    ("_CGPathCreateCopyByTransformingPath", |e| {
        let els = path_els(e, e.arg(0));
        let m = optional_affine(e, e.arg(1))?;
        let p = new_path(e, m.map_or(els.clone(), |m| transform_els(&els, &m)))?;
        e.ret(p);
        Ok(())
    }),
    ("_CGPathMoveToPoint", |e| {
        let (p, m) = (e.arg(0), optional_affine(e, e.arg(1))?);
        path_add(e, p, vec![El::Move(d(e, 0), d(e, 1))], m);
        Ok(())
    }),
    ("_CGPathAddLineToPoint", |e| {
        let (p, m) = (e.arg(0), optional_affine(e, e.arg(1))?);
        path_add(e, p, vec![El::Line(d(e, 0), d(e, 1))], m);
        Ok(())
    }),
    ("_CGPathAddCurveToPoint", |e| {
        let (p, m) = (e.arg(0), optional_affine(e, e.arg(1))?);
        path_add(e, p, vec![El::Cubic(d(e, 0), d(e, 1), d(e, 2), d(e, 3), d(e, 4), d(e, 5))], m);
        Ok(())
    }),
    ("_CGPathAddQuadCurveToPoint", |e| {
        let (p, m) = (e.arg(0), optional_affine(e, e.arg(1))?);
        let (x0, y0) = last_point(&path_els(e, p));
        // Into the path's space first: the start point is already there.
        let id = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let (cx, cy) = apply(m.as_ref().unwrap_or(&id), d(e, 0), d(e, 1));
        let (x, y) = apply(m.as_ref().unwrap_or(&id), d(e, 2), d(e, 3));
        let c1 = (x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0));
        let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
        path_add(e, p, vec![El::Cubic(c1.0, c1.1, c2.0, c2.1, x, y)], None);
        Ok(())
    }),
    ("_CGBitmapContextCreate", bitmap_create),
    // A CGImage of what's been drawn so far.
    ("_CGBitmapContextCreateImage", |e| {
        let c = e.arg(0);
        flush_bitmap(e, c)?;
        let Some(b) = e.render.bitmaps.get(&c).copied() else {
            e.ret(0);
            return Ok(());
        };
        let img = match read_bitmap(e, &b)? {
            Some(pix) => crate::imageio::new_cg_image(e, std::rc::Rc::new(pix))?,
            None => 0,
        };
        e.ret(img);
        Ok(())
    }),
    // (c, rect in d0–d3, image)
    ("_CGContextDrawImage", |e| {
        let (c, img) = (e.arg(0), e.arg(1));
        let rect = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)].map(|v| v as f64);
        let Some(pix) = e.render.images.get(&img).map(|i| i.pix.clone()) else { return Ok(()) };
        draw_image(e, c, pix, rect, false, 1.0)
    }),
    ("_CGBitmapContextCreateWithData", bitmap_create),
    ("_CGBitmapContextGetData", |e| bitmap_get(e, |b| b.data)),
    ("_CGBitmapContextGetWidth", |e| bitmap_get(e, |b| b.w as u64)),
    ("_CGBitmapContextGetHeight", |e| bitmap_get(e, |b| b.h as u64)),
    ("_CGBitmapContextGetBytesPerRow", |e| bitmap_get(e, |b| b.bpr)),
    ("_CGBitmapContextGetBitsPerPixel", |e| bitmap_get(e, |b| b.bpp as u64 * 8)),
    ("_CGBitmapContextGetBitsPerComponent", |e| bitmap_get(e, |_| 8)),
    ("_CGBitmapContextGetBitmapInfo", |e| bitmap_get(e, |b| b.info as u64)),
    ("_CGBitmapContextGetAlphaInfo", |e| bitmap_get(e, |b| (b.info & 0x1f) as u64)),
    ("_CGContextSetGrayFillColor", |e| {
        let (g, a) = (d(e, 0), d(e, 1));
        with_ctx(e, |c| c.gs.fill = [g, g, g, a])
    }),
    ("_CGContextSetGrayStrokeColor", |e| {
        let (g, a) = (d(e, 0), d(e, 1));
        with_ctx(e, |c| c.gs.stroke = [g, g, g, a])
    }),
    ("_CGContextSetFillColor", |e| {
        let p = e.arg(1);
        let mut col = [0.0f32; 4];
        for (i, v) in col.iter_mut().enumerate() {
            *v = f64::from_bits(e.read_u64(p + i as u64 * 8)?) as f32;
        }
        with_ctx(e, |c| c.gs.fill = col)
    }),
    ("_CGContextSetFillColorSpace", nothing),
    ("_CGContextSetStrokeColorSpace", nothing),
    ("_CGContextSetBlendMode", nothing),
    ("_CGContextSetTextDrawingMode", nothing),
    ("_CGContextSetTextMatrix", nothing),
    ("_CGContextSetTextPosition", nothing),
    ("_CGContextSetFontSmoothingStyle", nothing),
    ("_CGContextSetShouldSubpixelPositionFonts", nothing),
    ("_CGContextSetShouldSubpixelQuantizeFonts", nothing),
    ("_CGContextSetAllowsFontSubpixelPositioning", nothing),
    ("_CGContextSetAllowsFontSubpixelQuantization", nothing),
    ("_CGContextSetAllowsFontSmoothing", nothing),
    ("_CGContextSetInterpolationQuality", nothing),
    ("_CGContextFlush", nothing),
    ("_CGContextSynchronize", nothing),
    ("_CGContextGetCTM", |e| {
        let m = e.render.contexts.get(&e.arg(0)).map_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], |c| c.gs.ctm);
        let out = e.cpu.x[8];
        for (i, v) in m.iter().enumerate() {
            e.write_u64(out + i as u64 * 8, (*v as f64).to_bits())?;
        }
        Ok(())
    }),
    ("_CGContextClearRect", |e| {
        // Transparent black over the rect (a copy, not a blend).
        let (c, r) = (e.arg(0), [d(e, 0), d(e, 1), d(e, 2), d(e, 3)]);
        flush_bitmap(e, c)?;
        let (Some(b), Some(ctx)) = (e.render.bitmaps.get(&c).copied(), e.render.contexts.get(&c)) else { return Ok(()) };
        let m = ctx.gs.ctm;
        let (ax, ay) = apply(&m, r[0], r[1]);
        let (bx, by) = apply(&m, r[0] + r[2], r[1] + r[3]);
        let (x0, x1) = (ax.min(bx).max(0.0) as u64, (ax.max(bx).min(b.w as f32)) as u64);
        let (y0, y1) = (ay.min(by).max(0.0) as u64, (ay.max(by).min(b.h as f32)) as u64);
        if x1 > x0 {
            let zeros = vec![0u8; ((x1 - x0) * b.bpp as u64) as usize];
            for y in y0..y1 {
                e.p.memory.write(b.data + y * b.bpr + x0 * b.bpp as u64, &zeros)?;
            }
        }
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

    fn painter(w: u32, h: u32) -> Painter<'static> {
        Painter { pix: Pixmap::new(w, h).unwrap(), font: None, emoji: None, faces: Default::default() }
    }

    /// The largest difference in any channel between two pictures.
    fn largest_difference(a: &Pixmap, b: &Pixmap) -> u8 {
        a.data().iter().zip(b.data()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)
    }

    /// A big box filled in pieces (rectangles, and paths for the corners) is
    /// the box's shape filled: rounded or square, continuous or circular, at
    /// fractional positions and scales, opaque or not.
    #[test]
    fn boxes_in_pieces_are_their_shape() {
        let cases: &[([f32; 4], bool, [f32; 6], [f32; 4])] = &[
            ([47.33; 4], true, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0]),
            ([12.0, 0.0, 30.0, 5.0], false, [3.0, 0.0, 0.0, 3.0, 10.3, 7.7], [0.2, 0.4, 0.9, 1.0]),
            ([0.0; 4], false, [1.5, 0.0, 0.0, 1.5, 3.25, 0.5], [0.9, 0.1, 0.1, 0.5]),
            ([60.0; 4], true, [2.0, 0.0, 0.0, 2.0, -40.6, 12.2], [0.0, 0.0, 0.0, 0.8]),
        ];
        for &(radii, continuous, t, color) in cases {
            let (w, h) = (180.0, 260.0);
            let mut fast = painter(600, 900);
            fast.fill_box(w, h, radii, continuous, color, 1.0, &t, None);
            let mut slow = painter(600, 900);
            let (r, _) = clamp_radii(w, h, radii, continuous);
            slow.fill(&corner_shape_radii(w, h, r, continuous), color, 1.0, false, &t, None);
            let d = largest_difference(&fast.pix, &slow.pix);
            // (On edges across pixels: rectangles' coverage is exact, a path's
            // is in quarters of a pixel, so they may differ by up to an eighth.)
            assert!(d <= 32, "radii {radii:?} at {t:?}: off by up to {d}");
            // No seams: the inside is solid.
            let px = fast.pix.pixel((t[4] + w * t[0] / 2.0) as u32, (t[5] + h * t[3] / 2.0) as u32).unwrap();
            assert_eq!(px.alpha(), (color[3] * 255.0).round() as u8);
        }
    }

    /// Text from the glyph cache looks as rasterised in place: the first
    /// drawing rasterises, later ones copy; both match drawing the outline.
    #[test]
    fn cached_glyphs_match_rasterised_ones() {
        // pomme's text font (from the sysroot; skipped without it).
        let Ok(font) = std::fs::read("../../vendor/sysroot/System/Library/Fonts/Inter.ttf") else { return };
        let face = ttf_parser::Face::parse(&font, 0).unwrap();
        let gid = face.glyph_index('g').unwrap();
        let size = 17.0f32;
        let s = size / face.units_per_em() as f32;
        let t = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let color = [0.1, 0.2, 0.3, 1.0];
        for origin in [(10.0f32, 30.0f32), (10.37, 30.81)] {
            let mut slow = painter(120, 120);
            let mut b = Outline(PathBuilder::new());
            face.outline_glyph(gid, &mut b).unwrap();
            let path = b.0.finish().unwrap();
            let place = [s, 0.0, 0.0, -s, origin.0, origin.1];
            slow.pix.fill_path(&path, &paint(color, 1.0).unwrap(), FillRule::Winding, ts(&concat(&place, &t)), None);
            for _ in 0..2 {
                let mut fast = painter(120, 120);
                let drawn = glyph_cache::draw(&mut fast.pix, (((font.as_ptr() as usize), font.len()), gid.0, [26, 51, 77, 255]), face.glyph_bounding_box(gid), s, origin, &t, 1.0, None, |pix, base| {
                    pix.fill_path(&path, &paint(color, 1.0).unwrap(), FillRule::Winding, base, None);
                });
                assert!(drawn);
                // (Positions are kept to a quarter pixel, so edges may differ a little.)
                let d = largest_difference(&fast.pix, &slow.pix);
                let inked = |p: &Pixmap| p.data().chunks(4).filter(|c| c[3] > 0).count() as i64;
                assert!((inked(&fast.pix) - inked(&slow.pix)).abs() <= 12, "{origin:?}");
                assert!(d <= 140, "{origin:?}: off by up to {d}");
            }
        }
    }

    #[test]
    fn arcs_end_where_they_should() {
        let mut els = Vec::new();
        arc_els(&mut els, 0.0, 0.0, 10.0, 0.0, std::f32::consts::PI, false);
        let Some(El::Cubic(.., x, y)) = els.last().copied() else { panic!() };
        assert!((x + 10.0).abs() < 1e-4 && y.abs() < 1e-4);
    }
}

#[cfg(test)]
mod shell_tests {
    use super::*;
    use crate::device::Orientation;

    /// The screen sits inside the bezel, and turns with the phone: in
    /// landscape the picture is wider than tall and the screen too.
    #[test]
    fn shell_places_the_screen() {
        let d = crate::device::find("iphone-17").unwrap();
        for o in [Orientation::Portrait, Orientation::LandscapeRight, Orientation::LandscapeLeft] {
            let (sw, sh) = d.screen(o);
            let screen = Pixmap::new(sw as u32, sh as u32).unwrap();
            let (pix, r) = shell(d, o, &screen, 1.0).unwrap();
            assert_eq!((r[2].round(), r[3].round()), (sw as f32, sh as f32), "{o:?}");
            assert!(r[0] > 0.0 && r[1] > 0.0 && r[0] + r[2] < pix.width() as f32 && r[1] + r[3] < pix.height() as f32, "{o:?} {r:?}");
            assert_eq!(pix.width() > pix.height(), o.is_landscape(), "{o:?}");
        }
    }
}
