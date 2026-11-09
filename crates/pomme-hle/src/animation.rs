//! Core Animation: animations on layers, evaluated on the virtual clock.
//!
//! As on iOS, a layer has its model values (what the app set) and, while
//! animations run, presentation values: what's on screen at this moment. An
//! animation never changes the model; the renderer asks [`present_view`] /
//! [`present_layer`] for the presentation values at the current time. When
//! an animation ends it's removed (unless `removedOnCompletion` is off and it
//! fills forwards), its delegate hears `animationDidStop:finished:`, and
//! completion blocks run, on the main run loop at that moment.
//!
//! Animations come from `-[CALayer addAnimation:forKey:]` (CABasicAnimation,
//! CAKeyframeAnimation, CASpringAnimation, CAAnimationGroup, read when added,
//! as Core Animation copies them) and from UIView's animation blocks: a view
//! property changed inside `+animateWithDuration:…` animates from what was on
//! screen to the new value.
//!
//! Time is the guest's clock (instructions retired, plus idle time skipped),
//! so a screenshot at 0.15 s into an animation is the same every run. While
//! anything animates, a live host gets frames continuously.

use crate::foundation;
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};
use std::collections::BTreeMap;

/// An animatable value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Val {
    N(f64),
    /// A point or size.
    P([f64; 2]),
    /// A rectangle.
    R([f64; 4]),
    /// A colour, RGBA.
    C([f64; 4]),
    /// A CATransform3D, row-major as Core Animation stores it (m11 m12 … m44).
    M([f64; 16]),
}

/// An animatable property (a key path).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Position,
    PositionX,
    PositionY,
    Bounds,
    BoundsSize,
    BoundsOrigin,
    Opacity,
    Transform,
    Scale,
    ScaleX,
    ScaleY,
    Rotation,
    TranslationX,
    TranslationY,
    Background,
    CornerRadius,
    BorderWidth,
    ShadowOpacity,
}

impl Prop {
    pub fn parse(key_path: &str) -> Option<Prop> {
        Some(match key_path {
            "position" => Prop::Position,
            "position.x" => Prop::PositionX,
            "position.y" => Prop::PositionY,
            "bounds" => Prop::Bounds,
            "bounds.size" => Prop::BoundsSize,
            "bounds.origin" => Prop::BoundsOrigin,
            "opacity" => Prop::Opacity,
            "transform" => Prop::Transform,
            "transform.scale" => Prop::Scale,
            "transform.scale.x" => Prop::ScaleX,
            "transform.scale.y" => Prop::ScaleY,
            "transform.rotation" | "transform.rotation.z" => Prop::Rotation,
            "transform.translation.x" => Prop::TranslationX,
            "transform.translation.y" => Prop::TranslationY,
            "backgroundColor" => Prop::Background,
            "cornerRadius" => Prop::CornerRadius,
            "borderWidth" => Prop::BorderWidth,
            "shadowOpacity" => Prop::ShadowOpacity,
            _ => return None,
        })
    }
}

/// How progress through an animation maps to progress between its values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timing {
    Linear,
    /// A cubic Bézier from (0,0) to (1,1) through these control points.
    Bezier([f64; 4]),
    /// A damped spring (CASpringAnimation): follows the physics over time.
    Spring { mass: f64, stiffness: f64, damping: f64, velocity: f64 },
}

/// The named timing functions (`kCAMediaTimingFunction…`).
pub fn named_timing(name: &str) -> Timing {
    match name {
        "easeIn" => Timing::Bezier([0.42, 0.0, 1.0, 1.0]),
        "easeOut" => Timing::Bezier([0.0, 0.0, 0.58, 1.0]),
        "easeInEaseOut" => Timing::Bezier([0.42, 0.0, 0.58, 1.0]),
        "default" => Timing::Bezier([0.25, 0.1, 0.25, 1.0]),
        _ => Timing::Linear,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    /// fromValue / toValue / byValue (missing ends take the model value).
    Basic { from: Option<Val>, to: Option<Val>, by: Option<Val> },
    /// Keyframes at key times (evenly spaced when the app gives none).
    Keyframes { values: Vec<Val>, times: Vec<f64>, discrete: bool },
}

#[derive(Clone, Debug)]
pub struct Anim {
    pub key: String,
    pub prop: Prop,
    pub curve: Curve,
    /// When it starts (virtual seconds).
    pub begin: f64,
    pub duration: f64,
    pub timing: Timing,
    /// Iterations (1 when the app gives none; infinity repeats forever).
    pub repeat: f64,
    pub autoreverses: bool,
    pub fill_forwards: bool,
    pub fill_backwards: bool,
    pub removed_on_completion: bool,
    pub additive: bool,
    /// The CAAnimation object (retained while it's on the layer), if any.
    pub object: u64,
    /// UIView animations: the block's completion runs when they're done (see `uiview::animate`).
    pub delegate: u64,
}

impl Anim {
    /// How long it runs, every iteration and reversal included.
    pub fn active(&self) -> f64 {
        self.duration * self.repeat.max(1.0) * if self.autoreverses { 2.0 } else { 1.0 }
    }

    pub fn end(&self) -> f64 {
        self.begin + self.active()
    }

    /// Progress between the values at `now` (eased), or None when it has no effect.
    fn progress(&self, now: f64) -> Option<f64> {
        let t = now - self.begin;
        if t < 0.0 {
            return self.fill_backwards.then_some(0.0);
        }
        let d = self.duration.max(1e-9);
        let (local, finished) = if t >= self.active() {
            if !self.fill_forwards {
                return None;
            }
            // Where it ends: after an autoreversed iteration, back at the start.
            (if self.autoreverses { 0.0 } else { d }, true)
        } else {
            let cycle = if self.autoreverses { 2.0 * d } else { d };
            let c = t % cycle;
            (if self.autoreverses && c > d { 2.0 * d - c } else { c }, false)
        };
        Some(match self.timing {
            Timing::Linear => local / d,
            Timing::Bezier(p) => bezier(p, (local / d).clamp(0.0, 1.0)),
            Timing::Spring { mass, stiffness, damping, velocity } => {
                if finished && !self.autoreverses {
                    1.0
                } else {
                    spring(mass, stiffness, damping, velocity, local)
                }
            }
        })
    }

    /// The value at `now`, given the property's model value.
    fn value(&self, now: f64, model: Val) -> Option<Val> {
        let p = self.progress(now)?;
        let v = match &self.curve {
            Curve::Basic { from, to, by } => {
                let (from, to) = match (from, to, by) {
                    (Some(f), Some(t), _) => (*f, *t),
                    (Some(f), None, Some(b)) => (*f, add(*f, *b)),
                    (None, Some(t), Some(b)) => (sub(*t, *b), *t),
                    (Some(f), None, None) => (*f, model),
                    (None, Some(t), None) => (model, *t),
                    (None, None, Some(b)) => (model, add(model, *b)),
                    (None, None, None) => return None,
                };
                lerp(from, to, p)
            }
            Curve::Keyframes { values, times, discrete } => keyframe(values, times, *discrete, p)?,
        };
        Some(if self.additive { add(model, v) } else { v })
    }
}

/// A UIView animation block being run: what property changes inside it get.
#[derive(Clone, Copy, Debug)]
struct Block {
    begin: f64,
    duration: f64,
    timing: Timing,
    repeat: f64,
    autoreverses: bool,
    /// `performWithoutAnimation:`: changes don't animate.
    off: bool,
}

#[derive(Default)]
pub struct State {
    /// Layer → its animations, in the order they were added.
    pub layers: BTreeMap<u64, Vec<Anim>>,
    /// UIView animation blocks running now, innermost last.
    blocks: Vec<Block>,
    /// Animations that ended, waiting for their delegates (run-loop callouts).
    stopped: Vec<(u64, u64, bool)>,
}

// ---- timing ----

/// A cubic Bézier timing curve at x: the y where the curve's x is `x`.
fn bezier([x1, y1, x2, y2]: [f64; 4], x: f64) -> f64 {
    let at = |a: f64, b: f64, t: f64| 3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t;
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // Bisection: x(t) increases with t on [0, 1] for valid timing curves.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if at(x1, x2, mid) < x { lo = mid } else { hi = mid }
    }
    at(y1, y2, (lo + hi) / 2.0)
}

/// A damped spring from 0 to 1 at `t` seconds (the closed-form solution).
pub fn spring(mass: f64, stiffness: f64, damping: f64, velocity: f64, t: f64) -> f64 {
    let (m, k) = (mass.max(1e-6), stiffness.max(1e-6));
    let w0 = (k / m).sqrt();
    let zeta = damping / (2.0 * (k * m).sqrt());
    // Displacement from the target, starting at -1 with the initial velocity toward it.
    let v0 = velocity;
    let x = if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        (-zeta * w0 * t).exp() * ((zeta * w0 - v0) / wd * (wd * t).sin() + (wd * t).cos())
    } else if zeta == 1.0 {
        (-w0 * t).exp() * (1.0 + (w0 - v0) * t)
    } else {
        let s = (zeta * zeta - 1.0).sqrt();
        let (r1, r2) = (-w0 * (zeta - s), -w0 * (zeta + s));
        let b = (v0 + r1) / (r1 - r2);
        (1.0 - b) * (r1 * t).exp() + b * (r2 * t).exp()
    };
    1.0 - x
}

/// How long a spring takes to settle (CASpringAnimation's `settlingDuration`):
/// until it stays within 0.1% of its target.
pub fn settling_duration(mass: f64, stiffness: f64, damping: f64, velocity: f64) -> f64 {
    let step = 1.0 / 240.0;
    let mut last_out = 0.0;
    let mut t = 0.0;
    while t < 30.0 {
        if (spring(mass, stiffness, damping, velocity, t) - 1.0).abs() > 0.001 {
            last_out = t;
        }
        t += step;
    }
    (last_out + step).min(30.0)
}

/// UIView's spring (`usingSpringWithDamping:initialSpringVelocity:`) as a
/// spring that does its motion in `duration` (its response) with damping ratio `ratio`.
pub fn view_spring(duration: f64, ratio: f64, velocity: f64) -> Timing {
    let response = duration.max(1e-3);
    let stiffness = (2.0 * std::f64::consts::PI / response).powi(2);
    let damping = 4.0 * std::f64::consts::PI * ratio.clamp(0.0, 1.0) / response;
    Timing::Spring { mass: 1.0, stiffness, damping, velocity }
}

// ---- values ----

fn zip<const N: usize>(a: [f64; N], b: [f64; N], f: impl Fn(f64, f64) -> f64) -> [f64; N] {
    std::array::from_fn(|i| f(a[i], b[i]))
}

fn combine(a: Val, b: Val, f: impl Fn(f64, f64) -> f64 + Copy) -> Val {
    match (a, b) {
        (Val::N(x), Val::N(y)) => Val::N(f(x, y)),
        (Val::P(x), Val::P(y)) => Val::P(zip(x, y, f)),
        (Val::R(x), Val::R(y)) => Val::R(zip(x, y, f)),
        (Val::C(x), Val::C(y)) => Val::C(zip(x, y, f)),
        (Val::M(x), Val::M(y)) => Val::M(zip(x, y, f)),
        (_, b) => b,
    }
}

fn add(a: Val, b: Val) -> Val {
    match (a, b) {
        // Transforms add by concatenation.
        (Val::M(x), Val::M(y)) => Val::M(concat3d(&y, &x)),
        _ => combine(a, b, |x, y| x + y),
    }
}

fn sub(a: Val, b: Val) -> Val {
    combine(a, b, |x, y| x - y)
}

fn lerp(a: Val, b: Val, t: f64) -> Val {
    match (a, b) {
        (Val::M(x), Val::M(y)) => Val::M(lerp_transform(&x, &y, t)),
        _ => combine(a, b, |x, y| x + (y - x) * t),
    }
}

fn keyframe(values: &[Val], times: &[f64], discrete: bool, p: f64) -> Option<Val> {
    let n = values.len();
    if n == 0 {
        return None;
    }
    if n == 1 {
        return Some(values[0]);
    }
    let time = |i: usize| times.get(i).copied().unwrap_or(i as f64 / (n - 1) as f64);
    let mut i = 0;
    while i + 2 < n && p >= time(i + 1) {
        i += 1;
    }
    if discrete {
        return Some(if p >= time(i + 1) { values[i + 1] } else { values[i] });
    }
    let (t0, t1) = (time(i), time(i + 1));
    let f = if t1 > t0 { ((p - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
    Some(lerp(values[i], values[i + 1], f))
}

pub fn identity3d() -> [f64; 16] {
    let mut m = [0.0; 16];
    for i in 0..4 {
        m[i * 5] = 1.0;
    }
    m
}

/// `a` then `b` (row vectors, as CATransform3DConcat).
pub fn concat3d(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut r = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    r
}

/// Interpolate transforms as Core Animation does: by their parts (scale,
/// rotation, translation), so a rotation turns rather than shrinking through
/// the middle. The 2D part is decomposed; the rest interpolates linearly.
fn lerp_transform(a: &[f64; 16], b: &[f64; 16], t: f64) -> [f64; 16] {
    let parts = |m: &[f64; 16]| {
        let (sx, sy) = ((m[0] * m[0] + m[1] * m[1]).sqrt(), (m[4] * m[4] + m[5] * m[5]).sqrt());
        let det = m[0] * m[5] - m[1] * m[4];
        let sy = if det < 0.0 { -sy } else { sy };
        (sx, sy, m[1].atan2(m[0]), m[12], m[13])
    };
    let (asx, asy, ar, atx, aty) = parts(a);
    let (bsx, bsy, mut br, btx, bty) = parts(b);
    // The short way round.
    if (br - ar).abs() > std::f64::consts::PI {
        br -= (br - ar).signum() * 2.0 * std::f64::consts::PI;
    }
    let l = |x: f64, y: f64| x + (y - x) * t;
    let (sx, sy, r, tx, ty) = (l(asx, bsx), l(asy, bsy), l(ar, br), l(atx, btx), l(aty, bty));
    let mut m = zip(*a, *b, |x, y| x + (y - x) * t);
    m[0] = sx * r.cos();
    m[1] = sx * r.sin();
    m[4] = -sy * r.sin();
    m[5] = sy * r.cos();
    m[12] = tx;
    m[13] = ty;
    m
}

// ---- reading the model, applying the presentation ----

/// The model value of `prop` on layer `l` (backed by view `v`, if any).
fn model(emu: &mut Emu, l: u64, v: Option<u64>, prop: Prop) -> Val {
    let ly = crate::calayer::layer(emu, l);
    let (position, bounds, opacity, background) = match v {
        Some(v) => {
            let vw = crate::uiview::view(emu, v);
            (vw.center, vw.bounds, vw.alpha, vw.background)
        }
        None => (ly.position, ly.bounds, ly.opacity as f64, ly.background),
    };
    let m = &ly.transform;
    match prop {
        Prop::Position => Val::P(position),
        Prop::PositionX => Val::N(position[0]),
        Prop::PositionY => Val::N(position[1]),
        Prop::Bounds => Val::R(bounds),
        Prop::BoundsSize => Val::P([bounds[2], bounds[3]]),
        Prop::BoundsOrigin => Val::P([bounds[0], bounds[1]]),
        Prop::Opacity => Val::N(opacity),
        Prop::Transform => Val::M(*m),
        Prop::Scale | Prop::ScaleX => Val::N((m[0] * m[0] + m[1] * m[1]).sqrt()),
        Prop::ScaleY => Val::N((m[4] * m[4] + m[5] * m[5]).sqrt()),
        Prop::Rotation => Val::N(m[1].atan2(m[0])),
        Prop::TranslationX => Val::N(m[12]),
        Prop::TranslationY => Val::N(m[13]),
        Prop::Background => Val::C(emu.ui.views.colors.get(&background).copied().unwrap_or([0.0; 4])),
        Prop::CornerRadius => Val::N(ly.corner_radius),
        Prop::BorderWidth => Val::N(ly.border_width),
        Prop::ShadowOpacity => Val::N(ly.shadow_opacity as f64),
    }
}

/// What a layer (and its view) show at a moment.
pub struct Shown {
    pub position: [f64; 2],
    pub bounds: [f64; 4],
    pub opacity: f64,
    /// The background colour, when an animation sets it.
    pub background: Option<[f64; 4]>,
}

/// Apply `layer`'s running animations to the values in `shown` and `ly`.
fn apply(emu: &mut Emu, l: u64, v: Option<u64>, shown: &mut Shown, ly: &mut crate::calayer::Layer) {
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    prune(emu, l, now);
    let Some(anims) = emu.anim.layers.get(&l).cloned() else { return };
    for a in &anims {
        let base = model(emu, l, v, a.prop);
        let Some(val) = a.value(now, base) else { continue };
        let num = |v: Val| if let Val::N(n) = v { Some(n) } else { None };
        match (a.prop, val) {
            (Prop::Position, Val::P(p)) => shown.position = p,
            (Prop::PositionX, Val::N(x)) => shown.position[0] = x,
            (Prop::PositionY, Val::N(y)) => shown.position[1] = y,
            (Prop::Bounds, Val::R(r)) => shown.bounds = r,
            (Prop::BoundsSize, Val::P(s)) => {
                shown.bounds[2] = s[0];
                shown.bounds[3] = s[1];
            }
            (Prop::BoundsOrigin, Val::P(o)) => {
                shown.bounds[0] = o[0];
                shown.bounds[1] = o[1];
            }
            (Prop::Opacity, Val::N(o)) => shown.opacity = o,
            (Prop::Transform, Val::M(m)) => ly.transform = m,
            (Prop::Scale, _) | (Prop::ScaleX, _) | (Prop::ScaleY, _) | (Prop::Rotation, _) | (Prop::TranslationX, _) | (Prop::TranslationY, _) => {
                let Some(n) = num(val) else { continue };
                ly.transform = set_part(&ly.transform, a.prop, n);
            }
            (Prop::Background, Val::C(c)) => shown.background = Some(c),
            (Prop::CornerRadius, Val::N(r)) => ly.corner_radius = r,
            (Prop::BorderWidth, Val::N(w)) => ly.border_width = w,
            (Prop::ShadowOpacity, Val::N(o)) => ly.shadow_opacity = o as f32,
            _ => {}
        }
    }
}

/// A transform with one part (scale, rotation, translation) set, as the
/// `transform.*` key paths do.
fn set_part(m: &[f64; 16], prop: Prop, n: f64) -> [f64; 16] {
    let (sx, sy) = ((m[0] * m[0] + m[1] * m[1]).sqrt(), (m[4] * m[4] + m[5] * m[5]).sqrt());
    let r = m[1].atan2(m[0]);
    let (mut sx, mut sy, mut r, mut tx, mut ty) = (sx, sy, r, m[12], m[13]);
    match prop {
        Prop::Scale => (sx, sy) = (n, n),
        Prop::ScaleX => sx = n,
        Prop::ScaleY => sy = n,
        Prop::Rotation => r = n,
        Prop::TranslationX => tx = n,
        Prop::TranslationY => ty = n,
        _ => {}
    }
    let mut out = *m;
    out[0] = sx * r.cos();
    out[1] = sx * r.sin();
    out[4] = -sy * r.sin();
    out[5] = sy * r.cos();
    out[12] = tx;
    out[13] = ty;
    out
}

/// What view `v` shows now: its center, bounds, alpha and layer, animated.
pub fn present_view(emu: &mut Emu, v: u64, vw: &mut crate::uiview::View, ly: &mut crate::calayer::Layer) -> Option<[f64; 4]> {
    if vw.layer == 0 || !emu.anim.layers.contains_key(&vw.layer) {
        return None;
    }
    let mut shown = Shown { position: vw.center, bounds: vw.bounds, opacity: vw.alpha, background: None };
    apply(emu, vw.layer, Some(v), &mut shown, ly);
    vw.center = shown.position;
    vw.bounds = shown.bounds;
    vw.alpha = shown.opacity;
    shown.background
}

/// What a standalone layer shows now.
pub fn present_layer(emu: &mut Emu, l: u64, ly: &mut crate::calayer::Layer) -> Option<[f64; 4]> {
    if !emu.anim.layers.contains_key(&l) {
        return None;
    }
    let mut shown = Shown { position: ly.position, bounds: ly.bounds, opacity: ly.opacity as f64, background: None };
    apply(emu, l, None, &mut shown, ly);
    ly.position = shown.position;
    ly.bounds = shown.bounds;
    ly.opacity = shown.opacity as f32;
    shown.background
}

/// Whether anything is animating at `now` (a live host keeps drawing frames).
pub fn animating(emu: &Emu) -> bool {
    if emu.anim.layers.is_empty() {
        return false;
    }
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    // Until a frame after the end, so the last frame shows where it ended.
    emu.anim.layers.values().flatten().any(|a| a.begin <= now + 1.0 / 30.0 && now < a.end() + 1.0 / 30.0)
}

// ---- adding and removing ----

/// Put `a` on layer `l`, replacing any animation with the same key. Can't
/// fail, so property setters can call it; replaced animations' delegates hear
/// about it at the next commit ([`commit`]).
pub fn push(emu: &mut Emu, l: u64, a: Anim) {
    let list = emu.anim.layers.entry(l).or_default();
    if !a.key.is_empty() {
        let mut gone = Vec::new();
        list.retain(|x| {
            let same = x.key == a.key;
            if same && x.object != 0 {
                gone.push((x.object, x.delegate, false));
            }
            !same
        });
        emu.anim.stopped.extend(gone);
    }
    list.push(a);
    crate::host::changed(emu);
}

/// Add a CAAnimation's animation: on the layer, retained, with its end scheduled.
fn add_anim(emu: &mut Emu, l: u64, a: Anim) -> Result<()> {
    if a.object != 0 {
        send(emu, a.object, "retain", &[])?;
    }
    let (object, end) = (a.object, a.end());
    let finite = a.repeat.is_finite();
    push(emu, l, a);
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    if finite && object != 0 {
        let info = emu.alloc(16)?;
        emu.write_u64(info, l)?;
        emu.write_u64(info + 8, object)?;
        crate::uikit::after_on_main(emu, (end - now).max(0.0), end_fire, info)?;
    }
    commit(emu)
}

/// Core Animation's commit (the run loop's before-waiting pass): delegates of
/// animations that stopped hear about it.
pub fn commit(emu: &mut Emu) -> Result<()> {
    if emu.anim.stopped.is_empty() {
        return Ok(());
    }
    stopped_fire(emu)
}

/// Drop animations that ended and leave nothing behind (UIView's, which have no delegate).
fn prune(emu: &mut Emu, l: u64, now: f64) {
    if let Some(list) = emu.anim.layers.get_mut(&l) {
        list.retain(|a| a.object != 0 || a.fill_forwards || !a.removed_on_completion || now < a.end());
        if list.is_empty() {
            emu.anim.layers.remove(&l);
        }
    }
}

/// The run-loop callout at an animation's end: remove it, tell its delegate.
fn end_fire(emu: &mut Emu) -> Result<()> {
    let info = emu.arg(1);
    let (l, object) = (emu.read_u64(info)?, emu.read_u64(info + 8)?);
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    let Some(list) = emu.anim.layers.get_mut(&l) else { return Ok(()) };
    // The animation still on the layer (not replaced or removed since), ended by now.
    let Some(i) = list.iter().position(|a| a.object == object && a.end() <= now + 1e-9) else { return Ok(()) };
    let a = list[i].clone();
    if a.removed_on_completion || !a.fill_forwards {
        list.remove(i);
        if list.is_empty() {
            emu.anim.layers.remove(&l);
        }
    }
    crate::host::changed(emu);
    stopped(emu, a.object, a.delegate, true)
}

/// An animation stopped: its delegate hears `animationDidStop:finished:`; the object is released.
fn stopped(emu: &mut Emu, object: u64, delegate: u64, finished: bool) -> Result<()> {
    if object == 0 {
        return Ok(());
    }
    let d = if delegate != 0 { delegate } else { objc::bag_get(emu, object, "delegate").unwrap_or(0) };
    if d != 0 {
        let sel = objc::intern(emu, "animationDidStop:finished:")?;
        if objc::responds(emu, d, sel)? {
            send(emu, d, "animationDidStop:finished:", &[object, finished as u64])?;
        }
    }
    send(emu, object, "release", &[])?;
    Ok(())
}

/// Remove `l`'s animations with `key` (all when None), telling their delegates they didn't finish.
pub fn remove(emu: &mut Emu, l: u64, key: Option<&str>) -> Result<()> {
    let Some(list) = emu.anim.layers.get_mut(&l) else { return Ok(()) };
    let mut gone = Vec::new();
    list.retain(|a| {
        let hit = key.is_none_or(|k| a.key == k);
        if hit {
            gone.push((a.object, a.delegate));
        }
        !hit
    });
    if list.is_empty() {
        emu.anim.layers.remove(&l);
    }
    crate::host::changed(emu);
    // Delegates hear it at the next commit, as Core Animation tells them.
    emu.anim.stopped.extend(gone.into_iter().filter(|g| g.0 != 0).map(|(o, d)| (o, d, false)));
    Ok(())
}

fn stopped_fire(emu: &mut Emu) -> Result<()> {
    for (o, d, finished) in std::mem::take(&mut emu.anim.stopped) {
        stopped(emu, o, d, finished)?;
    }
    Ok(())
}

// ---- reading CAAnimation objects ----

fn bag_f64(emu: &Emu, o: u64, key: &str) -> Option<f64> {
    emu.objc.bag.get(&(o, key.to_string())).map(|&(_, q)| f64::from_bits(q as u64))
}

fn bag_bool(emu: &Emu, o: u64, key: &str) -> Option<bool> {
    emu.objc.bag.get(&(o, key.to_string())).map(|&(x, _)| x & 0xff != 0)
}

/// An animated value from an object: NSNumber, NSValue (point, size, rect,
/// transform) or a colour.
pub fn read_val(emu: &mut Emu, o: u64) -> Result<Option<Val>> {
    if o == 0 {
        return Ok(None);
    }
    if let Some(c) = emu.ui.views.colors.get(&o) {
        return Ok(Some(Val::C(*c)));
    }
    if let Some(v) = crate::nsvalue::animatable(emu, o)? {
        return Ok(Some(v));
    }
    let num = objc::host_class(emu, "NSNumber")?;
    if objc::is_kind_of(emu, o, num)? {
        let (_, d) = foundation::number_value(emu, o)?;
        return Ok(Some(Val::N(d)));
    }
    Ok(None)
}

fn read_timing(emu: &mut Emu, o: u64) -> Timing {
    let tf = objc::bag_get(emu, o, "timingFunction").unwrap_or(0);
    if tf == 0 {
        return Timing::Linear;
    }
    timing_of(emu, tf)
}

/// A CAMediaTimingFunction's curve.
fn timing_of(emu: &Emu, tf: u64) -> Timing {
    match emu.objc.bag.get(&(tf, "controlPoints".to_string())) {
        Some(_) => {
            let p: [f64; 4] = std::array::from_fn(|i| bag_f64(emu, tf, &format!("cp{i}")).unwrap_or(0.0));
            Timing::Bezier(p)
        }
        None => Timing::Linear,
    }
}

/// Read a CAAnimation (and a group's children) into animations starting at `begin`.
fn read_anim(emu: &mut Emu, o: u64, key: &str, now: f64, out: &mut Vec<Anim>, group: Option<&Anim>) -> Result<()> {
    let cls_name = |emu: &mut Emu, n: &str| -> Result<bool> {
        let c = objc::host_class(emu, n)?;
        objc::is_kind_of(emu, o, c)
    };
    // Core Animation's defaults: a 0.25 s duration when none is set.
    let duration = bag_f64(emu, o, "duration").filter(|d| *d > 0.0).unwrap_or(if group.is_some() { group.unwrap().duration } else { 0.25 });
    let begin_time = bag_f64(emu, o, "beginTime").unwrap_or(0.0);
    let fill = match objc::bag_get(emu, o, "fillMode") {
        Some(f) if f != 0 => foundation::nsstring(emu, f).unwrap_or_default(),
        _ => "removed".into(),
    };
    let repeat = match bag_f64(emu, o, "repeatCount") {
        Some(r) if r > 0.0 => r,
        _ => match bag_f64(emu, o, "repeatDuration") {
            Some(d) if d > 0.0 => d / duration,
            _ => 1.0,
        },
    };
    let begin = match group {
        Some(g) => g.begin + begin_time,
        // beginTime is in the layer's time (CACurrentMediaTime): 0 means now.
        None if begin_time > 0.0 => begin_time,
        None => now,
    };
    let base = Anim {
        key: key.to_string(),
        prop: Prop::Opacity,
        curve: Curve::Basic { from: None, to: None, by: None },
        begin,
        duration,
        timing: read_timing(emu, o),
        repeat,
        autoreverses: bag_bool(emu, o, "autoreverses").unwrap_or(false),
        fill_forwards: fill == "forwards" || fill == "both",
        fill_backwards: fill == "backwards" || fill == "both",
        removed_on_completion: bag_bool(emu, o, "removedOnCompletion").unwrap_or(true),
        additive: bag_bool(emu, o, "additive").unwrap_or(false),
        object: o,
        delegate: 0,
    };
    if cls_name(emu, "CAAnimationGroup")? {
        let children = objc::bag_get(emu, o, "animations").unwrap_or(0);
        let n = if children != 0 { send(emu, children, "count", &[])? } else { 0 };
        for i in 0..n {
            let c = send(emu, children, "objectAtIndex:", &[i])?;
            let mut sub = Vec::new();
            read_anim(emu, c, key, now, &mut sub, Some(&base))?;
            for mut s in sub {
                // The group owns the children: they end with it.
                s.object = 0;
                s.fill_forwards |= base.fill_forwards;
                s.removed_on_completion = base.removed_on_completion;
                out.push(s);
            }
        }
        // The group itself: a no-op animation carrying its timing, for its end and delegate.
        out.push(Anim { curve: Curve::Keyframes { values: vec![], times: vec![], discrete: false }, ..base });
        return Ok(());
    }
    let key_path = objc::bag_get(emu, o, "keyPath").map(|k| foundation::nsstring(emu, k).unwrap_or_default()).unwrap_or_default();
    let Some(prop) = Prop::parse(&key_path) else {
        // Not an animatable property pomme draws: it still runs and ends, for its delegate.
        out.push(Anim { curve: Curve::Keyframes { values: vec![], times: vec![], discrete: false }, ..base });
        return Ok(());
    };
    let mut a = Anim { prop, ..base };
    if cls_name(emu, "CAKeyframeAnimation")? {
        let arr = objc::bag_get(emu, o, "values").unwrap_or(0);
        let n = if arr != 0 { send(emu, arr, "count", &[])? } else { 0 };
        let mut values = Vec::new();
        for i in 0..n {
            let v = send(emu, arr, "objectAtIndex:", &[i])?;
            if let Some(v) = read_val(emu, v)? {
                values.push(v);
            }
        }
        let kt = objc::bag_get(emu, o, "keyTimes").unwrap_or(0);
        let m = if kt != 0 { send(emu, kt, "count", &[])? } else { 0 };
        let mut times = Vec::new();
        for i in 0..m {
            let v = send(emu, kt, "objectAtIndex:", &[i])?;
            if let Some(Val::N(t)) = read_val(emu, v)? {
                times.push(t);
            }
        }
        let mode = objc::bag_get(emu, o, "calculationMode").map(|m| foundation::nsstring(emu, m).unwrap_or_default()).unwrap_or_default();
        a.curve = Curve::Keyframes { values, times, discrete: mode == "discrete" };
    } else {
        let from = objc::bag_get(emu, o, "fromValue").unwrap_or(0);
        let to = objc::bag_get(emu, o, "toValue").unwrap_or(0);
        let by = objc::bag_get(emu, o, "byValue").unwrap_or(0);
        a.curve = Curve::Basic { from: read_val(emu, from)?, to: read_val(emu, to)?, by: read_val(emu, by)? };
    }
    if cls_name(emu, "CASpringAnimation")? {
        a.timing = Timing::Spring {
            mass: bag_f64(emu, o, "mass").unwrap_or(1.0),
            stiffness: bag_f64(emu, o, "stiffness").unwrap_or(100.0),
            damping: bag_f64(emu, o, "damping").unwrap_or(10.0),
            velocity: bag_f64(emu, o, "initialVelocity").unwrap_or(0.0),
        };
    }
    out.push(a);
    Ok(())
}

// ---- system transitions ----

/// A critically (or nearly) damped spring as UIKit's transitions use, from its
/// natural frequency `omega` (rad/s), damping ratio `zeta` and initial
/// velocity (in distances per second); and how long it takes to settle.
pub fn transition_spring((omega, zeta, velocity): (f64, f64, f64)) -> (Timing, f64) {
    let t = Timing::Spring { mass: 1.0, stiffness: omega * omega, damping: 2.0 * zeta * omega, velocity };
    let Timing::Spring { mass, stiffness, damping, velocity } = t else { unreachable!() };
    (t, settling_duration(mass, stiffness, damping, velocity))
}

/// Animate `prop` on layer `l` from `from` to `to` starting now, holding the
/// end value until the transition finishes (system transitions: navigation,
/// presentation).
pub fn transition(emu: &mut Emu, l: u64, prop: Prop, from: Val, to: Val, timing: Timing, duration: f64) {
    let begin = crate::threads::now_ns(emu) as f64 / 1e9;
    push(
        emu,
        l,
        Anim {
            key: "pomme.transition".into(),
            prop,
            curve: Curve::Basic { from: Some(from), to: Some(to), by: None },
            begin,
            duration,
            timing,
            repeat: 1.0,
            autoreverses: false,
            fill_forwards: true,
            fill_backwards: false,
            removed_on_completion: true,
            additive: false,
            object: 0,
            delegate: 0,
        },
    );
}

/// The transition's animations are done: take them off `l` (the model shows from now).
pub fn end_transition(emu: &mut Emu, l: u64) {
    if let Some(list) = emu.anim.layers.get_mut(&l) {
        list.retain(|a| a.key != "pomme.transition");
        if list.is_empty() {
            emu.anim.layers.remove(&l);
        }
    }
    crate::host::changed(emu);
}

// ---- UIView animation blocks ----

/// UIView animation options: the curve (bits 16–19), repeat and autoreverse.
pub fn block_timing(options: u64) -> Timing {
    match (options >> 16) & 0xf {
        1 => named_timing("easeIn"),
        2 => named_timing("easeOut"),
        3 => Timing::Linear,
        _ => named_timing("easeInEaseOut"),
    }
}

/// Run `f` (a UIView animation block) with changes animating like this.
pub fn in_block(emu: &mut Emu, delay: f64, duration: f64, timing: Timing, options: u64, f: impl FnOnce(&mut Emu) -> Result<()>) -> Result<()> {
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    let repeat = if options & (1 << 3) != 0 { f64::INFINITY } else { 1.0 };
    emu.anim.blocks.push(Block { begin: now + delay, duration, timing, repeat, autoreverses: options & (1 << 4) != 0, off: duration <= 0.0 });
    let r = f(emu);
    emu.anim.blocks.pop();
    r
}

/// Run `f` with changes not animating (`performWithoutAnimation:`).
pub fn without(emu: &mut Emu, f: impl FnOnce(&mut Emu) -> Result<()>) -> Result<()> {
    emu.anim.blocks.push(Block { begin: 0.0, duration: 0.0, timing: Timing::Linear, repeat: 1.0, autoreverses: false, off: true });
    let r = f(emu);
    emu.anim.blocks.pop();
    r
}

/// Inside `+addKeyframeWithRelativeStartTime:relativeDuration:animations:`: a part of the enclosing block.
pub fn in_keyframe(emu: &mut Emu, start: f64, length: f64, f: impl FnOnce(&mut Emu) -> Result<()>) -> Result<()> {
    let Some(&outer) = emu.anim.blocks.last() else { return f(emu) };
    emu.anim.blocks.push(Block { begin: outer.begin + start * outer.duration, duration: length * outer.duration, timing: Timing::Linear, ..outer });
    let r = f(emu);
    emu.anim.blocks.pop();
    r
}

/// Before a view property changes: what's on screen now, if the change will animate.
pub fn before(emu: &mut Emu, v: u64, prop: Prop) -> Option<Val> {
    let b = *emu.anim.blocks.last()?;
    if b.off {
        return None;
    }
    let vw = crate::uiview::view(emu, v);
    // A view that isn't on screen has nothing to animate from (its layer was never committed).
    if vw.layer == 0 || vw.window == 0 {
        return None;
    }
    let mut shown_vw = vw.clone();
    let mut ly = crate::calayer::layer(emu, vw.layer);
    let bg = present_view(emu, v, &mut shown_vw, &mut ly);
    Some(match prop {
        Prop::Position => Val::P(shown_vw.center),
        Prop::Bounds => Val::R(shown_vw.bounds),
        Prop::Opacity => Val::N(shown_vw.alpha),
        Prop::Transform => Val::M(ly.transform),
        Prop::Background => Val::C(bg.unwrap_or_else(|| emu.ui.views.colors.get(&vw.background).copied().unwrap_or([0.0; 4]))),
        other => model(emu, vw.layer, Some(v), other),
    })
}

/// After a view property changed: animate from what was on screen (`old`, from [`before`]).
pub fn after(emu: &mut Emu, v: u64, prop: Prop, old: Option<Val>) {
    let (Some(old), Some(&b)) = (old, emu.anim.blocks.last()) else { return };
    let l = crate::uiview::view(emu, v).layer;
    let new = model(emu, l, Some(v), prop);
    if new == old {
        return;
    }
    let key = match prop {
        Prop::Position => "position",
        Prop::Bounds => "bounds",
        Prop::Opacity => "opacity",
        Prop::Transform => "transform",
        Prop::Background => "backgroundColor",
        _ => "",
    };
    let a = Anim {
        key: key.to_string(),
        prop,
        curve: Curve::Basic { from: Some(old), to: Some(new), by: None },
        begin: b.begin,
        duration: b.duration,
        timing: b.timing,
        repeat: b.repeat,
        autoreverses: b.autoreverses,
        // Before a delay passes, the old value shows (as UIKit's animations fill backwards).
        fill_forwards: false,
        fill_backwards: true,
        removed_on_completion: true,
        additive: false,
        object: 0,
        delegate: 0,
    };
    push(emu, l, a);
}

// ---- Objective-C ----

/// `-[CALayer addAnimation:forKey:]`: Core Animation copies the animation; so does pomme, reading it now.
fn add_animation(emu: &mut Emu) -> Result<()> {
    let (l, o, key) = (emu.arg(0), emu.arg(2), emu.arg(3));
    if o == 0 {
        return Ok(());
    }
    let key = if key != 0 { foundation::nsstring(emu, key)? } else { String::new() };
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    let mut anims = Vec::new();
    read_anim(emu, o, &key, now, &mut anims, None)?;
    // Children of a group share its key; only the last (the group) replaces by key.
    let n = anims.len();
    for (i, mut a) in anims.into_iter().enumerate() {
        if i + 1 < n {
            a.key = format!("{key}\u{0}{i}");
        }
        add_anim(emu, l, a)?;
    }
    if let Some(d) = objc::bag_get(emu, o, "delegate").filter(|&d| d != 0) {
        let sel = objc::intern(emu, "animationDidStart:")?;
        if objc::responds(emu, d, sel)? {
            send(emu, d, "animationDidStart:", &[o])?;
        }
    }
    Ok(())
}

fn animation_keys(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let keys: Vec<String> = emu.anim.layers.get(&l).map(|v| v.iter().filter(|a| !a.key.is_empty() && !a.key.contains('\u{0}')).map(|a| a.key.clone()).collect()).unwrap_or_default();
    if keys.is_empty() {
        emu.ret(0);
        return Ok(());
    }
    let mut items = Vec::new();
    for k in keys {
        items.push(foundation::new_nsstring(emu, &k)?);
    }
    let arr = foundation::new_array_of(emu, &items)?;
    emu.ret(arr);
    Ok(())
}

fn animation_for_key(emu: &mut Emu) -> Result<()> {
    let (l, key) = (emu.arg(0), emu.arg(2));
    let key = if key != 0 { foundation::nsstring(emu, key)? } else { String::new() };
    let o = emu.anim.layers.get(&l).and_then(|v| v.iter().find(|a| a.key == key)).map_or(0, |a| a.object);
    emu.ret(o);
    Ok(())
}

/// `+[CAMediaTimingFunction functionWithName:]` / `functionWithControlPoints::::`.
fn timing_function(emu: &mut Emu, points: [f64; 4]) -> Result<u64> {
    let cls = objc::host_class(emu, "CAMediaTimingFunction")?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.objc.bag.insert((o, "controlPoints".into()), (1, 0));
    for (i, p) in points.iter().enumerate() {
        emu.objc.bag.insert((o, format!("cp{i}")), (0, p.to_bits() as u128));
    }
    Ok(o)
}

/// `+[CAAnimation animation]` / `+[CAPropertyAnimation animationWithKeyPath:]`: an instance of the receiving class.
fn new_animation(emu: &mut Emu, key_path: Option<u64>) -> Result<()> {
    let cls = emu.arg(0);
    let o = objc::create_instance(emu, cls, 0)?;
    if let Some(k) = key_path.filter(|&k| k != 0) {
        let k = send(emu, k, "copy", &[])?;
        objc::bag_set(emu, o, "keyPath", k);
    }
    let o = send(emu, o, "autorelease", &[])?;
    emu.ret(o);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("CAAnimation", true, "animation", |e| new_animation(e, None)),
    ("CAPropertyAnimation", true, "animationWithKeyPath:", |e| {
        let k = e.arg(2);
        new_animation(e, Some(k))
    }),
    ("CALayer", false, "addAnimation:forKey:", add_animation),
    ("CALayer", false, "removeAnimationForKey:", |e| {
        let (l, k) = (e.arg(0), e.arg(2));
        let k = if k != 0 { foundation::nsstring(e, k)? } else { String::new() };
        remove(e, l, Some(&k))
    }),
    ("CALayer", false, "removeAllAnimations", |e| {
        let l = e.arg(0);
        remove(e, l, None)
    }),
    ("CALayer", false, "animationKeys", animation_keys),
    ("CALayer", false, "animationForKey:", animation_for_key),
    ("CAMediaTimingFunction", true, "functionWithName:", |e| {
        let name = e.arg(2);
        let name = if name != 0 { foundation::nsstring(e, name)? } else { String::new() };
        let p = match named_timing(&name) {
            Timing::Bezier(p) => p,
            _ => [0.0, 0.0, 1.0, 1.0],
        };
        let o = timing_function(e, p)?;
        e.ret(o);
        Ok(())
    }),
    ("CAMediaTimingFunction", true, "functionWithControlPoints::::", |e| {
        let p = std::array::from_fn(|i| f64::from_bits(e.cpu.q[i] as u64) as f32 as f64);
        let o = timing_function(e, p)?;
        e.ret(o);
        Ok(())
    }),
    ("CAMediaTimingFunction", false, "initWithControlPoints::::", |e| {
        let o = e.arg(0);
        e.objc.bag.insert((o, "controlPoints".into()), (1, 0));
        for i in 0..4 {
            let p = f64::from_bits(e.cpu.q[i] as u64) as f32 as f64;
            e.objc.bag.insert((o, format!("cp{i}")), (0, p.to_bits() as u128));
        }
        e.ret(o);
        Ok(())
    }),
    ("CASpringAnimation", false, "settlingDuration", |e| {
        let o = e.arg(0);
        let d = settling_duration(
            bag_f64(e, o, "mass").unwrap_or(1.0),
            bag_f64(e, o, "stiffness").unwrap_or(100.0),
            bag_f64(e, o, "damping").unwrap_or(10.0),
            bag_f64(e, o, "initialVelocity").unwrap_or(0.0),
        );
        e.cpu.q[0] = d.to_bits() as u128;
        Ok(())
    }),
    ("CAAnimation", false, "duration", |e| {
        let d = bag_f64(e, e.arg(0), "duration").unwrap_or(0.0);
        e.cpu.q[0] = d.to_bits() as u128;
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_curves() {
        let ease = named_timing("easeInEaseOut");
        let Timing::Bezier(p) = ease else { panic!() };
        assert!((bezier(p, 0.5) - 0.5).abs() < 1e-6);
        assert!(bezier(p, 0.25) < 0.25 && bezier(p, 0.75) > 0.75);
        assert_eq!(bezier(p, 0.0), 0.0);
        assert!((bezier(p, 1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn springs_settle_and_overshoot_when_underdamped() {
        // Critically damped: no overshoot, near 1 after a while.
        let crit = (0..200).map(|i| spring(1.0, 100.0, 20.0, 0.0, i as f64 * 0.01)).fold(0.0f64, f64::max);
        assert!(crit <= 1.0 + 1e-9);
        // Underdamped overshoots.
        let under = (0..200).map(|i| spring(1.0, 100.0, 5.0, 0.0, i as f64 * 0.01)).fold(0.0f64, f64::max);
        assert!(under > 1.05);
        assert!((spring(1.0, 100.0, 10.0, 0.0, 5.0) - 1.0).abs() < 1e-3);
        let d = settling_duration(1.0, 100.0, 10.0, 0.0);
        assert!(d > 0.5 && d < 2.0, "{d}");
    }

    #[test]
    fn basic_animation_values_over_time() {
        let a = Anim {
            key: "o".into(),
            prop: Prop::Opacity,
            curve: Curve::Basic { from: Some(Val::N(0.0)), to: Some(Val::N(1.0)), by: None },
            begin: 1.0,
            duration: 2.0,
            timing: Timing::Linear,
            repeat: 1.0,
            autoreverses: false,
            fill_forwards: false,
            fill_backwards: false,
            removed_on_completion: true,
            additive: false,
            object: 0,
            delegate: 0,
        };
        let model = Val::N(0.5);
        assert_eq!(a.value(0.5, model), None); // not started, no backwards fill
        assert_eq!(a.value(2.0, model), Some(Val::N(0.5)));
        assert_eq!(a.value(3.5, model), None); // over
        let rev = Anim { autoreverses: true, ..a.clone() };
        assert_eq!(rev.value(4.0, model), Some(Val::N(0.5))); // on the way back
        let keys = Anim { curve: Curve::Keyframes { values: vec![Val::N(0.0), Val::N(10.0), Val::N(0.0)], times: vec![], discrete: false }, ..a };
        assert_eq!(keys.value(2.0, model), Some(Val::N(10.0)));
        assert_eq!(keys.value(1.5, model), Some(Val::N(5.0)));
    }

    #[test]
    fn rotations_turn_rather_than_shrink() {
        let rot = |r: f64| {
            let mut m = identity3d();
            m[0] = r.cos();
            m[1] = r.sin();
            m[4] = -r.sin();
            m[5] = r.cos();
            m
        };
        let mid = lerp_transform(&identity3d(), &rot(std::f64::consts::FRAC_PI_2), 0.5);
        let scale = (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
        assert!((scale - 1.0).abs() < 1e-9);
        assert!((mid[1].atan2(mid[0]) - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
    }
}
