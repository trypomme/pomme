//! UIActivityIndicatorView as iOS 26 draws it: eight spokes, a darker trail
//! going round. Like UIKit, the spinner doesn't rotate anything: its layer
//! plays 16 drawn frames as a discrete keyframe animation on `contents`,
//! 0.8 s a turn, forever. The frames are drawn here to the geometry and
//! shading measured from iOS's own (scripts/measure/transitions):
//!
//! - medium (20 pt): spokes 2.67 pt wide from 3.33 to 10 pt out from the centre;
//! - large (37 pt): 5 pt wide from 5.5 to 17.5 pt out, centred at 17.5 pt
//!   (iOS's large frames leave 2 pt free at the right and bottom);
//! - colour (60, 60, 67), the secondary label's; spokes rest at alpha 42/255,
//!   the head is 130/255 and each spoke behind it is 22 dimmer, falling 11
//!   more between its two frames; the head moves a spoke clockwise every two frames.

use crate::animation::{Anim, Curve, Prop, Timing, Val};
use crate::objc::{self, send};
use crate::render::Image;
use crate::{Emu, HostFn, Result};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

const CLASS: &str = "UIActivityIndicatorView";
/// UIActivityIndicatorViewStyle: medium and large, and the old white-large, white and gray.
const MEDIUM: u64 = 100;
const LARGE: u64 = 101;
const FRAMES: usize = 16;
const TURN: f64 = 0.8;
const GRAY: [f64; 4] = [60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 1.0];

/// How a style draws: (view size, centre, inner and outer spoke radius, spoke width) in points.
fn geometry(style: u64) -> (f64, f64, f64, f64, f64) {
    if is_large(style) { (37.0, 17.5, 5.5, 17.5, 5.0) } else { (20.0, 10.0, 10.0 / 3.0, 10.0, 8.0 / 3.0) }
}

fn is_large(style: u64) -> bool {
    matches!(style, LARGE | 0)
}

/// The spinner's colour: set by the app, or the style's (white for the old white styles).
fn color(emu: &mut Emu, v: u64, style: u64) -> [f64; 4] {
    match objc::bag_get(emu, v, "pomme.color").filter(|&c| c != 0) {
        Some(c) => emu.ui.views.colors.get(&c).copied().unwrap_or(GRAY),
        None if matches!(style, 0 | 1) => [1.0, 1.0, 1.0, 1.0],
        None => GRAY,
    }
}

/// Spoke `k`'s alpha (0–255) in frame `f`.
fn spoke_alpha(f: usize, k: usize) -> f32 {
    let head = (6 + f / 2) % 8;
    let age = (head + 8 - k) % 8;
    (130.0 - 22.0 * age as f32 - 11.0 * (f % 2) as f32).max(42.0)
}

/// Draw frame `f` for `style` at `scale` pixels per point.
fn draw_frame(style: u64, rgba: [f64; 4], f: usize, scale: f64) -> Option<Pixmap> {
    let (size, centre, inner, outer, width) = geometry(style);
    let px = (size * scale).round() as u32;
    let mut pix = Pixmap::new(px, px)?;
    let r = (width / 2.0) as f32;
    for k in 0..8 {
        // A capsule from `inner` to `outer` along the spoke, pointing up, then turned.
        let (a, b) = ((inner + width / 2.0) as f32, (outer - width / 2.0) as f32);
        let mut pb = PathBuilder::new();
        pb.push_circle(0.0, -a, r);
        pb.push_rect(tiny_skia::Rect::from_ltrb(-r, -b, r, -a)?);
        pb.push_circle(0.0, -b, r);
        let path = pb.finish()?;
        let t = Transform::from_rotate(45.0 * k as f32).post_translate(centre as f32, centre as f32).post_scale(scale as f32, scale as f32);
        let mut paint = Paint::default();
        paint.anti_alias = true;
        let alpha = spoke_alpha(f, k) / 255.0 * rgba[3] as f32;
        paint.set_color(tiny_skia::Color::from_rgba(rgba[0] as f32, rgba[1] as f32, rgba[2] as f32, alpha)?);
        pix.fill_path(&path, &paint, FillRule::Winding, t, None);
    }
    Some(pix)
}

/// The 16 frames for this spinner, as images (made once per style and colour).
fn frames(emu: &mut Emu, v: u64) -> Result<Vec<u64>> {
    let style = objc::bag_get(emu, v, "pomme.style").unwrap_or(MEDIUM);
    let rgba = color(emu, v, style);
    let key = (style, rgba.map(f64::to_bits));
    if let Some(ids) = emu.spinner_frames.get(&key) {
        return Ok(ids.clone());
    }
    let scale = emu.device.scale;
    let size = geometry(style).0;
    let mut ids = Vec::new();
    for f in 0..FRAMES {
        let Some(pix) = draw_frame(style, rgba, f, scale) else { continue };
        let id = emu.alloc(16)?;
        emu.render.images.insert(id, Image { pix: std::rc::Rc::new(pix), size: [size, size], scale, caps: [0.0; 4], orientation: 0 });
        ids.push(id);
    }
    emu.spinner_frames.insert(key, ids.clone());
    Ok(ids)
}

/// Show the spinner: still on its first frame, or turning.
fn update(emu: &mut Emu, v: u64) -> Result<()> {
    let ids = frames(emu, v)?;
    let l = send(emu, v, "layer", &[])?;
    let center = crate::foundation::new_nsstring(emu, "center")?;
    let animating = objc::bag_get(emu, v, "pomme.animating").unwrap_or(0) != 0;
    let scale = emu.device.scale;
    {
        let ly = crate::calayer::layer_mut(emu, l);
        ly.contents = ids.first().copied().unwrap_or(0);
        ly.contents_scale = scale;
        ly.contents_gravity = center;
    }
    crate::animation::remove(emu, l, Some("contents"))?;
    if animating && !ids.is_empty() {
        let begin = crate::threads::now_ns(emu) as f64 / 1e9;
        let anim = Anim {
            key: "contents".into(),
            prop: Prop::Contents,
            curve: Curve::Keyframes { values: ids.into_iter().map(Val::Obj).collect(), times: vec![], discrete: true },
            begin,
            duration: TURN,
            timing: Timing::Linear,
            repeat: f64::INFINITY,
            autoreverses: false,
            fill_forwards: false,
            fill_backwards: false,
            removed_on_completion: true,
            additive: false,
            object: 0,
            delegate: 0,
            added: Some(begin),
        };
        crate::animation::push(emu, l, anim);
    }
    let hides = objc::bag_get(emu, v, "pomme.hidesWhenStopped").unwrap_or(1) != 0;
    crate::uiview::view_mut(emu, v).hidden = hides && !animating;
    crate::host::changed(emu);
    Ok(())
}

fn set_style(emu: &mut Emu, v: u64, style: u64) -> Result<()> {
    objc::bag_set(emu, v, "pomme.style", style);
    update(emu, v)
}

fn ret_size(emu: &mut Emu, style: u64) -> Result<()> {
    let s = geometry(style).0;
    emu.cpu.q[0] = s.to_bits() as u128;
    emu.cpu.q[1] = s.to_bits() as u128;
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (CLASS, false, "initWithActivityIndicatorStyle:", |e| {
        let (v, style) = (e.arg(0), e.arg(2));
        let s = geometry(style).0;
        e.cpu.q[0] = 0;
        e.cpu.q[1] = 0;
        e.cpu.q[2] = s.to_bits() as u128;
        e.cpu.q[3] = s.to_bits() as u128;
        crate::uiview::view_init_with_frame(e)?;
        set_style(e, v, style)?;
        e.ret(v);
        Ok(())
    }),
    (CLASS, false, "initWithFrame:", |e| {
        let v = e.arg(0);
        crate::uiview::view_init_with_frame(e)?;
        set_style(e, v, MEDIUM)?;
        e.ret(v);
        Ok(())
    }),
    (CLASS, false, "startAnimating", |e| {
        let v = e.arg(0);
        objc::bag_set(e, v, "pomme.animating", 1);
        update(e, v)
    }),
    (CLASS, false, "stopAnimating", |e| {
        let v = e.arg(0);
        objc::bag_set(e, v, "pomme.animating", 0);
        update(e, v)
    }),
    (CLASS, false, "isAnimating", |e| {
        let r = objc::bag_get(e, e.arg(0), "pomme.animating").unwrap_or(0);
        e.ret(r);
        Ok(())
    }),
    (CLASS, false, "hidesWhenStopped", |e| {
        let r = objc::bag_get(e, e.arg(0), "pomme.hidesWhenStopped").unwrap_or(1);
        e.ret(r);
        Ok(())
    }),
    (CLASS, false, "setHidesWhenStopped:", |e| {
        let (v, h) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, v, "pomme.hidesWhenStopped", h);
        update(e, v)
    }),
    (CLASS, false, "activityIndicatorViewStyle", |e| {
        let r = objc::bag_get(e, e.arg(0), "pomme.style").unwrap_or(MEDIUM);
        e.ret(r);
        Ok(())
    }),
    (CLASS, false, "setActivityIndicatorViewStyle:", |e| {
        let (v, s) = (e.arg(0), e.arg(2));
        set_style(e, v, s)
    }),
    (CLASS, false, "color", |e| {
        let v = e.arg(0);
        let c = objc::bag_get(e, v, "pomme.color").unwrap_or(0);
        let c = if c != 0 {
            c
        } else {
            let style = objc::bag_get(e, v, "pomme.style").unwrap_or(MEDIUM);
            let rgba = color(e, v, style);
            crate::uiview::new_color(e, rgba)?
        };
        e.ret(c);
        Ok(())
    }),
    (CLASS, false, "setColor:", |e| {
        let (v, c) = (e.arg(0), e.arg(2));
        if c != 0 {
            send(e, c, "retain", &[])?;
        }
        objc::bag_set(e, v, "pomme.color", c);
        update(e, v)
    }),
    (CLASS, false, "intrinsicContentSize", |e| {
        let s = objc::bag_get(e, e.arg(0), "pomme.style").unwrap_or(MEDIUM);
        ret_size(e, s)
    }),
    (CLASS, false, "sizeThatFits:", |e| {
        let s = objc::bag_get(e, e.arg(0), "pomme.style").unwrap_or(MEDIUM);
        ret_size(e, s)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The alpha table, as measured from iOS 26's frames.
    #[test]
    fn spoke_shading_matches_ios() {
        let ios: [[u8; 8]; 4] = [[42, 42, 42, 64, 86, 108, 130, 42], [42, 42, 42, 53, 75, 97, 119, 42], [42, 42, 42, 42, 64, 86, 108, 130], [42, 42, 42, 42, 53, 75, 97, 119]];
        for (f, row) in ios.iter().enumerate() {
            for (k, &a) in row.iter().enumerate() {
                assert_eq!(spoke_alpha(f, k), a as f32, "frame {f} spoke {k}");
            }
        }
        // Frame 4: the head has come round to the top.
        assert_eq!(spoke_alpha(4, 0), 130.0);
    }

    #[test]
    fn frames_draw_the_spokes() {
        let pix = draw_frame(LARGE, GRAY, 0, 3.0).unwrap();
        assert_eq!(pix.width(), 111);
        // The left spoke (the head in frame 0) at 12 pt out from the centre, 17.5 pt.
        let p = pix.pixel(((17.5 - 12.0) * 3.0) as u32, (17.5 * 3.0) as u32).unwrap();
        assert!((p.alpha() as i32 - 130).abs() <= 3, "{}", p.alpha());
        // The top spoke, resting.
        let p = pix.pixel((17.5 * 3.0) as u32, ((17.5 - 12.0) * 3.0) as u32).unwrap();
        assert!((p.alpha() as i32 - 42).abs() <= 3, "{}", p.alpha());
        // Nothing at the centre.
        assert_eq!(pix.pixel((17.5 * 3.0) as u32, (17.5 * 3.0) as u32).unwrap().alpha(), 0);
    }
}
