//! UIImage and UIGraphicsImageRenderer.
//!
//! A renderer records its drawing block into a CGContext (render.rs) and
//! rasterizes it into a bitmap; the resulting UIImage is also its own
//! CGImage, the way pomme's UIColor is its own CGColor. Resizable images keep
//! their cap insets; layers draw them nine-sliced through `contentsCenter`.

use crate::objc::{self, host_class};
use crate::render::{self, Image};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Clone, Copy, Default)]
struct Renderer {
    size: [f64; 2],
    scale: f64,
}

#[derive(Default)]
pub struct State {
    /// UIGraphicsImageRendererFormat → scale.
    formats: HashMap<u64, f64>,
    renderers: HashMap<u64, Renderer>,
    /// UIGraphicsImageRendererContext → CGContext.
    contexts: HashMap<u64, u64>,
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_pair(emu: &mut Emu, a: f64, b: f64) -> Result<()> {
    emu.cpu.q[0] = a.to_bits() as u128;
    emu.cpu.q[1] = b.to_bits() as u128;
    Ok(())
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

/// A UIImage for `img`.
pub fn new_image(emu: &mut Emu, img: Image) -> Result<u64> {
    let o = new_object(emu, "UIImage")?;
    emu.render.images.insert(o, img);
    Ok(o)
}

fn image(emu: &Emu, o: u64) -> Option<Image> {
    emu.render.images.get(&o).cloned()
}

// ---- UIGraphicsImageRendererFormat ----

fn format_new(emu: &mut Emu) -> Result<()> {
    let f = new_object(emu, "UIGraphicsImageRendererFormat")?;
    emu.images.formats.insert(f, emu.device.scale);
    ret(emu, f)
}

// ---- UIGraphicsImageRenderer ----

fn renderer_init(emu: &mut Emu, size: [f64; 2], format: u64) -> Result<()> {
    let r = emu.arg(0);
    let scale = emu.images.formats.get(&format).copied().unwrap_or(emu.device.scale);
    emu.images.renderers.insert(r, Renderer { size, scale });
    ret(emu, r)
}

/// Run the drawing block against a fresh context and rasterize what it drew.
fn draw(emu: &mut Emu, renderer: u64, block: u64) -> Result<Image> {
    let r = emu.images.renderers.get(&renderer).copied().unwrap_or(Renderer { size: [1.0, 1.0], scale: emu.device.scale });
    let cg = render::begin_context(emu)?;
    let rc = new_object(emu, "UIGraphicsImageRendererContext")?;
    emu.images.contexts.insert(rc, cg);
    let result = crate::libsystem::call_block(emu, block, &[rc]);
    let ops = render::end_context(emu, cg);
    emu.images.contexts.remove(&rc);
    result?;
    let pix = render::rasterize(emu, &ops, r.size, r.scale)?;
    // Debugging aid: POMME_DUMP_IMAGES=dir writes every rendered image as a PNG.
    if let Ok(dir) = std::env::var("POMME_DUMP_IMAGES") {
        let n = emu.images.renderers.len() + emu.render.images.len();
        let _ = std::fs::write(format!("{dir}/image-{n}-{}x{}.png", pix.width(), pix.height()), pix.encode_png().unwrap_or_default());
        println!("[image] {n}: {:?} pt at {}x, {} ops: {ops:?}", r.size, r.scale, ops.len());
    }
    Ok(Image { pix: std::rc::Rc::new(pix), size: r.size, scale: r.scale, caps: [0.0; 4] })
}

fn image_with_actions(emu: &mut Emu) -> Result<()> {
    let (r, block) = (emu.arg(0), emu.arg(2));
    let img = draw(emu, r, block)?;
    let o = new_image(emu, img)?;
    ret(emu, o)
}

fn png_with_actions(emu: &mut Emu) -> Result<()> {
    let (r, block) = (emu.arg(0), emu.arg(2));
    let img = draw(emu, r, block)?;
    let png = img.pix.encode_png().unwrap_or_default();
    let data = crate::foundation::new_data(emu, &png)?;
    ret(emu, data)
}

// ---- UIImage ----

fn resizable(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let caps = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    let Some(mut img) = image(emu, o) else { return ret(emu, o) };
    img.caps = caps;
    let n = new_image(emu, img)?;
    ret(emu, n)
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

const F: &str = "UIGraphicsImageRendererFormat";
const R: &str = "UIGraphicsImageRenderer";
const C: &str = "UIGraphicsImageRendererContext";
const I: &str = "UIImage";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (F, true, "defaultFormat", format_new),
    (F, true, "preferredFormat", format_new),
    (F, true, "formatForTraitCollection:", format_new),
    (F, false, "init", |e| {
        let f = e.arg(0);
        e.images.formats.insert(f, e.device.scale);
        ret(e, f)
    }),
    (F, false, "scale", |e| {
        let s = e.images.formats.get(&e.arg(0)).copied().unwrap_or(e.device.scale);
        crate::uikit::ret_f64(e, s);
        Ok(())
    }),
    (F, false, "setScale:", |e| {
        let (f, s) = (e.arg(0), d(e, 0));
        e.images.formats.insert(f, if s > 0.0 { s } else { e.device.scale });
        Ok(())
    }),
    (R, false, "initWithSize:", |e| {
        let size = [d(e, 0), d(e, 1)];
        renderer_init(e, size, 0)
    }),
    (R, false, "initWithSize:format:", |e| {
        let (size, f) = ([d(e, 0), d(e, 1)], e.arg(2));
        renderer_init(e, size, f)
    }),
    (R, false, "initWithBounds:format:", |e| {
        let (size, f) = ([d(e, 2), d(e, 3)], e.arg(2));
        renderer_init(e, size, f)
    }),
    (R, false, "imageWithActions:", image_with_actions),
    (R, false, "PNGDataWithActions:", png_with_actions),
    (C, false, "CGContext", |e| {
        let c = e.images.contexts.get(&e.arg(0)).copied().unwrap_or(0);
        ret(e, c)
    }),
    (I, false, "size", |e| {
        let s = image(e, e.arg(0)).map_or([0.0; 2], |i| i.size);
        ret_pair(e, s[0], s[1])
    }),
    (I, false, "scale", |e| {
        let s = image(e, e.arg(0)).map_or(1.0, |i| i.scale);
        crate::uikit::ret_f64(e, s);
        Ok(())
    }),
    (I, false, "capInsets", |e| {
        let c = image(e, e.arg(0)).map_or([0.0; 4], |i| i.caps);
        crate::uikit::ret_rect(e, c);
        Ok(())
    }),
    (I, false, "CGImage", ret_self),
    (I, false, "resizableImageWithCapInsets:", resizable),
    (I, false, "resizableImageWithCapInsets:resizingMode:", resizable),
    (I, false, "imageWithRenderingMode:", ret_self),
    (I, false, "imageFlippedForRightToLeftLayoutDirection", ret_self),
    (I, false, "renderingMode", |e| ret(e, 0)),
    (I, false, "imageOrientation", |e| ret(e, 0)),
];

/// `CGImageGetWidth` / `CGImageGetHeight` (pixels).
pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CGImageGetWidth", |e| {
        let w = image(e, e.arg(0)).map_or(0, |i| i.pix.width());
        ret(e, w as u64)
    }),
    ("_CGImageGetHeight", |e| {
        let h = image(e, e.arg(0)).map_or(0, |i| i.pix.height());
        ret(e, h as u64)
    }),
    ("_CGImageRetain", |e| {
        let i = e.arg(0);
        ret(e, i)
    }),
    ("_CGImageRelease", |_| Ok(())),
];
