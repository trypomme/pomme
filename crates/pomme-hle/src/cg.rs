//! CoreGraphics geometry: CGRect / CGPoint functions and CGAffineTransform.
//!
//! CGRect and CGPoint are homogeneous float aggregates, so they travel in
//! d0–d7 (a second rect starts at d4). CGAffineTransform has six members:
//! it's passed by reference in an integer register and returned through x8.
//! Drawing (paths, contexts, images) comes with rendering.

use crate::uikit::ret_rect;
use crate::{Emu, HostFn, Result};

type Rect = [f64; 4];

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn rect_arg(emu: &Emu, first: usize) -> Rect {
    [d(emu, first), d(emu, first + 1), d(emu, first + 2), d(emu, first + 3)]
}

fn ret_f(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn ret_bool(emu: &mut Emu, b: bool) -> Result<()> {
    emu.ret(b as u64);
    Ok(())
}

fn ret_r(emu: &mut Emu, r: Rect) -> Result<()> {
    ret_rect(emu, r);
    Ok(())
}

const NULL: Rect = [f64::INFINITY, f64::INFINITY, 0.0, 0.0];

fn is_null(r: Rect) -> bool {
    r[0].is_infinite() || r[1].is_infinite()
}

/// Same rect with non-negative width and height.
fn standardize(r: Rect) -> Rect {
    let [mut x, mut y, mut w, mut h] = r;
    if w < 0.0 {
        x += w;
        w = -w;
    }
    if h < 0.0 {
        y += h;
        h = -h;
    }
    [x, y, w, h]
}

fn is_empty(r: Rect) -> bool {
    is_null(r) || r[2] == 0.0 || r[3] == 0.0
}

fn union(a: Rect, b: Rect) -> Rect {
    if is_null(a) {
        return b;
    }
    if is_null(b) {
        return a;
    }
    let (a, b) = (standardize(a), standardize(b));
    let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
    let (x1, y1) = ((a[0] + a[2]).max(b[0] + b[2]), (a[1] + a[3]).max(b[1] + b[3]));
    [x0, y0, x1 - x0, y1 - y0]
}

fn intersection(a: Rect, b: Rect) -> Rect {
    if is_null(a) || is_null(b) {
        return NULL;
    }
    let (a, b) = (standardize(a), standardize(b));
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    if x1 < x0 || y1 < y0 {
        return NULL;
    }
    [x0, y0, x1 - x0, y1 - y0]
}

fn get(emu: &mut Emu, f: fn(Rect) -> f64) -> Result<()> {
    let r = standardize(rect_arg(emu, 0));
    ret_f(emu, f(r))
}

fn equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (rect_arg(emu, 0), rect_arg(emu, 4));
    ret_bool(emu, a == b || (standardize(a) == standardize(b)))
}

fn contains_point(emu: &mut Emu) -> Result<()> {
    let r = standardize(rect_arg(emu, 0));
    let (px, py) = (d(emu, 4), d(emu, 5));
    ret_bool(emu, !is_null(r) && px >= r[0] && px < r[0] + r[2] && py >= r[1] && py < r[1] + r[3])
}

fn contains_rect(emu: &mut Emu) -> Result<()> {
    let (a, b) = (rect_arg(emu, 0), rect_arg(emu, 4));
    ret_bool(emu, !is_null(a) && union(a, b) == standardize(a))
}

fn intersects(emu: &mut Emu) -> Result<()> {
    let i = intersection(rect_arg(emu, 0), rect_arg(emu, 4));
    ret_bool(emu, !is_null(i) && !is_empty(i))
}

fn inset(emu: &mut Emu) -> Result<()> {
    let r = standardize(rect_arg(emu, 0));
    let (dx, dy) = (d(emu, 4), d(emu, 5));
    let out = [r[0] + dx, r[1] + dy, r[2] - 2.0 * dx, r[3] - 2.0 * dy];
    ret_r(emu, if is_null(r) || out[2] < 0.0 || out[3] < 0.0 { NULL } else { out })
}

fn offset(emu: &mut Emu) -> Result<()> {
    let r = rect_arg(emu, 0);
    let (dx, dy) = (d(emu, 4), d(emu, 5));
    ret_r(emu, if is_null(r) { r } else { [r[0] + dx, r[1] + dy, r[2], r[3]] })
}

// ---- CGAffineTransform {a, b, c, d, tx, ty} ----

type Affine = [f64; 6];

fn read_affine(emu: &Emu, p: u64) -> Result<Affine> {
    let mut t = [0.0; 6];
    for (i, v) in t.iter_mut().enumerate() {
        *v = f64::from_bits(emu.read_u64(p + i as u64 * 8)?);
    }
    Ok(t)
}

fn ret_affine(emu: &mut Emu, t: Affine) -> Result<()> {
    let out = emu.cpu.x[8];
    for (i, v) in t.iter().enumerate() {
        emu.write_u64(out + i as u64 * 8, v.to_bits())?;
    }
    Ok(())
}

/// `t1` then `t2`.
fn concat(t1: Affine, t2: Affine) -> Affine {
    let [a, b, c, d, tx, ty] = t1;
    let [a2, b2, c2, d2, tx2, ty2] = t2;
    [a * a2 + b * c2, a * b2 + b * d2, c * a2 + d * c2, c * b2 + d * d2, tx * a2 + ty * c2 + tx2, tx * b2 + ty * d2 + ty2]
}

fn translation(x: f64, y: f64) -> Affine {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn scale(x: f64, y: f64) -> Affine {
    [x, 0.0, 0.0, y, 0.0, 0.0]
}

fn make_translation(emu: &mut Emu) -> Result<()> {
    let t = translation(d(emu, 0), d(emu, 1));
    ret_affine(emu, t)
}

fn make_scale(emu: &mut Emu) -> Result<()> {
    let t = scale(d(emu, 0), d(emu, 1));
    ret_affine(emu, t)
}

fn translate(emu: &mut Emu) -> Result<()> {
    let t = read_affine(emu, emu.arg(0))?;
    let r = concat(translation(d(emu, 0), d(emu, 1)), t);
    ret_affine(emu, r)
}

fn scale_by(emu: &mut Emu) -> Result<()> {
    let t = read_affine(emu, emu.arg(0))?;
    let r = concat(scale(d(emu, 0), d(emu, 1)), t);
    ret_affine(emu, r)
}

fn concat_fn(emu: &mut Emu) -> Result<()> {
    let (a, b) = (read_affine(emu, emu.arg(0))?, read_affine(emu, emu.arg(1))?);
    ret_affine(emu, concat(a, b))
}

/// The inverse, or `t` itself when it isn't invertible (as CoreGraphics does).
fn invert(t: Affine) -> Affine {
    let [a, b, c, d, tx, ty] = t;
    let det = a * d - b * c;
    if det == 0.0 {
        return t;
    }
    let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
    [ia, ib, ic, id, -(tx * ia + ty * ic), -(tx * ib + ty * id)]
}

fn rotation(angle: f64) -> Affine {
    let (s, c) = angle.sin_cos();
    [c, s, -s, c, 0.0, 0.0]
}

fn apply_point(t: &Affine, x: f64, y: f64) -> (f64, f64) {
    (t[0] * x + t[2] * y + t[4], t[1] * x + t[3] * y + t[5])
}

fn ret_ds(emu: &mut Emu, v: &[f64]) -> Result<()> {
    for (i, x) in v.iter().enumerate() {
        emu.cpu.q[i] = x.to_bits() as u128;
    }
    Ok(())
}

// ---- CGColor: pomme's UIColor objects are their own CGColors ----

fn rgba(emu: &Emu, c: u64) -> [f64; 4] {
    emu.ui.views.colors.get(&c).copied().unwrap_or([0.0; 4])
}

/// `CGColorGetComponents` → `const CGFloat *` (r, g, b, a), allocated once per colour.
fn color_components(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    let p = match emu.ui.color_components.get(&c) {
        Some(&p) => p,
        None => {
            let p = emu.alloc(32)?;
            emu.ui.color_components.insert(c, p);
            p
        }
    };
    for (i, v) in rgba(emu, c).iter().enumerate() {
        emu.write_u64(p + i as u64 * 8, v.to_bits())?;
    }
    emu.ret(if c == 0 { 0 } else { p });
    Ok(())
}

fn color_alpha(emu: &mut Emu) -> Result<()> {
    let a = rgba(emu, emu.arg(0))[3];
    ret_f(emu, a)
}

fn color_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(1));
    let eq = a == b || (a != 0 && b != 0 && rgba(emu, a) == rgba(emu, b));
    ret_bool(emu, eq)
}

/// One device RGB colour space (any non-null pointer will do).
fn color_space(emu: &mut Emu) -> Result<()> {
    if emu.ui.rgb_space == 0 {
        emu.ui.rgb_space = emu.alloc(16)?;
    }
    emu.ret(emu.ui.rgb_space);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CGRectGetMinX", |e| get(e, |r| r[0])),
    ("_CGRectGetMinY", |e| get(e, |r| r[1])),
    ("_CGRectGetMidX", |e| get(e, |r| r[0] + r[2] / 2.0)),
    ("_CGRectGetMidY", |e| get(e, |r| r[1] + r[3] / 2.0)),
    ("_CGRectGetMaxX", |e| get(e, |r| r[0] + r[2])),
    ("_CGRectGetMaxY", |e| get(e, |r| r[1] + r[3])),
    ("_CGRectGetWidth", |e| get(e, |r| r[2])),
    ("_CGRectGetHeight", |e| get(e, |r| r[3])),
    ("_CGRectIsEmpty", |e| {
        let r = rect_arg(e, 0);
        ret_bool(e, is_empty(r))
    }),
    ("_CGRectIsNull", |e| {
        let r = rect_arg(e, 0);
        ret_bool(e, is_null(r))
    }),
    ("_CGRectEqualToRect", equal),
    ("_CGRectContainsPoint", contains_point),
    ("_CGRectContainsRect", contains_rect),
    ("_CGRectIntersectsRect", intersects),
    ("_CGRectIntersection", |e| {
        let r = intersection(rect_arg(e, 0), rect_arg(e, 4));
        ret_r(e, r)
    }),
    ("_CGRectUnion", |e| {
        let r = union(rect_arg(e, 0), rect_arg(e, 4));
        ret_r(e, r)
    }),
    ("_CGRectInset", inset),
    ("_CGRectOffset", offset),
    ("_CGRectStandardize", |e| {
        let r = standardize(rect_arg(e, 0));
        ret_r(e, r)
    }),
    ("_CGColorGetComponents", color_components),
    ("_CGColorGetAlpha", color_alpha),
    ("_CGColorGetNumberOfComponents", |e| {
        e.ret(4);
        Ok(())
    }),
    ("_CGColorEqualToColor", color_equal),
    ("_CGColorGetColorSpace", color_space),
    ("_CGColorSpaceCreateDeviceRGB", color_space),
    ("_CGColorSpaceGetModel", |e| {
        e.ret(1); // kCGColorSpaceModelRGB
        Ok(())
    }),
    ("_CGColorSpaceGetName", |e| {
        e.ret(0);
        Ok(())
    }),
    // Device RGB: no name, standard dynamic range.
    ("_CGColorSpaceCopyName", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_CGColorSpaceUsesITUR_2100TF", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_CGColorSpaceIsHDR", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_CGColorSpaceUsesExtendedRange", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_CGColorSpaceRelease", |_| Ok(())),
    ("_CGColorSpaceRetain", |_| Ok(())),
    ("_CGColorSpaceCreateWithName", color_space),
    ("_CGColorSpaceCreateDeviceGray", color_space),
    // `CGColorCreate(space, const CGFloat components[])`: RGB + alpha (pomme's one space).
    ("_CGColorCreate", |e| {
        let p = e.arg(1);
        let mut c = [0.0; 4];
        for (i, v) in c.iter_mut().enumerate() {
            *v = f64::from_bits(e.read_u64(p + i as u64 * 8)?);
        }
        let col = crate::uiview::new_color(e, c)?;
        e.ret(col);
        Ok(())
    }),
    ("_CGColorCreateSRGB", |e| {
        let col = crate::uiview::new_color(e, [d(e, 0), d(e, 1), d(e, 2), d(e, 3)])?;
        e.ret(col);
        Ok(())
    }),
    ("_CGColorCreateGenericRGB", |e| {
        let col = crate::uiview::new_color(e, [d(e, 0), d(e, 1), d(e, 2), d(e, 3)])?;
        e.ret(col);
        Ok(())
    }),
    ("_CGColorCreateGenericGray", |e| {
        let (g, a) = (d(e, 0), d(e, 1));
        let col = crate::uiview::new_color(e, [g, g, g, a])?;
        e.ret(col);
        Ok(())
    }),
    ("_CGColorRetain", |_| Ok(())),
    ("_CGColorRelease", |_| Ok(())),
    ("_CGAffineTransformMakeTranslation", make_translation),
    // CATransform3D (16 doubles, returned through x8).
    ("_CATransform3DMakeTranslation", |e| {
        let mut t = crate::calayer::IDENTITY;
        (t[12], t[13], t[14]) = (d(e, 0), d(e, 1), d(e, 2));
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &t)
    }),
    // The CATransform3D functions: transforms by reference (x0, x1), numbers in d0…, the result through x8.
    ("_CATransform3DMakeRotation", |e| {
        let t = crate::calayer::rotation3d(d(e, 0), d(e, 1), d(e, 2), d(e, 3));
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &t)
    }),
    ("_CATransform3DConcat", |e| {
        let a = crate::calayer::read_transform(e, e.cpu.x[0])?;
        let b = crate::calayer::read_transform(e, e.cpu.x[1])?;
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &crate::calayer::concat3d(&a, &b))
    }),
    ("_CATransform3DTranslate", |e| {
        let t = crate::calayer::read_transform(e, e.cpu.x[0])?;
        let mut m = crate::calayer::IDENTITY;
        (m[12], m[13], m[14]) = (d(e, 0), d(e, 1), d(e, 2));
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &crate::calayer::concat3d(&m, &t))
    }),
    ("_CATransform3DScale", |e| {
        let t = crate::calayer::read_transform(e, e.cpu.x[0])?;
        let mut m = crate::calayer::IDENTITY;
        (m[0], m[5], m[10]) = (d(e, 0), d(e, 1), d(e, 2));
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &crate::calayer::concat3d(&m, &t))
    }),
    ("_CATransform3DRotate", |e| {
        let t = crate::calayer::read_transform(e, e.cpu.x[0])?;
        let m = crate::calayer::rotation3d(d(e, 0), d(e, 1), d(e, 2), d(e, 3));
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &crate::calayer::concat3d(&m, &t))
    }),
    ("_CATransform3DEqualToTransform", |e| {
        let a = crate::calayer::read_transform(e, e.cpu.x[0])?;
        let b = crate::calayer::read_transform(e, e.cpu.x[1])?;
        e.ret((a == b) as u64);
        Ok(())
    }),
    ("_CATransform3DIsIdentity", |e| {
        let a = crate::calayer::read_transform(e, e.cpu.x[0])?;
        e.ret((a == crate::calayer::IDENTITY) as u64);
        Ok(())
    }),
    ("_CATransform3DMakeScale", |e| {
        let mut t = crate::calayer::IDENTITY;
        (t[0], t[5], t[10]) = (d(e, 0), d(e, 1), d(e, 2));
        let out = e.cpu.x[8];
        crate::calayer::write_transform(e, out, &t)
    }),
    ("_CGAffineTransformMakeScale", make_scale),
    ("_CGAffineTransformTranslate", translate),
    ("_CGAffineTransformScale", scale_by),
    ("_CGAffineTransformConcat", concat_fn),
    ("_CGAffineTransformInvert", |e| {
        let t = read_affine(e, e.arg(0))?;
        ret_affine(e, invert(t))
    }),
    ("_CGAffineTransformMakeRotation", |e| {
        let t = rotation(d(e, 0));
        ret_affine(e, t)
    }),
    ("_CGAffineTransformRotate", |e| {
        let t = read_affine(e, e.arg(0))?;
        let r = concat(rotation(d(e, 0)), t);
        ret_affine(e, r)
    }),
    ("_CGAffineTransformIsIdentity", |e| {
        let t = read_affine(e, e.arg(0))?;
        e.ret((t == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]) as u64);
        Ok(())
    }),
    ("_CGAffineTransformEqualToTransform", |e| {
        let (a, b) = (read_affine(e, e.arg(0))?, read_affine(e, e.arg(1))?);
        e.ret((a == b) as u64);
        Ok(())
    }),
    ("_CGAffineTransformMake", |e| {
        let t = [d(e, 0), d(e, 1), d(e, 2), d(e, 3), d(e, 4), d(e, 5)];
        ret_affine(e, t)
    }),
    ("_CGPointApplyAffineTransform", |e| {
        let t = read_affine(e, e.arg(0))?;
        let (x, y) = apply_point(&t, d(e, 0), d(e, 1));
        ret_ds(e, &[x, y])
    }),
    ("_CGSizeApplyAffineTransform", |e| {
        let t = read_affine(e, e.arg(0))?;
        let (w, h) = (d(e, 0), d(e, 1));
        ret_ds(e, &[t[0] * w + t[2] * h, t[1] * w + t[3] * h])
    }),
    ("_CGRectApplyAffineTransform", |e| {
        let t = read_affine(e, e.arg(0))?;
        let (x, y, w, h) = (d(e, 0), d(e, 1), d(e, 2), d(e, 3));
        let pts = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)].map(|(px, py)| apply_point(&t, px, py));
        let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        ret_ds(e, &[x0, y0, x1 - x0, y1 - y0])
    }),

];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects() {
        assert_eq!(union([0.0, 0.0, 10.0, 10.0], [5.0, 5.0, 10.0, 10.0]), [0.0, 0.0, 15.0, 15.0]);
        assert_eq!(intersection([0.0, 0.0, 10.0, 10.0], [5.0, 5.0, 10.0, 10.0]), [5.0, 5.0, 5.0, 5.0]);
        assert!(is_null(intersection([0.0, 0.0, 1.0, 1.0], [5.0, 5.0, 1.0, 1.0])));
        assert_eq!(standardize([10.0, 10.0, -5.0, -5.0]), [5.0, 5.0, 5.0, 5.0]);
    }

    #[test]
    fn inverts() {
        let t = concat(concat(scale(2.0, 3.0), rotation(0.7)), translation(5.0, -4.0));
        let r = concat(t, invert(t));
        for (x, y) in r.iter().zip([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]) {
            assert!((x - y).abs() < 1e-9, "{r:?}");
        }
    }

    #[test]
    fn affine() {
        // Translate then scale: (1,1) → (2,2) → (4,6).
        let t = concat(translation(1.0, 1.0), scale(2.0, 3.0));
        let [a, b, c, d, tx, ty] = t;
        assert_eq!((a * 1.0 + c * 1.0 + tx, b * 1.0 + d * 1.0 + ty), (4.0, 6.0));
    }
}
