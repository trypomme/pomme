//! UIBezierPath: a CGPath (pomme's, in the render module) with the line
//! width, line cap and join, and fill rule it's drawn with; drawn into the
//! current UIKit graphics context.

use crate::objc::{self, bag_get, bag_set};
use crate::render::{self, El, Line};
use crate::{Emu, HostFn, Result};

fn f(emu: &Emu, i: usize) -> f32 {
    f64::from_bits(emu.cpu.q[i] as u64) as f32
}

fn ret_f64(emu: &mut Emu, v: f64) {
    emu.cpu.q[0] = v.to_bits() as u128;
}

/// The path's CGPath (made the first time it's needed).
fn cgpath(emu: &mut Emu, b: u64) -> Result<u64> {
    if let Some(p) = bag_get(emu, b, "cgpath").filter(|&p| p != 0) {
        return Ok(p);
    }
    let p = render::new_path(emu, Vec::new())?;
    bag_set(emu, b, "cgpath", p);
    Ok(p)
}

fn els(emu: &mut Emu, b: u64) -> Result<Vec<El>> {
    let p = cgpath(emu, b)?;
    Ok(render::path_els(emu, p))
}

fn edit(emu: &mut Emu, b: u64, change: impl FnOnce(&mut Vec<El>)) -> Result<()> {
    let p = cgpath(emu, b)?;
    let mut e = render::path_els(emu, p);
    change(&mut e);
    render::set_path_els(emu, p, e);
    Ok(())
}

fn line_width(emu: &Emu, b: u64) -> f32 {
    bag_get(emu, b, "lineWidth").map_or(1.0, |w| f64::from_bits(w) as f32)
}

fn line(emu: &Emu, b: u64) -> Line {
    Line { cap: bag_get(emu, b, "lineCap").unwrap_or(0) as u8, join: bag_get(emu, b, "lineJoin").unwrap_or(0) as u8 }
}

/// A new path with these elements (the class constructors).
fn new_with(emu: &mut Emu, elements: Vec<El>) -> Result<()> {
    let cls = objc::host_class(emu, "UIBezierPath")?;
    let b = objc::create_instance(emu, cls, 0)?;
    let p = cgpath(emu, b)?;
    render::set_path_els(emu, p, elements);
    emu.ret(b);
    Ok(())
}

/// The last point of a path (where the next segment starts).
fn current_point(e: &[El]) -> (f32, f32) {
    let mut start = (0.0, 0.0);
    let mut cur = (0.0, 0.0);
    for el in e {
        match *el {
            El::Move(x, y) => {
                start = (x, y);
                cur = (x, y);
            }
            El::Line(x, y) | El::Cubic(_, _, _, _, x, y) => cur = (x, y),
            El::Close => cur = start,
        }
    }
    cur
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIBezierPath", true, "bezierPath", |e| new_with(e, Vec::new())),
    ("UIBezierPath", true, "bezierPathWithRect:", |e| {
        let r = [f(e, 0), f(e, 1), f(e, 2), f(e, 3)];
        new_with(e, render::rect_els(r[0], r[1], r[2], r[3]))
    }),
    ("UIBezierPath", true, "bezierPathWithOvalInRect:", |e| {
        let r = [f(e, 0), f(e, 1), f(e, 2), f(e, 3)];
        new_with(e, render::ellipse_els(r[0], r[1], r[2], r[3]))
    }),
    // (UIKit's rounded rectangles have continuous corners.)
    ("UIBezierPath", true, "bezierPathWithRoundedRect:cornerRadius:", |e| {
        let (r, radius) = ([f(e, 0), f(e, 1), f(e, 2), f(e, 3)], f(e, 4));
        let shape = render::corner_shape_radii(r[2], r[3], [radius; 4], true);
        new_with(e, render::transform_els(&shape, &[1.0, 0.0, 0.0, 1.0, r[0], r[1]]))
    }),
    ("UIBezierPath", true, "bezierPathWithArcCenter:radius:startAngle:endAngle:clockwise:", |e| {
        let (x, y, r, a0, a1, cw) = (f(e, 0), f(e, 1), f(e, 2), f(e, 3), f(e, 4), e.arg(2) & 1 != 0);
        let mut out = Vec::new();
        render::arc_els(&mut out, x, y, r, a0, a1, !cw);
        new_with(e, out)
    }),
    ("UIBezierPath", true, "bezierPathWithCGPath:", |e| {
        let elements = render::path_els(e, e.arg(2));
        new_with(e, elements)
    }),
    ("UIBezierPath", false, "init", |e| {
        let b = e.arg(0);
        cgpath(e, b)?;
        e.ret(b);
        Ok(())
    }),
    ("UIBezierPath", false, "moveToPoint:", |e| {
        let (x, y) = (f(e, 0), f(e, 1));
        edit(e, e.arg(0), |p| p.push(El::Move(x, y)))
    }),
    ("UIBezierPath", false, "addLineToPoint:", |e| {
        let (x, y) = (f(e, 0), f(e, 1));
        edit(e, e.arg(0), |p| p.push(El::Line(x, y)))
    }),
    ("UIBezierPath", false, "addCurveToPoint:controlPoint1:controlPoint2:", |e| {
        let v: Vec<f32> = (0..6).map(|i| f(e, i)).collect();
        edit(e, e.arg(0), |p| p.push(El::Cubic(v[2], v[3], v[4], v[5], v[0], v[1])))
    }),
    ("UIBezierPath", false, "addQuadCurveToPoint:controlPoint:", |e| {
        let (x, y, cx, cy) = (f(e, 0), f(e, 1), f(e, 2), f(e, 3));
        edit(e, e.arg(0), |p| {
            // As a cubic: the control point two thirds of the way from each end.
            let (x0, y0) = current_point(p);
            let c1 = (x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0));
            let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
            p.push(El::Cubic(c1.0, c1.1, c2.0, c2.1, x, y));
        })
    }),
    // (UIKit's clockwise, in its flipped coordinates, is increasing angle.)
    ("UIBezierPath", false, "addArcWithCenter:radius:startAngle:endAngle:clockwise:", |e| {
        let (x, y, r, a0, a1, cw) = (f(e, 0), f(e, 1), f(e, 2), f(e, 3), f(e, 4), e.arg(2) & 1 != 0);
        edit(e, e.arg(0), |p| render::arc_els(p, x, y, r, a0, a1, !cw))
    }),
    ("UIBezierPath", false, "closePath", |e| edit(e, e.arg(0), |p| p.push(El::Close))),
    ("UIBezierPath", false, "removeAllPoints", |e| edit(e, e.arg(0), |p| p.clear())),
    ("UIBezierPath", false, "appendPath:", |e| {
        let other = e.arg(2);
        let more = els(e, other)?;
        edit(e, e.arg(0), |p| p.extend(more))
    }),
    ("UIBezierPath", false, "CGPath", |e| {
        let p = cgpath(e, e.arg(0))?;
        e.ret(p);
        Ok(())
    }),
    ("UIBezierPath", false, "setCGPath:", |e| {
        let elements = render::path_els(e, e.arg(2));
        edit(e, e.arg(0), |p| *p = elements)
    }),
    ("UIBezierPath", false, "isEmpty", |e| {
        let empty = els(e, e.arg(0))?.is_empty();
        e.ret(empty as u64);
        Ok(())
    }),
    ("UIBezierPath", false, "currentPoint", |e| {
        let (x, y) = current_point(&els(e, e.arg(0))?);
        e.cpu.q[0] = (x as f64).to_bits() as u128;
        e.cpu.q[1] = (y as f64).to_bits() as u128;
        Ok(())
    }),
    ("UIBezierPath", false, "bounds", |e| {
        let r = render::path_bounds(&els(e, e.arg(0))?);
        for (i, v) in r.iter().enumerate() {
            e.cpu.q[i] = v.to_bits() as u128;
        }
        Ok(())
    }),
    ("UIBezierPath", false, "lineWidth", |e| {
        let w = line_width(e, e.arg(0));
        ret_f64(e, w as f64);
        Ok(())
    }),
    ("UIBezierPath", false, "setLineWidth:", |e| {
        let w = f64::from_bits(e.cpu.q[0] as u64);
        bag_set(e, e.arg(0), "lineWidth", w.to_bits());
        Ok(())
    }),
    ("UIBezierPath", false, "lineCapStyle", |e| {
        let c = line(e, e.arg(0)).cap;
        e.ret(c as u64);
        Ok(())
    }),
    ("UIBezierPath", false, "setLineCapStyle:", |e| {
        let v = e.arg(2);
        bag_set(e, e.arg(0), "lineCap", v);
        Ok(())
    }),
    ("UIBezierPath", false, "lineJoinStyle", |e| {
        let j = line(e, e.arg(0)).join;
        e.ret(j as u64);
        Ok(())
    }),
    ("UIBezierPath", false, "setLineJoinStyle:", |e| {
        let v = e.arg(2);
        bag_set(e, e.arg(0), "lineJoin", v);
        Ok(())
    }),
    ("UIBezierPath", false, "usesEvenOddFillRule", |e| {
        let v = bag_get(e, e.arg(0), "evenOdd").unwrap_or(0);
        e.ret(v);
        Ok(())
    }),
    ("UIBezierPath", false, "setUsesEvenOddFillRule:", |e| {
        let v = e.arg(2) & 1;
        bag_set(e, e.arg(0), "evenOdd", v);
        Ok(())
    }),
    ("UIBezierPath", false, "stroke", |e| {
        let b = e.arg(0);
        let (path, w, l) = (els(e, b)?, line_width(e, b), line(e, b));
        render::stroke_in_current(e, &path, w, l)
    }),
    ("UIBezierPath", false, "fill", |e| {
        let b = e.arg(0);
        let (path, eo) = (els(e, b)?, bag_get(e, b, "evenOdd").unwrap_or(0) != 0);
        render::fill_in_current(e, &path, eo)
    }),
    ("UIBezierPath", false, "addClip", |e| {
        let b = e.arg(0);
        let (path, eo) = (els(e, b)?, bag_get(e, b, "evenOdd").unwrap_or(0) != 0);
        render::clip_in_current(e, &path, eo)
    }),
];
