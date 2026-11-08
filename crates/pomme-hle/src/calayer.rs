//! CALayer: the layer tree's state, kept host-side by address (layers get
//! subclassed too). No compositing yet; this is what a renderer will read.
//!
//! A view's own layer shares the view's state the way UIKit's does: its
//! bounds, position, opacity, hidden flag, background colour and clipping
//! are the view's. Everything else (corners, borders, shadows, transform,
//! contents, sublayers added by the app) lives in [`Layer`].
//!
//! `opacity` and `shadowOpacity` are `float`s (s0); CATransform3D has 16
//! members, so it's passed by reference and returned through x8.

use crate::objc::{self, send};
use crate::uikit::ret_rect;
use crate::uiview::{frame_of, set_frame_of, view, view_mut};
use crate::{Emu, HostFn, Result};

pub const IDENTITY: [f64; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

#[derive(Debug, Clone)]
pub struct Layer {
    pub bounds: [f64; 4],
    pub position: [f64; 2],
    pub anchor: [f64; 2],
    pub z: f64,
    pub superlayer: u64,
    pub sublayers: Vec<u64>,
    pub opacity: f32,
    pub hidden: bool,
    pub masks: bool,
    /// CGColor (pomme's UIColor objects stand in for CGColors).
    pub background: u64,
    pub corner_radius: f64,
    pub masked_corners: u64,
    pub border_width: f64,
    pub border_color: u64,
    pub shadow_color: u64,
    pub shadow_opacity: f32,
    pub shadow_radius: f64,
    pub shadow_offset: [f64; 2],
    pub shadow_path: u64,
    pub transform: [f64; 16],
    pub sublayer_transform: [f64; 16],
    pub contents: u64,
    pub contents_scale: f64,
    pub contents_gravity: u64,
    pub mask: u64,
    pub name: u64,
    pub delegate: u64,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            bounds: [0.0; 4],
            position: [0.0; 2],
            anchor: [0.5, 0.5],
            z: 0.0,
            superlayer: 0,
            sublayers: Vec::new(),
            opacity: 1.0,
            hidden: false,
            masks: false,
            background: 0,
            corner_radius: 0.0,
            masked_corners: 15,
            border_width: 0.0,
            border_color: 0,
            shadow_color: 0,
            shadow_opacity: 0.0,
            shadow_radius: 3.0,
            shadow_offset: [0.0, -3.0],
            shadow_path: 0,
            transform: IDENTITY,
            sublayer_transform: IDENTITY,
            contents: 0,
            contents_scale: 1.0,
            contents_gravity: 0,
            mask: 0,
            name: 0,
            delegate: 0,
        }
    }
}

pub fn layer_mut(emu: &mut Emu, l: u64) -> &mut Layer {
    emu.ui.views.layers.entry(l).or_default()
}

pub fn layer(emu: &mut Emu, l: u64) -> Layer {
    layer_mut(emu, l).clone()
}

/// The view whose own layer this is.
pub fn backing_view(emu: &Emu, l: u64) -> Option<u64> {
    emu.ui.views.layer_view.get(&l).copied()
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn s(emu: &Emu, i: usize) -> f32 {
    f32::from_bits(emu.cpu.q[i] as u32)
}

fn ret_d(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn ret_s(emu: &mut Emu, v: f32) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn ret_pair(emu: &mut Emu, p: [f64; 2]) -> Result<()> {
    emu.cpu.q[0] = p[0].to_bits() as u128;
    emu.cpu.q[1] = p[1].to_bits() as u128;
    Ok(())
}

fn ret_x(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

// ---- geometry ----

fn bounds_of(emu: &mut Emu, l: u64) -> [f64; 4] {
    match backing_view(emu, l) {
        Some(v) => view(emu, v).bounds,
        None => layer(emu, l).bounds,
    }
}

fn position_of(emu: &mut Emu, l: u64) -> [f64; 2] {
    match backing_view(emu, l) {
        Some(v) => view(emu, v).center,
        None => layer(emu, l).position,
    }
}

/// frame = position − anchor × size (transforms ignored).
pub fn layer_frame(emu: &mut Emu, l: u64) -> [f64; 4] {
    if let Some(v) = backing_view(emu, l) {
        return frame_of(emu, v);
    }
    let ly = layer(emu, l);
    let [_, _, w, h] = ly.bounds;
    [ly.position[0] - ly.anchor[0] * w, ly.position[1] - ly.anchor[1] * h, w, h]
}

fn get_bounds(emu: &mut Emu) -> Result<()> {
    let b = bounds_of(emu, emu.arg(0));
    ret_rect(emu, b);
    Ok(())
}

fn set_bounds(emu: &mut Emu) -> Result<()> {
    let (l, r) = (emu.arg(0), [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)]);
    match backing_view(emu, l) {
        Some(v) => view_mut(emu, v).bounds = r,
        None => layer_mut(emu, l).bounds = r,
    }
    Ok(())
}

fn get_position(emu: &mut Emu) -> Result<()> {
    let p = position_of(emu, emu.arg(0));
    ret_pair(emu, p)
}

fn set_position(emu: &mut Emu) -> Result<()> {
    let (l, p) = (emu.arg(0), [d(emu, 0), d(emu, 1)]);
    match backing_view(emu, l) {
        Some(v) => view_mut(emu, v).center = p,
        None => layer_mut(emu, l).position = p,
    }
    Ok(())
}

fn get_frame(emu: &mut Emu) -> Result<()> {
    let f = layer_frame(emu, emu.arg(0));
    ret_rect(emu, f);
    Ok(())
}

fn set_frame(emu: &mut Emu) -> Result<()> {
    let (l, r) = (emu.arg(0), [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)]);
    if let Some(v) = backing_view(emu, l) {
        set_frame_of(emu, v, r);
        return Ok(());
    }
    let ly = layer_mut(emu, l);
    ly.bounds[2] = r[2];
    ly.bounds[3] = r[3];
    ly.position = [r[0] + ly.anchor[0] * r[2], r[1] + ly.anchor[1] * r[3]];
    Ok(())
}

fn get_anchor(emu: &mut Emu) -> Result<()> {
    let a = layer(emu, emu.arg(0)).anchor;
    ret_pair(emu, a)
}

fn set_anchor(emu: &mut Emu) -> Result<()> {
    let (l, a) = (emu.arg(0), [d(emu, 0), d(emu, 1)]);
    layer_mut(emu, l).anchor = a;
    Ok(())
}

// ---- appearance ----

fn get_opacity(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let o = match backing_view(emu, l) {
        Some(v) => view(emu, v).alpha as f32,
        None => layer(emu, l).opacity,
    };
    ret_s(emu, o)
}

fn set_opacity(emu: &mut Emu) -> Result<()> {
    let (l, o) = (emu.arg(0), s(emu, 0));
    match backing_view(emu, l) {
        Some(v) => view_mut(emu, v).alpha = o as f64,
        None => layer_mut(emu, l).opacity = o,
    }
    Ok(())
}

fn get_hidden(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let h = match backing_view(emu, l) {
        Some(v) => view(emu, v).hidden,
        None => layer(emu, l).hidden,
    };
    ret_x(emu, h as u64)
}

fn set_hidden(emu: &mut Emu) -> Result<()> {
    let (l, h) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    match backing_view(emu, l) {
        Some(v) => view_mut(emu, v).hidden = h,
        None => layer_mut(emu, l).hidden = h,
    }
    Ok(())
}

fn get_masks(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let m = match backing_view(emu, l) {
        Some(v) => view(emu, v).clips,
        None => layer(emu, l).masks,
    };
    ret_x(emu, m as u64)
}

fn set_masks(emu: &mut Emu) -> Result<()> {
    let (l, m) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    match backing_view(emu, l) {
        Some(v) => view_mut(emu, v).clips = m,
        None => layer_mut(emu, l).masks = m,
    }
    Ok(())
}

fn get_background(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let c = match backing_view(emu, l) {
        Some(v) => view(emu, v).background,
        None => layer(emu, l).background,
    };
    ret_x(emu, c)
}

fn set_background(emu: &mut Emu) -> Result<()> {
    let (l, c) = (emu.arg(0), emu.arg(2));
    match backing_view(emu, l) {
        Some(v) => view_mut(emu, v).background = c,
        None => layer_mut(emu, l).background = c,
    }
    Ok(())
}

fn read_transform(emu: &Emu, p: u64) -> Result<[f64; 16]> {
    let mut t = [0.0; 16];
    for (i, v) in t.iter_mut().enumerate() {
        *v = f64::from_bits(emu.read_u64(p + i as u64 * 8)?);
    }
    Ok(t)
}

fn write_transform(emu: &mut Emu, p: u64, t: &[f64; 16]) -> Result<()> {
    for (i, v) in t.iter().enumerate() {
        emu.write_u64(p + i as u64 * 8, v.to_bits())?;
    }
    Ok(())
}

fn get_transform(emu: &mut Emu, sub: bool) -> Result<()> {
    let ly = layer(emu, emu.arg(0));
    let out = emu.cpu.x[8];
    write_transform(emu, out, if sub { &ly.sublayer_transform } else { &ly.transform })
}

fn set_transform(emu: &mut Emu, sub: bool) -> Result<()> {
    let (l, p) = (emu.arg(0), emu.arg(2));
    let t = read_transform(emu, p)?;
    let ly = layer_mut(emu, l);
    if sub {
        ly.sublayer_transform = t;
    } else {
        ly.transform = t;
    }
    Ok(())
}

/// The CGAffineTransform part of a CATransform3D.
pub fn affine_of(t: &[f64; 16]) -> [f64; 6] {
    [t[0], t[1], t[4], t[5], t[12], t[13]]
}

pub fn transform_of_affine(a: [f64; 6]) -> [f64; 16] {
    let mut t = IDENTITY;
    (t[0], t[1], t[4], t[5], t[12], t[13]) = (a[0], a[1], a[2], a[3], a[4], a[5]);
    t
}

/// `-[UIView transform]` (CGAffineTransform via x8) and `-setTransform:` (by reference in x2).
fn view_transform(emu: &mut Emu) -> Result<()> {
    let l = send(emu, emu.arg(0), "layer", &[])?;
    let a = affine_of(&layer(emu, l).transform);
    let out = emu.cpu.x[8];
    for (i, v) in a.iter().enumerate() {
        emu.write_u64(out + i as u64 * 8, v.to_bits())?;
    }
    Ok(())
}

fn view_set_transform(emu: &mut Emu) -> Result<()> {
    let (v, p) = (emu.arg(0), emu.arg(2));
    let mut a = [0.0; 6];
    for (i, x) in a.iter_mut().enumerate() {
        *x = f64::from_bits(emu.read_u64(p + i as u64 * 8)?);
    }
    let l = send(emu, v, "layer", &[])?;
    layer_mut(emu, l).transform = transform_of_affine(a);
    Ok(())
}

// ---- tree ----

fn detach(emu: &mut Emu, l: u64) {
    let sup = layer(emu, l).superlayer;
    if sup != 0 {
        layer_mut(emu, sup).sublayers.retain(|&s| s != l);
    }
    layer_mut(emu, l).superlayer = 0;
}

fn insert(emu: &mut Emu, parent: u64, child: u64, index: Option<usize>) {
    if child == 0 || child == parent {
        return;
    }
    detach(emu, child);
    let p = layer_mut(emu, parent);
    let i = index.unwrap_or(p.sublayers.len()).min(p.sublayers.len());
    p.sublayers.insert(i, child);
    layer_mut(emu, child).superlayer = parent;
}

fn add_sublayer(emu: &mut Emu) -> Result<()> {
    insert(emu, emu.arg(0), emu.arg(2), None);
    Ok(())
}

fn insert_at(emu: &mut Emu) -> Result<()> {
    insert(emu, emu.arg(0), emu.arg(2), Some(emu.arg(3) as usize));
    Ok(())
}

fn insert_relative(emu: &mut Emu, above: bool) -> Result<()> {
    let (p, c, sib) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let i = layer(emu, p).sublayers.iter().position(|&s| s == sib).map(|i| if above { i + 1 } else { i });
    insert(emu, p, c, i);
    Ok(())
}

fn replace_sublayer(emu: &mut Emu) -> Result<()> {
    let (p, old, new) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let i = layer(emu, p).sublayers.iter().position(|&s| s == old);
    detach(emu, old);
    insert(emu, p, new, i);
    Ok(())
}

fn remove_from_superlayer(emu: &mut Emu) -> Result<()> {
    detach(emu, emu.arg(0));
    Ok(())
}

fn sublayers(emu: &mut Emu) -> Result<()> {
    let s = layer(emu, emu.arg(0)).sublayers;
    let a = if s.is_empty() { 0 } else { crate::foundation::new_array_of(emu, &s)? };
    ret_x(emu, a)
}

fn superlayer(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let mut s = layer(emu, l).superlayer;
    if s == 0
        && let Some(v) = backing_view(emu, l)
    {
        let sup = view(emu, v).superview;
        if sup != 0 {
            s = send(emu, sup, "layer", &[])?;
        }
    }
    ret_x(emu, s)
}

fn delegate(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let d = backing_view(emu, l).unwrap_or_else(|| layer(emu, l).delegate);
    ret_x(emu, d)
}

fn nothing(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn zero(emu: &mut Emu) -> Result<()> {
    ret_x(emu, 0)
}

fn layer_class_new(emu: &mut Emu) -> Result<()> {
    let cls = emu.arg(0);
    let l = objc::create_instance(emu, cls, 0)?;
    layer_mut(emu, l);
    let l = send(emu, l, "init", &[])?;
    ret_x(emu, l)
}

/// Pointer-sized property accessors on [`Layer`].
macro_rules! obj_prop {
    ($get:ident, $set:ident, $field:ident) => {
        fn $get(emu: &mut Emu) -> Result<()> {
            let v = layer(emu, emu.arg(0)).$field;
            ret_x(emu, v)
        }
        fn $set(emu: &mut Emu) -> Result<()> {
            let (l, v) = (emu.arg(0), emu.arg(2));
            layer_mut(emu, l).$field = v;
            Ok(())
        }
    };
}

macro_rules! f64_prop {
    ($get:ident, $set:ident, $field:ident) => {
        fn $get(emu: &mut Emu) -> Result<()> {
            let v = layer(emu, emu.arg(0)).$field;
            ret_d(emu, v)
        }
        fn $set(emu: &mut Emu) -> Result<()> {
            let (l, v) = (emu.arg(0), d(emu, 0));
            layer_mut(emu, l).$field = v;
            Ok(())
        }
    };
}

obj_prop!(border_color, set_border_color, border_color);
obj_prop!(shadow_color, set_shadow_color, shadow_color);
obj_prop!(shadow_path, set_shadow_path, shadow_path);
obj_prop!(contents, set_contents, contents);
obj_prop!(contents_gravity, set_contents_gravity, contents_gravity);
obj_prop!(mask, set_mask, mask);
obj_prop!(name, set_name, name);
obj_prop!(masked_corners, set_masked_corners, masked_corners);
f64_prop!(corner_radius, set_corner_radius, corner_radius);
f64_prop!(border_width, set_border_width, border_width);
f64_prop!(shadow_radius, set_shadow_radius, shadow_radius);
f64_prop!(contents_scale, set_contents_scale, contents_scale);
f64_prop!(z_position, set_z_position, z);

const L: &str = "CALayer";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (L, true, "layer", layer_class_new),
    (L, false, "init", ret_self),
    (L, false, "bounds", get_bounds),
    (L, false, "setBounds:", set_bounds),
    (L, false, "position", get_position),
    (L, false, "setPosition:", set_position),
    (L, false, "frame", get_frame),
    (L, false, "setFrame:", set_frame),
    (L, false, "anchorPoint", get_anchor),
    (L, false, "setAnchorPoint:", set_anchor),
    (L, false, "zPosition", z_position),
    (L, false, "setZPosition:", set_z_position),
    (L, false, "opacity", get_opacity),
    (L, false, "setOpacity:", set_opacity),
    (L, false, "isHidden", get_hidden),
    (L, false, "setHidden:", set_hidden),
    (L, false, "masksToBounds", get_masks),
    (L, false, "setMasksToBounds:", set_masks),
    (L, false, "backgroundColor", get_background),
    (L, false, "setBackgroundColor:", set_background),
    (L, false, "cornerRadius", corner_radius),
    (L, false, "setCornerRadius:", set_corner_radius),
    (L, false, "maskedCorners", masked_corners),
    (L, false, "setMaskedCorners:", set_masked_corners),
    (L, false, "setCornerCurve:", nothing),
    (L, false, "borderWidth", border_width),
    (L, false, "setBorderWidth:", set_border_width),
    (L, false, "borderColor", border_color),
    (L, false, "setBorderColor:", set_border_color),
    (L, false, "shadowColor", shadow_color),
    (L, false, "setShadowColor:", set_shadow_color),
    (L, false, "shadowOpacity", |e| {
        let v = layer(e, e.arg(0)).shadow_opacity;
        ret_s(e, v)
    }),
    (L, false, "setShadowOpacity:", |e| {
        let (l, v) = (e.arg(0), s(e, 0));
        layer_mut(e, l).shadow_opacity = v;
        Ok(())
    }),
    (L, false, "shadowRadius", shadow_radius),
    (L, false, "setShadowRadius:", set_shadow_radius),
    (L, false, "shadowOffset", |e| {
        let v = layer(e, e.arg(0)).shadow_offset;
        ret_pair(e, v)
    }),
    (L, false, "setShadowOffset:", |e| {
        let (l, v) = (e.arg(0), [d(e, 0), d(e, 1)]);
        layer_mut(e, l).shadow_offset = v;
        Ok(())
    }),
    (L, false, "shadowPath", shadow_path),
    (L, false, "setShadowPath:", set_shadow_path),
    (L, false, "transform", |e| get_transform(e, false)),
    (L, false, "setTransform:", |e| set_transform(e, false)),
    (L, false, "sublayerTransform", |e| get_transform(e, true)),
    (L, false, "setSublayerTransform:", |e| set_transform(e, true)),
    (L, false, "contents", contents),
    (L, false, "setContents:", set_contents),
    (L, false, "contentsScale", contents_scale),
    (L, false, "setContentsScale:", set_contents_scale),
    (L, false, "contentsGravity", contents_gravity),
    (L, false, "setContentsGravity:", set_contents_gravity),
    (L, false, "mask", mask),
    (L, false, "setMask:", set_mask),
    (L, false, "name", name),
    (L, false, "setName:", set_name),
    (L, false, "delegate", delegate),
    (L, false, "setDelegate:", |e| {
        let (l, v) = (e.arg(0), e.arg(2));
        layer_mut(e, l).delegate = v;
        Ok(())
    }),
    (L, false, "sublayers", sublayers),
    (L, false, "superlayer", superlayer),
    (L, false, "addSublayer:", add_sublayer),
    (L, false, "insertSublayer:atIndex:", insert_at),
    (L, false, "insertSublayer:above:", |e| insert_relative(e, true)),
    (L, false, "insertSublayer:below:", |e| insert_relative(e, false)),
    (L, false, "replaceSublayer:with:", replace_sublayer),
    (L, false, "removeFromSuperlayer", remove_from_superlayer),
    (L, false, "presentationLayer", ret_self),
    (L, false, "modelLayer", ret_self),
    (L, false, "setNeedsDisplay", nothing),
    (L, false, "setNeedsLayout", nothing),
    (L, false, "setNeedsDisplayInRect:", nothing),
    (L, false, "layoutIfNeeded", nothing),
    (L, false, "layoutSublayers", nothing),
    (L, false, "displayIfNeeded", nothing),
    (L, false, "addAnimation:forKey:", nothing),
    (L, false, "removeAnimationForKey:", nothing),
    (L, false, "removeAllAnimations", nothing),
    (L, false, "animationForKey:", zero),
    (L, false, "animationKeys", zero),
    ("UIView", false, "transform", view_transform),
    ("UIView", false, "setTransform:", view_set_transform),
];
