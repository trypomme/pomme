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

/// A UIImageView's images.
#[derive(Default, Clone, Copy)]
struct ImageView {
    image: u64,
    highlighted_image: u64,
    highlighted: bool,
}

#[derive(Clone, Default)]
pub struct State {
    image_views: HashMap<u64, ImageView>,
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

/// The image drawn another way (a rendering mode, a tint): a new UIImage.
fn restyled(emu: &mut Emu, change: impl FnOnce(&mut crate::render::ImageLook)) -> Result<()> {
    let Some(mut img) = image(emu, emu.arg(0)) else { return ret(emu, emu.arg(0)) };
    change(&mut img.look);
    if img.look.tint != 0 {
        objc::send(emu, img.look.tint, "retain", &[])?;
    }
    let o = new_image(emu, img)?;
    ret(emu, o)
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
    Ok(Image { pix: std::rc::Rc::new(pix), size: r.size, scale: r.scale, caps: [0.0; 4], orientation: 0, look: Default::default() })
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

// ---- making UIImages: from CGImages, files and data ----

/// UIImageOrientation → the EXIF orientation that turns its pixels upright.
fn exif_of(orientation: u8) -> u8 {
    [1, 3, 8, 6, 2, 4, 5, 7].get(orientation as usize).copied().unwrap_or(1)
}

/// `pix` as an image at `scale`, turned by `orientation`: its size in points,
/// sideways orientations swapping width and height.
fn image_of(pix: std::rc::Rc<tiny_skia::Pixmap>, scale: f64, orientation: u8) -> Image {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (w, h) = (pix.width() as f64 / scale, pix.height() as f64 / scale);
    let size = if matches!(orientation, 2 | 3 | 6 | 7) { [h, w] } else { [w, h] };
    Image { pix, size, scale, caps: [0.0; 4], orientation, look: Default::default() }
}

/// The receiver (an init) or a new UIImage of the receiving class (a class
/// method) holding `img`; nil without one.
fn ret_image(emu: &mut Emu, img: Option<Image>, class_method: bool) -> Result<()> {
    let Some(img) = img else { return ret(emu, 0) };
    let o = if class_method {
        let cls = emu.arg(0);
        let o = objc::send(emu, cls, "alloc", &[])?;
        objc::send(emu, o, "init", &[])?
    } else {
        emu.arg(0)
    };
    emu.render.images.insert(o, img);
    ret(emu, o)
}

fn from_cg(emu: &mut Emu, class_method: bool, with_scale: bool) -> Result<()> {
    let cg = emu.arg(2);
    let (scale, orientation) = if with_scale { (d(emu, 0), emu.arg(3) as u8) } else { (1.0, 0) };
    let img = image(emu, cg).map(|i| image_of(i.pix, scale, orientation));
    ret_image(emu, img, class_method)
}

/// The first frame of an image file's bytes (UIImage shows animated files still).
fn decode_first(bytes: &[u8], scale: f64) -> Option<Image> {
    let d = crate::imageio::decode(bytes)?;
    let f = d.frames.first()?.pix.clone();
    // EXIF orientations 1–8 → UIImageOrientation.
    let orientation = [0, 0, 4, 1, 5, 6, 3, 7, 2][d.orientation.min(8) as usize];
    Some(image_of(f, scale, orientation))
}

fn from_data(emu: &mut Emu, class_method: bool, with_scale: bool) -> Result<()> {
    let data = emu.arg(2);
    let scale = if with_scale { d(emu, 0) } else { 1.0 };
    let bytes = crate::foundation::data_bytes(emu, data)?;
    let img = decode_first(&bytes, scale);
    ret_image(emu, img, class_method)
}

/// A file's scale from its name: `name@2x.png` is 2×.
fn scale_in_name(path: &str) -> f64 {
    let stem = path.rsplit('/').next().unwrap_or(path);
    let stem = stem.split('.').next().unwrap_or(stem);
    match stem.rsplit_once('@') {
        Some((_, s)) => s.trim_end_matches('x').parse().unwrap_or(1.0),
        None => 1.0,
    }
}

fn read_guest_file(emu: &mut Emu, path: &str) -> Result<Option<Vec<u8>>> {
    let s = crate::foundation::new_nsstring(emu, path)?;
    let cls = host_class(emu, "NSData")?;
    let data = objc::send(emu, cls, "dataWithContentsOfFile:", &[s])?;
    Ok(if data == 0 { None } else { Some(crate::foundation::data_bytes(emu, data)?) })
}

fn from_file(emu: &mut Emu, class_method: bool) -> Result<()> {
    let path = crate::foundation::nsstring(emu, emu.arg(2))?;
    let img = match read_guest_file(emu, &path)? {
        Some(bytes) => decode_first(&bytes, scale_in_name(&path)),
        None => None,
    };
    ret_image(emu, img, class_method)
}

/// `+imageNamed:` (and the …inBundle: forms): a file in the bundle, the
/// variant for the screen's scale first (`name@3x.png`, `@2x`, plain).
fn named(emu: &mut Emu) -> Result<()> {
    let (name, bundle) = (emu.arg(2), emu.arg(3));
    if name == 0 {
        return ret(emu, 0);
    }
    let name = crate::foundation::nsstring(emu, name)?;
    let bundle = if bundle != 0 && objc::sel_name(emu, emu.arg(1)).contains("inBundle:") {
        bundle
    } else {
        let cls = host_class(emu, "NSBundle")?;
        objc::send(emu, cls, "mainBundle", &[])?
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !e.is_empty() && !e.contains('/') => (s.to_string(), e.to_string()),
        _ => (name.clone(), "png".to_string()),
    };
    let device = emu.device.scale.round() as u32;
    let mut candidates: Vec<String> = (1..=device).rev().map(|k| format!("{stem}@{k}x")).collect();
    candidates.push(stem.clone());
    for c in candidates {
        let n = crate::foundation::new_nsstring(emu, &c)?;
        let t = crate::foundation::new_nsstring(emu, &ext)?;
        let p = objc::send(emu, bundle, "pathForResource:ofType:", &[n, t])?;
        if p == 0 {
            continue;
        }
        let path = crate::foundation::nsstring(emu, p)?;
        if let Some(bytes) = read_guest_file(emu, &path)? {
            let img = decode_first(&bytes, scale_in_name(&path));
            return ret_image(emu, img, true);
        }
    }
    ret(emu, 0)
}

/// `-drawInRect:` and friends: into the current UIKit context, upright
/// (turned by the image's orientation), at `alpha`.
fn draw_in(emu: &mut Emu, rect: Option<[f64; 4]>, point: [f64; 2], alpha: f32) -> Result<()> {
    let Some(img) = image(emu, emu.arg(0)) else { return Ok(()) };
    let c = emu.render.stack.last().copied().unwrap_or(0);
    if c == 0 {
        return Ok(());
    }
    let pix = if img.orientation != 0 {
        crate::imageio::upright(&img.pix, exif_of(img.orientation)).map(std::rc::Rc::new).unwrap_or(img.pix)
    } else {
        img.pix
    };
    let rect = rect.unwrap_or([point[0], point[1], img.size[0], img.size[1]]);
    crate::render::draw_image(emu, c, pix, rect, true, alpha)
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

const F: &str = "UIGraphicsImageRendererFormat";
const R: &str = "UIGraphicsImageRenderer";
const C: &str = "UIGraphicsImageRendererContext";
const I: &str = "UIImage";

// ---- UIImageView: its image is its layer's contents ----

/// Show what the view should: the highlighted image while highlighted (if
/// it has one), else the image, at the image's scale; and re-lay out, since
/// its intrinsic size is the image's.
fn image_view_update(emu: &mut Emu, v: u64) -> Result<()> {
    let iv = emu.images.image_views.get(&v).copied().unwrap_or_default();
    let shown = if iv.highlighted && iv.highlighted_image != 0 { iv.highlighted_image } else { iv.image };
    let layer = objc::send(emu, v, "layer", &[])?;
    let img = image(emu, shown);
    let scale = img.as_ref().map_or(1.0, |i| i.scale);
    // A turned image shows upright: its pixels turned by its orientation.
    let contents = match img {
        Some(i) if i.orientation != 0 => match crate::imageio::upright(&i.pix, exif_of(i.orientation)) {
            Some(p) => new_image(emu, image_of(std::rc::Rc::new(p), i.scale, 0))?,
            None => shown,
        },
        _ => shown,
    };
    let l = crate::calayer::layer_mut(emu, layer);
    l.contents = contents;
    l.contents_scale = scale;
    objc::send(emu, v, "invalidateIntrinsicContentSize", &[])?;
    crate::host::frame_changed(emu);
    Ok(())
}

fn image_view_set(emu: &mut Emu, f: impl FnOnce(&mut ImageView, u64)) -> Result<()> {
    let (v, img) = (emu.arg(0), emu.arg(2));
    if img != 0 {
        objc::send(emu, img, "retain", &[])?;
    }
    f(emu.images.image_views.entry(v).or_default(), img);
    image_view_update(emu, v)
}

/// The image's size in points, or None for no image.
fn image_view_size(emu: &Emu, v: u64) -> Option<[f64; 2]> {
    let iv = emu.images.image_views.get(&v)?;
    let shown = if iv.highlighted && iv.highlighted_image != 0 { iv.highlighted_image } else { iv.image };
    image(emu, shown).map(|i| i.size)
}

/// `-initWithImage:` (and `…highlightedImage:`): sized to the image.
fn image_view_init(emu: &mut Emu, highlighted: bool) -> Result<()> {
    let (v, img, hi) = (emu.arg(0), emu.arg(2), if highlighted { emu.arg(3) } else { 0 });
    let size = image(emu, img).map_or([0.0; 2], |i| i.size);
    let v = objc::send_fp_id(emu, v, "initWithFrame:", &[], &[0.0, 0.0, size[0], size[1]])?;
    let entry = emu.images.image_views.entry(v).or_default();
    entry.image = img;
    entry.highlighted_image = hi;
    image_view_update(emu, v)?;
    ret(emu, v)
}

const IV: &str = "UIImageView";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (IV, false, "initWithImage:", |e| image_view_init(e, false)),
    (IV, false, "initWithImage:highlightedImage:", |e| image_view_init(e, true)),
    (IV, false, "image", |e| {
        let i = e.images.image_views.get(&e.arg(0)).map_or(0, |iv| iv.image);
        ret(e, i)
    }),
    (IV, false, "setImage:", |e| image_view_set(e, |iv, img| iv.image = img)),
    (IV, false, "highlightedImage", |e| {
        let i = e.images.image_views.get(&e.arg(0)).map_or(0, |iv| iv.highlighted_image);
        ret(e, i)
    }),
    (IV, false, "setHighlightedImage:", |e| image_view_set(e, |iv, img| iv.highlighted_image = img)),
    (IV, false, "isHighlighted", |e| {
        let h = e.images.image_views.get(&e.arg(0)).is_some_and(|iv| iv.highlighted);
        ret(e, h as u64)
    }),
    (IV, false, "setHighlighted:", |e| {
        let (v, h) = (e.arg(0), e.arg(2) & 0xff != 0);
        e.images.image_views.entry(v).or_default().highlighted = h;
        image_view_update(e, v)
    }),
    // UIViewNoIntrinsicMetric (-1) without an image.
    (IV, false, "intrinsicContentSize", |e| {
        let s = image_view_size(e, e.arg(0)).unwrap_or([-1.0, -1.0]);
        ret_pair(e, s[0], s[1])
    }),
    (IV, false, "sizeThatFits:", |e| {
        let s = image_view_size(e, e.arg(0)).unwrap_or([0.0, 0.0]);
        ret_pair(e, s[0], s[1])
    }),
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
    (I, false, "imageWithRenderingMode:", |e| {
        let mode = e.arg(2) as u8;
        restyled(e, |l| l.rendering_mode = mode)
    }),
    // A template in its own color.
    (I, false, "imageWithTintColor:", |e| {
        let c = e.arg(2);
        restyled(e, |l| {
            l.tint = c;
            l.rendering_mode = 2;
        })
    }),
    (I, false, "imageWithTintColor:renderingMode:", |e| {
        let (c, mode) = (e.arg(2), e.arg(3) as u8);
        restyled(e, |l| {
            l.tint = c;
            l.rendering_mode = mode;
        })
    }),
    (I, false, "imageFlippedForRightToLeftLayoutDirection", |e| restyled(e, |l| l.flips_rtl = true)),
    (I, false, "flipsForRightToLeftLayoutDirection", |e| {
        let f = image(e, e.arg(0)).is_some_and(|i| i.look.flips_rtl);
        ret(e, f as u64)
    }),
    (I, false, "renderingMode", |e| {
        let m = image(e, e.arg(0)).map_or(0, |i| i.look.rendering_mode);
        ret(e, m as u64)
    }),
    (I, false, "imageOrientation", |e| {
        let o = image(e, e.arg(0)).map_or(0, |i| i.orientation);
        ret(e, o as u64)
    }),
    (I, true, "imageWithCGImage:", |e| from_cg(e, true, false)),
    (I, true, "imageWithCGImage:scale:orientation:", |e| from_cg(e, true, true)),
    (I, false, "initWithCGImage:", |e| from_cg(e, false, false)),
    (I, false, "initWithCGImage:scale:orientation:", |e| from_cg(e, false, true)),
    (I, true, "imageWithData:", |e| from_data(e, true, false)),
    (I, true, "imageWithData:scale:", |e| from_data(e, true, true)),
    (I, false, "initWithData:", |e| from_data(e, false, false)),
    (I, false, "initWithData:scale:", |e| from_data(e, false, true)),
    (I, true, "imageWithContentsOfFile:", |e| from_file(e, true)),
    (I, false, "initWithContentsOfFile:", |e| from_file(e, false)),
    (I, true, "imageNamed:", named),
    (I, true, "imageNamed:inBundle:compatibleWithTraitCollection:", named),
    (I, true, "imageNamed:inBundle:withConfiguration:", named),
    // A still image: no frames, no duration.
    (I, false, "images", |e| ret(e, 0)),
    (I, false, "duration", |e| {
        crate::uikit::ret_f64(e, 0.0);
        Ok(())
    }),
    (I, false, "drawInRect:", |e| {
        let r = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        draw_in(e, Some(r), [0.0; 2], 1.0)
    }),
    // (Blend modes other than normal aren't drawn differently yet.)
    (I, false, "drawInRect:blendMode:alpha:", |e| {
        let r = [d(e, 0), d(e, 1), d(e, 2), d(e, 3)];
        let a = d(e, 4) as f32;
        draw_in(e, Some(r), [0.0; 2], a)
    }),
    (I, false, "drawAtPoint:", |e| {
        let p = [d(e, 0), d(e, 1)];
        draw_in(e, None, p, 1.0)
    }),
    (I, false, "drawAtPoint:blendMode:alpha:", |e| {
        let p = [d(e, 0), d(e, 1)];
        let a = d(e, 2) as f32;
        draw_in(e, None, p, a)
    }),
];

/// `CGImageGetWidth` / `CGImageGetHeight` (pixels).
pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    // An image's PNG (`image.pngData()`), at its pixels (nil without any).
    ("_UIImagePNGRepresentation", |e| {
        let Some(img) = image(e, e.arg(0)) else { return ret(e, 0) };
        let Ok(png) = img.pix.encode_png() else { return ret(e, 0) };
        let data = crate::foundation::new_data(e, &png)?;
        ret(e, data)
    }),
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
