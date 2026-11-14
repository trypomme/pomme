//! ImageIO: image files to CGImages. A CGImageSource holds the file's bytes
//! (all at once, or as they arrive for incremental loading) and decodes
//! them on demand: PNG (and APNG), JPEG (with its EXIF orientation), GIF
//! and WebP (animated or not), each frame with its delay. Properties come
//! back under ImageIO's own keys (`PixelWidth`, `{GIF}`/`DelayTime`, …), so
//! image libraries (SDWebImage, Kingfisher, React Native's loaders) read
//! them as they do on iOS.

use crate::objc::{self, host_class};
use crate::render::Image;
use crate::{foundation, Emu, HostFn, Result};
use std::collections::HashMap;
use std::rc::Rc;
use tiny_skia::Pixmap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl Format {
    /// The file's Uniform Type Identifier.
    pub fn uti(self) -> &'static str {
        match self {
            Format::Png => "public.png",
            Format::Jpeg => "public.jpeg",
            Format::Gif => "com.compuserve.gif",
            Format::WebP => "org.webmproject.webp",
        }
    }

    /// The properties sub-dictionary for this format (kCGImagePropertyPNGDictionary, …).
    fn dictionary(self) -> &'static str {
        match self {
            Format::Png => "{PNG}",
            Format::Jpeg => "{JFIF}",
            Format::Gif => "{GIF}",
            Format::WebP => "{WebP}",
        }
    }

    fn image_format(self) -> image::ImageFormat {
        match self {
            Format::Png => image::ImageFormat::Png,
            Format::Jpeg => image::ImageFormat::Jpeg,
            Format::Gif => image::ImageFormat::Gif,
            Format::WebP => image::ImageFormat::WebP,
        }
    }
}

/// What the file's first bytes say it is.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Format::WebP)
    } else {
        None
    }
}

#[derive(Clone)]
pub struct Frame {
    pub pix: Rc<Pixmap>,
    /// Seconds this frame shows for (animated images).
    pub delay: f64,
}

/// A decoded file.
pub struct Decoded {
    pub format: Format,
    pub frames: Vec<Frame>,
    /// EXIF orientation (1–8; 1 is upright).
    pub orientation: u8,
    pub has_alpha: bool,
}

impl Decoded {
    pub fn size(&self) -> (u32, u32) {
        self.frames.first().map_or((0, 0), |f| (f.pix.width(), f.pix.height()))
    }
    fn animated(&self) -> bool {
        self.frames.len() > 1
    }
}

fn pixmap(rgba: &image::RgbaImage) -> Option<Pixmap> {
    let mut pix = Pixmap::new(rgba.width(), rgba.height())?;
    for (dst, src) in pix.pixels_mut().iter_mut().zip(rgba.pixels()) {
        *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
    }
    Some(pix)
}

fn frames_of<'a>(frames: image::Frames<'a>) -> Option<Vec<Frame>> {
    let mut out = Vec::new();
    for f in frames {
        let f = f.ok()?;
        let (n, d) = f.delay().numer_denom_ms();
        let delay = if d == 0 { 0.0 } else { n as f64 / d as f64 / 1000.0 };
        out.push(Frame { pix: Rc::new(pixmap(f.buffer())?), delay });
    }
    (!out.is_empty()).then_some(out)
}

/// Decode a whole file.
pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    use image::{AnimationDecoder, ImageDecoder};
    let format = sniff(bytes)?;
    let cursor = || std::io::Cursor::new(bytes);
    // Animated formats: every frame.
    let animated = match format {
        Format::Gif => image::codecs::gif::GifDecoder::new(cursor()).ok().and_then(|d| frames_of(d.into_frames())),
        Format::WebP => image::codecs::webp::WebPDecoder::new(cursor()).ok().filter(|d| d.has_animation()).and_then(|d| frames_of(d.into_frames())),
        Format::Png => image::codecs::png::PngDecoder::new(cursor())
            .ok()
            .filter(|d| d.is_apng().unwrap_or(false))
            .and_then(|d| d.apng().ok())
            .and_then(|d| frames_of(d.into_frames())),
        Format::Jpeg => None,
    };
    let orientation = match format {
        Format::Jpeg => image::codecs::jpeg::JpegDecoder::new(cursor()).ok().and_then(|mut d| d.orientation().ok()).map_or(1, exif_orientation),
        _ => 1,
    };
    let frames = match animated {
        Some(f) => f,
        None => {
            let img = image::load_from_memory_with_format(bytes, format.image_format()).ok()?;
            vec![Frame { pix: Rc::new(pixmap(&img.to_rgba8())?), delay: 0.0 }]
        }
    };
    let has_alpha = format != Format::Jpeg && frames.iter().any(|f| f.pix.pixels().iter().any(|p| p.alpha() != 255));
    Some(Decoded { format, frames, orientation, has_alpha })
}

fn exif_orientation(o: image::metadata::Orientation) -> u8 {
    use image::metadata::Orientation::*;
    match o {
        NoTransforms => 1,
        FlipHorizontal => 2,
        Rotate180 => 3,
        FlipVertical => 4,
        Rotate90FlipH => 5,
        Rotate90 => 6,
        Rotate270FlipH => 7,
        Rotate270 => 8,
    }
}

// ---- CGImageSource ----

#[derive(Default)]
struct Source {
    bytes: Vec<u8>,
    /// Incremental sources: whether the last bytes have arrived.
    complete: bool,
    decoded: Option<Rc<Decoded>>,
}

/// A CGImageDestination: where the file goes, its type, and what's been added.
struct Destination {
    /// A CFMutableData to append to, or a file URL.
    data: u64,
    url: u64,
    format: Option<Format>,
    frames: Vec<(Rc<Pixmap>, f64)>,
    /// kCGImageDestinationLossyCompressionQuality (0–1).
    quality: Option<f64>,
}

#[derive(Default)]
pub struct State {
    sources: HashMap<u64, Source>,
    destinations: HashMap<u64, Destination>,
    /// CGDataProviders: their bytes.
    pub(crate) providers: HashMap<u64, Rc<Vec<u8>>>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn new_source(emu: &mut Emu, bytes: Vec<u8>, complete: bool) -> Result<u64> {
    let s = new_object(emu, "CGImageSource")?;
    emu.imageio.sources.insert(s, Source { bytes, complete, decoded: None });
    Ok(s)
}

/// The source's decoded file, decoding it the first time (once all its bytes are in).
fn decoded(emu: &mut Emu, s: u64) -> Option<Rc<Decoded>> {
    let src = emu.imageio.sources.get_mut(&s)?;
    if src.decoded.is_none() && src.complete {
        src.decoded = decode(&src.bytes).map(Rc::new);
    }
    src.decoded.clone()
}

/// CGImageSourceStatus.
const COMPLETE: i64 = 0;
const INCOMPLETE: i64 = -1;
const UNKNOWN_TYPE: i64 = -3;
const INVALID_DATA: i64 = -4;

fn status(emu: &mut Emu, s: u64) -> i64 {
    let Some(src) = emu.imageio.sources.get(&s) else { return INVALID_DATA };
    if sniff(&src.bytes).is_none() {
        return if src.bytes.len() < 12 && !src.complete { INCOMPLETE } else { UNKNOWN_TYPE };
    }
    if !src.complete {
        return INCOMPLETE;
    }
    if decoded(emu, s).is_some() { COMPLETE } else { INVALID_DATA }
}

/// A CGImage (pomme's UIImage objects are their own CGImages) at 1×.
pub fn new_cg_image(emu: &mut Emu, pix: Rc<Pixmap>) -> Result<u64> {
    let size = [pix.width() as f64, pix.height() as f64];
    crate::image::new_image(emu, Image { pix, size, scale: 1.0, caps: [0.0; 4], orientation: 0 })
}

/// An option's value in an options dictionary (0 if absent).
fn option(emu: &mut Emu, dict: u64, key: &str) -> Result<u64> {
    if dict == 0 {
        return Ok(0);
    }
    let k = foundation::new_nsstring(emu, key)?;
    foundation::dict_get(emu, dict, k)
}

fn number_option(emu: &mut Emu, dict: u64, key: &str) -> Result<Option<f64>> {
    let v = option(emu, dict, key)?;
    if v == 0 {
        return Ok(None);
    }
    let r = objc::send_fp(emu, v, "doubleValue", &[], &[])?;
    Ok(Some(r[0]))
}

fn bool_option(emu: &mut Emu, dict: u64, key: &str) -> Result<bool> {
    let v = option(emu, dict, key)?;
    Ok(v != 0 && objc::send(emu, v, "boolValue", &[])? & 0xff != 0)
}

fn dict(emu: &mut Emu, pairs: &[(&str, u64)]) -> Result<u64> {
    let mut kv = Vec::new();
    for &(k, v) in pairs {
        let k = foundation::new_nsstring(emu, k)?;
        kv.push((k, v));
    }
    foundation::new_dict_of(emu, &kv)
}

/// `CGImageSourceCopyPropertiesAtIndex`: size, orientation, alpha, and the
/// format's dictionary (frame delays for animations).
fn frame_properties(emu: &mut Emu, d: &Decoded, i: usize) -> Result<u64> {
    let (w, h) = d.size();
    let width = foundation::new_number_i64(emu, w as i64)?;
    let height = foundation::new_number_i64(emu, h as i64)?;
    let orientation = foundation::new_number_i64(emu, d.orientation as i64)?;
    let alpha = foundation::new_number_i64(emu, d.has_alpha as i64)?;
    let depth = foundation::new_number_i64(emu, 8)?;
    let rgb = foundation::new_nsstring(emu, "RGB")?;
    let mut format = Vec::new();
    if d.animated() {
        let delay = foundation::new_number_f64(emu, d.frames[i].delay)?;
        // ImageIO clamps short GIF delays to 0.1 s in DelayTime; the unclamped one is as written.
        let clamped = foundation::new_number_f64(emu, if d.frames[i].delay < 0.011 { 0.1 } else { d.frames[i].delay })?;
        format.push(("DelayTime", clamped));
        format.push(("UnclampedDelayTime", delay));
    }
    let format_dict = dict(emu, &format)?;
    dict(emu, &[("PixelWidth", width), ("PixelHeight", height), ("Orientation", orientation), ("HasAlpha", alpha), ("Depth", depth), ("ColorModel", rgb), (d.format.dictionary(), format_dict)])
}

fn file_properties(emu: &mut Emu, d: &Decoded) -> Result<u64> {
    let mut format = Vec::new();
    if d.animated() {
        let loops = foundation::new_number_i64(emu, 0)?; // forever
        format.push(("LoopCount", loops));
    }
    let format_dict = dict(emu, &format)?;
    let size = foundation::new_number_i64(emu, 0)?;
    dict(emu, &[(d.format.dictionary(), format_dict), ("FileSize", size)])
}

/// A thumbnail no larger than `max` pixels on its longer side.
fn thumbnail(pix: &Pixmap, max: f64) -> Option<Pixmap> {
    let (w, h) = (pix.width() as f64, pix.height() as f64);
    let k = (max / w.max(h)).min(1.0);
    if k >= 1.0 {
        return Some(pix.clone());
    }
    let (tw, th) = ((w * k).round().max(1.0) as u32, (h * k).round().max(1.0) as u32);
    let mut out = Pixmap::new(tw, th)?;
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
    out.draw_pixmap(0, 0, pix.as_ref(), &paint, tiny_skia::Transform::from_scale((tw as f64 / w) as f32, (th as f64 / h) as f32), None);
    Some(out)
}

/// A pixmap turned upright for an EXIF orientation (what
/// kCGImageSourceCreateThumbnailWithTransform asks for).
pub(crate) fn upright(pix: &Pixmap, orientation: u8) -> Option<Pixmap> {
    let (w, h) = (pix.width() as f32, pix.height() as f32);
    // (a, b, c, d, tx, ty) and whether width and height swap.
    let (m, swap) = match orientation {
        2 => ([-1.0, 0.0, 0.0, 1.0, w, 0.0], false),
        3 => ([-1.0, 0.0, 0.0, -1.0, w, h], false),
        4 => ([1.0, 0.0, 0.0, -1.0, 0.0, h], false),
        5 => ([0.0, 1.0, 1.0, 0.0, 0.0, 0.0], true),
        6 => ([0.0, 1.0, -1.0, 0.0, h, 0.0], true),
        7 => ([0.0, -1.0, -1.0, 0.0, h, w], true),
        8 => ([0.0, -1.0, 1.0, 0.0, 0.0, w], true),
        _ => return Some(pix.clone()),
    };
    let (ow, oh) = if swap { (pix.height(), pix.width()) } else { (pix.width(), pix.height()) };
    let mut out = Pixmap::new(ow, oh)?;
    let t = tiny_skia::Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
    out.draw_pixmap(0, 0, pix.as_ref(), &tiny_skia::PixmapPaint::default(), t, None);
    Some(out)
}

fn create_image(emu: &mut Emu, thumb: bool) -> Result<()> {
    let (s, i, opts) = (emu.arg(0), emu.arg(1) as usize, emu.arg(2));
    let Some(d) = decoded(emu, s) else { return ret(emu, 0) };
    let Some(frame) = d.frames.get(i).cloned() else { return ret(emu, 0) };
    if !thumb {
        let img = new_cg_image(emu, frame.pix)?;
        return ret(emu, img);
    }
    let mut pix = (*frame.pix).clone();
    if bool_option(emu, opts, "kCGImageSourceCreateThumbnailWithTransform")? {
        pix = upright(&pix, d.orientation).unwrap_or(pix);
    }
    if let Some(max) = number_option(emu, opts, "kCGImageSourceThumbnailMaxPixelSize")? {
        pix = thumbnail(&pix, max).unwrap_or(pix);
    }
    let img = new_cg_image(emu, Rc::new(pix))?;
    ret(emu, img)
}

fn utis(emu: &mut Emu) -> Result<u64> {
    let mut list = Vec::new();
    for f in [Format::Png, Format::Jpeg, Format::Gif, Format::WebP] {
        list.push(foundation::new_nsstring(emu, f.uti())?);
    }
    foundation::new_array_of(emu, &list)
}

// ---- CGDataProvider and CGImage's pixels ----

fn new_provider(emu: &mut Emu, bytes: Vec<u8>) -> Result<u64> {
    let p = new_object(emu, "CGDataProvider")?;
    emu.imageio.providers.insert(p, Rc::new(bytes));
    Ok(p)
}

/// A CGImage's pixels as CoreGraphics lays them out for pomme's images:
/// 8-bit RGBA, premultiplied, alpha last (kCGImageAlphaPremultipliedLast).
fn pixel_bytes(pix: &Pixmap) -> Vec<u8> {
    pix.data().to_vec()
}

/// `CGImageCreate(width, height, bitsPerComponent, bitsPerPixel, bytesPerRow,
/// space, bitmapInfo, provider, decode, shouldInterpolate, intent)`.
fn cg_image_create(emu: &mut Emu) -> Result<()> {
    let (w, h, bpc, bpp, bpr, info, provider) = (emu.arg(0) as u32, emu.arg(1) as u32, emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(6) as u32, emu.arg(7));
    let Some(bytes) = emu.imageio.providers.get(&provider).cloned() else { return ret(emu, 0) };
    if bpc != 8 || (bpp != 32 && bpp != 24) || w == 0 || h == 0 {
        return ret(emu, 0);
    }
    let Some(mut pix) = Pixmap::new(w, h) else { return ret(emu, 0) };
    let bytes_pp = (bpp / 8) as usize;
    let (order, skip) = crate::render::pixel_layout(info);
    let premultiplied = matches!(info & 0x1f, 1 | 2);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let at = y * bpr as usize + x * bytes_pp;
            let Some(p) = bytes.get(at..at + bytes_pp) else { continue };
            let (r, g, b) = (p[order[0].min(bytes_pp - 1)], p[order[1].min(bytes_pp - 1)], p[order[2].min(bytes_pp - 1)]);
            let a = if skip || bytes_pp == 3 { 255 } else { p[order[3]] };
            let c = if premultiplied {
                tiny_skia::PremultipliedColorU8::from_rgba(r.min(a), g.min(a), b.min(a), a)
            } else {
                Some(tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply())
            };
            if let Some(c) = c {
                pix.pixels_mut()[y * w as usize + x] = c;
            }
        }
    }
    let img = new_cg_image(emu, Rc::new(pix))?;
    ret(emu, img)
}

fn image_pix(emu: &Emu, img: u64) -> Option<Rc<Pixmap>> {
    emu.render.images.get(&img).map(|i| i.pix.clone())
}

fn image_rgba(emu: &Emu, img: u64) -> Option<Image> {
    emu.render.images.get(&img).cloned()
}

/// `CGImageCreateWithImageInRect(image, rect)` (pixels).
fn cg_image_in_rect(emu: &mut Emu) -> Result<()> {
    let img = emu.arg(0);
    let r: Vec<f64> = (0..4).map(|i| f64::from_bits(emu.cpu.q[i] as u64)).collect();
    let Some(pix) = image_pix(emu, img) else { return ret(emu, 0) };
    let rect = tiny_skia::IntRect::from_xywh(r[0].floor() as i32, r[1].floor() as i32, r[2].ceil() as u32, r[3].ceil() as u32);
    let Some(part) = rect.and_then(|r| pix.clone_rect(r)) else { return ret(emu, 0) };
    let out = new_cg_image(emu, Rc::new(part))?;
    ret(emu, out)
}

// ---- CGImageDestination ----

fn format_for_uti(uti: &str) -> Option<Format> {
    [Format::Png, Format::Jpeg, Format::Gif, Format::WebP].into_iter().find(|f| f.uti() == uti)
}

/// 8-bit RGBA, not premultiplied.
fn straight_rgba(pix: &Pixmap) -> image::RgbaImage {
    let mut out = image::RgbaImage::new(pix.width(), pix.height());
    for (dst, src) in out.pixels_mut().zip(pix.pixels()) {
        let c = src.demultiply();
        *dst = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

/// The file's bytes: one image, or an animation for GIF.
pub fn encode(format: Format, frames: &[(Rc<Pixmap>, f64)], quality: Option<f64>) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    let (first, _) = frames.first()?;
    let mut out = Vec::new();
    match format {
        Format::Png => {
            let rgba = straight_rgba(first);
            image::codecs::png::PngEncoder::new(&mut out).write_image(&rgba, rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).ok()?;
        }
        Format::Jpeg => {
            // JPEG has no alpha: composited over black, as ImageIO does.
            let rgba = straight_rgba(first);
            let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
            let q = (quality.unwrap_or(0.85).clamp(0.0, 1.0) * 100.0).round().max(1.0) as u8;
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q).write_image(&rgb, rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8).ok()?;
        }
        Format::Gif => {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            if frames.len() > 1 {
                enc.set_repeat(image::codecs::gif::Repeat::Infinite).ok()?;
            }
            for (pix, delay) in frames {
                let d = image::Delay::from_numer_denom_ms((delay * 1000.0).round() as u32, 1);
                enc.encode_frame(image::Frame::from_parts(straight_rgba(pix), 0, 0, d)).ok()?;
            }
        }
        Format::WebP => {
            let rgba = straight_rgba(first);
            image::codecs::webp::WebPEncoder::new_lossless(&mut out).write_image(&rgba, rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).ok()?;
        }
    }
    Some(out)
}

fn new_destination(emu: &mut Emu, data: u64, url: u64, uti: u64) -> Result<()> {
    let uti = foundation::nsstring(emu, uti)?;
    let Some(format) = format_for_uti(&uti) else { return ret(emu, 0) };
    let d = new_object(emu, "CGImageDestination")?;
    emu.imageio.destinations.insert(d, Destination { data, url, format: Some(format), frames: Vec::new(), quality: None });
    ret(emu, d)
}

/// Options and per-image properties: the lossy quality, and a GIF frame's delay.
fn apply_properties(emu: &mut Emu, d: u64, props: u64) -> Result<Option<f64>> {
    if let Some(q) = number_option(emu, props, "kCGImageDestinationLossyCompressionQuality")? {
        if let Some(dest) = emu.imageio.destinations.get_mut(&d) {
            dest.quality = Some(q);
        }
    }
    let gif = option(emu, props, "{GIF}")?;
    let delay = if gif != 0 { number_option(emu, gif, "DelayTime")? } else { None };
    Ok(delay)
}

fn add_image(emu: &mut Emu, d: u64, pix: Option<Rc<Pixmap>>, props: u64) -> Result<()> {
    let delay = apply_properties(emu, d, props)?.unwrap_or(0.1);
    if let (Some(pix), Some(dest)) = (pix, emu.imageio.destinations.get_mut(&d)) {
        dest.frames.push((pix, delay));
    }
    Ok(())
}

/// `CGImageDestinationFinalize`: encode and write. NO if there's nothing to write or it can't be encoded.
fn finalize(emu: &mut Emu) -> Result<()> {
    let d = emu.arg(0);
    let Some(dest) = emu.imageio.destinations.remove(&d) else { return ret(emu, 0) };
    let Some(bytes) = dest.format.and_then(|f| encode(f, &dest.frames, dest.quality)) else { return ret(emu, 0) };
    if dest.data != 0 {
        let buf = emu.alloc(bytes.len() as u64)?;
        emu.p.memory.write(buf, &bytes)?;
        objc::send(emu, dest.data, "appendBytes:length:", &[buf, bytes.len() as u64])?;
        return ret(emu, 1);
    }
    let data = foundation::new_data(emu, &bytes)?;
    let ok = objc::send(emu, data, "writeToURL:atomically:", &[dest.url, 1])? & 0xff;
    ret(emu, ok)
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    // (data, type, count, options)
    ("_CGImageDestinationCreateWithData", |e| {
        let (data, uti) = (e.arg(0), e.arg(1));
        new_destination(e, data, 0, uti)
    }),
    ("_CGImageDestinationCreateWithURL", |e| {
        let (url, uti) = (e.arg(0), e.arg(1));
        new_destination(e, 0, url, uti)
    }),
    ("_CGImageDestinationCopyTypeIdentifiers", |e| {
        let a = utis(e)?;
        ret(e, a)
    }),
    ("_CGImageDestinationGetTypeID", |e| ret(e, 0x1_0003)),
    ("_CGImageDestinationSetProperties", |e| {
        let (d, props) = (e.arg(0), e.arg(1));
        apply_properties(e, d, props)?;
        Ok(())
    }),
    ("_CGImageDestinationAddImage", |e| {
        let (d, img, props) = (e.arg(0), e.arg(1), e.arg(2));
        let pix = image_pix(e, img);
        add_image(e, d, pix, props)
    }),
    ("_CGImageDestinationAddImageFromSource", |e| {
        let (d, s, i, props) = (e.arg(0), e.arg(1), e.arg(2) as usize, e.arg(3));
        let pix = decoded(e, s).and_then(|dec| dec.frames.get(i).map(|f| f.pix.clone()));
        add_image(e, d, pix, props)
    }),
    ("_CGImageDestinationFinalize", finalize),
    ("_CGImageSourceCreateWithData", |e| {
        let bytes = foundation::data_bytes(e, e.arg(0))?;
        let s = new_source(e, bytes, true)?;
        ret(e, s)
    }),
    ("_CGImageSourceCreateWithURL", |e| {
        let p = objc::send(e, e.arg(0), "path", &[])?;
        let path = foundation::nsstring(e, p)?;
        let Some(bytes) = crate::fs::host_path(e, &path).and_then(|h| pomme_loader::vfs::read(&h).ok()) else { return ret(e, 0) };
        let s = new_source(e, bytes, true)?;
        ret(e, s)
    }),
    ("_CGImageSourceCreateWithDataProvider", |e| {
        let bytes = e.imageio.providers.get(&e.arg(0)).map(|b| b.to_vec()).unwrap_or_default();
        let s = new_source(e, bytes, true)?;
        ret(e, s)
    }),
    ("_CGImageSourceCreateIncremental", |e| {
        let s = new_source(e, Vec::new(), false)?;
        ret(e, s)
    }),
    ("_CGImageSourceUpdateData", |e| {
        let (s, data, last) = (e.arg(0), e.arg(1), e.arg(2) & 0xff != 0);
        let bytes = foundation::data_bytes(e, data)?;
        if let Some(src) = e.imageio.sources.get_mut(&s) {
            *src = Source { bytes, complete: last, decoded: None };
        }
        Ok(())
    }),
    ("_CGImageSourceGetStatus", |e| {
        let st = status(e, e.arg(0));
        ret(e, st as u64)
    }),
    ("_CGImageSourceGetStatusAtIndex", |e| {
        let (s, i) = (e.arg(0), e.arg(1) as usize);
        let st = status(e, s);
        let st = if st == COMPLETE && decoded(e, s).is_some_and(|d| i >= d.frames.len()) { INVALID_DATA } else { st };
        ret(e, st as u64)
    }),
    ("_CGImageSourceGetCount", |e| {
        let n = decoded(e, e.arg(0)).map_or(0, |d| d.frames.len());
        ret(e, n as u64)
    }),
    ("_CGImageSourceGetType", |e| {
        let s = e.arg(0);
        let f = e.imageio.sources.get(&s).and_then(|src| sniff(&src.bytes));
        let t = match f {
            Some(f) => foundation::new_nsstring(e, f.uti())?,
            None => 0,
        };
        ret(e, t)
    }),
    ("_CGImageSourceGetTypeID", |e| ret(e, 0x1_0001)),
    ("_CGImageSourceCopyTypeIdentifiers", |e| {
        let a = utis(e)?;
        ret(e, a)
    }),
    ("_CGImageSourceCopyProperties", |e| {
        let Some(d) = decoded(e, e.arg(0)) else { return ret(e, 0) };
        let p = file_properties(e, &d)?;
        ret(e, p)
    }),
    ("_CGImageSourceCopyPropertiesAtIndex", |e| {
        let i = e.arg(1) as usize;
        let Some(d) = decoded(e, e.arg(0)).filter(|d| i < d.frames.len()) else { return ret(e, 0) };
        let p = frame_properties(e, &d, i)?;
        ret(e, p)
    }),
    ("_CGImageSourceCreateImageAtIndex", |e| create_image(e, false)),
    ("_CGImageSourceCreateThumbnailAtIndex", |e| create_image(e, true)),
    ("_CGImageSourceRemoveCacheAtIndex", |_| Ok(())),
    // CGImage: 8-bit premultiplied RGBA.
    ("_CGImageGetBitsPerComponent", |e| ret(e, 8)),
    ("_CGImageGetBitsPerPixel", |e| ret(e, 32)),
    ("_CGImageGetBytesPerRow", |e| {
        let w = image_pix(e, e.arg(0)).map_or(0, |p| p.width());
        ret(e, w as u64 * 4)
    }),
    // kCGImageAlphaPremultipliedLast | kCGBitmapByteOrderDefault.
    ("_CGImageGetAlphaInfo", |e| ret(e, 1)),
    ("_CGImageGetBitmapInfo", |e| ret(e, 1)),
    ("_CGImageGetColorSpace", |e| {
        if e.ui.rgb_space == 0 {
            e.ui.rgb_space = e.alloc(16)?;
        }
        ret(e, e.ui.rgb_space)
    }),
    ("_CGImageGetUTType", |e| ret(e, 0)),
    ("_CGImageIsMask", |e| ret(e, 0)),
    ("_CGImageGetShouldInterpolate", |e| ret(e, 1)),
    ("_CGImageGetRenderingIntent", |e| ret(e, 0)),
    ("_CGImageGetDecode", |e| ret(e, 0)),
    ("_CGImageGetTypeID", |e| ret(e, 0x1_0002)),
    ("_CGImageCreate", cg_image_create),
    ("_CGImageCreateWithImageInRect", cg_image_in_rect),
    ("_CGImageCreateCopy", |e| {
        let i = e.arg(0);
        ret(e, i)
    }),
    ("_CGImageGetDataProvider", |e| {
        let Some(img) = image_rgba(e, e.arg(0)) else { return ret(e, 0) };
        let p = new_provider(e, pixel_bytes(&img.pix))?;
        ret(e, p)
    }),
    ("_CGDataProviderCopyData", |e| {
        let bytes = e.imageio.providers.get(&e.arg(0)).cloned().unwrap_or_default();
        let d = foundation::new_data(e, &bytes)?;
        ret(e, d)
    }),
    ("_CGDataProviderCreateWithCFData", |e| {
        let bytes = foundation::data_bytes(e, e.arg(0))?;
        let p = new_provider(e, bytes)?;
        ret(e, p)
    }),
    // (info, data, size, releaseData): the bytes are copied now, then released.
    ("_CGDataProviderCreateWithData", |e| {
        let (info, data, size, release) = (e.arg(0), e.arg(1), e.arg(2), e.arg(3));
        let bytes = e.p.memory.read(data, size as usize)?.to_vec();
        let p = new_provider(e, bytes)?;
        if release != 0 {
            e.call(release, &[info, data, size])?;
        }
        ret(e, p)
    }),
    // A file's bytes (0 if it isn't there).
    ("_CGDataProviderCreateWithURL", |e| {
        let path = foundation::url_file_path(e, e.arg(0))?;
        let Some(bytes) = crate::fs::read_file(e, &path) else { return ret(e, 0) };
        let p = new_provider(e, bytes)?;
        ret(e, p)
    }),
    ("_CGDataProviderCreateWithFilename", |e| {
        let path = e.cstr(e.arg(0))?;
        let Some(bytes) = crate::fs::read_file(e, &path) else { return ret(e, 0) };
        let p = new_provider(e, bytes)?;
        ret(e, p)
    }),
    ("_CGDataProviderRetain", |e| {
        let p = e.arg(0);
        ret(e, p)
    }),
    ("_CGDataProviderRelease", |_| Ok(())),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Rc<Pixmap> {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]));
        Rc::new(p)
    }

    #[test]
    fn sniffs_formats() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some(Format::Png));
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some(Format::Jpeg));
        assert_eq!(sniff(b"GIF89a"), Some(Format::Gif));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::WebP));
        assert_eq!(sniff(b"<svg"), None);
    }

    #[test]
    fn png_round_trip_keeps_pixels_and_alpha() {
        let bytes = encode(Format::Png, &[(solid(3, 2, [10, 200, 30, 128]), 0.0)], None).unwrap();
        let d = decode(&bytes).unwrap();
        assert_eq!((d.format, d.size(), d.frames.len(), d.has_alpha), (Format::Png, (3, 2), 1, true));
        // Kept premultiplied (as CoreGraphics does): within a step after the round trip.
        let c = d.frames[0].pix.pixel(1, 1).unwrap().demultiply();
        let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
        assert!(close(c.red(), 10) && close(c.green(), 200) && close(c.blue(), 30) && c.alpha() == 128, "{c:?}");
    }

    #[test]
    fn animated_gif_keeps_frames_and_delays() {
        let frames = [(solid(4, 4, [255, 0, 0, 255]), 0.5), (solid(4, 4, [0, 0, 255, 255]), 0.25)];
        let d = decode(&encode(Format::Gif, &frames, None).unwrap()).unwrap();
        assert_eq!(d.frames.len(), 2);
        assert_eq!((d.frames[0].delay, d.frames[1].delay), (0.5, 0.25));
        assert_eq!(d.frames[1].pix.pixel(0, 0).unwrap().blue(), 255);
    }

    #[test]
    fn exif_orientation_turns_upright() {
        // A 2×1 image: left pixel red. Orientation 6 (rotated 90° clockwise to show): 1×2, red on top.
        let mut p = Pixmap::new(2, 1).unwrap();
        p.pixels_mut()[0] = tiny_skia::ColorU8::from_rgba(255, 0, 0, 255).premultiply();
        p.pixels_mut()[1] = tiny_skia::ColorU8::from_rgba(0, 0, 255, 255).premultiply();
        let u = upright(&p, 6).unwrap();
        assert_eq!((u.width(), u.height()), (1, 2));
        assert_eq!(u.pixel(0, 0).unwrap().red(), 255);
    }
}
