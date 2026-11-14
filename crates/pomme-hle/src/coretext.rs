//! CoreText: fonts backed by real font files, as far as Skia's CoreText port
//! (React Native Skia, Flutter's text engine) and UIKit need.
//!
//! - Faces: the system UI font (`.AppleSystemUIFont`, the family Flutter asks
//!   for on iOS; pomme draws it with Inter) and the color emoji font
//!   (`Apple Color Emoji`, drawn with Noto Color Emoji's COLR glyphs), plus
//!   fonts apps register from data (`CTFontManagerCreateFontDescriptorFromData`).
//! - A CTFont is a face at a size, with a matrix and (for the variable system
//!   font) a weight. Metrics, glyph mapping, advances, bounds, outlines and raw
//!   tables come from the font file (ttf-parser).
//! - `CTFontDrawGlyphs` rasterizes into the context: bitmap contexts are drawn
//!   into right away (render.rs), recording contexts get the outlines.
//!
//! CoreText objects are host Objective-C objects with their own CF type IDs
//! (foundation.rs maps CFGetTypeID for them).

use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;
use std::rc::Rc;

/// CF type IDs pomme gives its CoreText/CoreGraphics objects.
pub const CTFONT_TYPE_ID: u64 = 0x7001;
pub const CTFONT_DESCRIPTOR_TYPE_ID: u64 = 0x7002;
pub const CGPATH_TYPE_ID: u64 = 0x7003;
pub const CGCONTEXT_TYPE_ID: u64 = 0x7004;

/// A font file's face.
#[derive(Clone)]
pub struct Face {
    pub family: String,
    pub postscript: String,
    pub full: String,
    pub style: String,
    pub bytes: Rc<Vec<u8>>,
    pub index: u32,
    pub color: bool,
    /// The variable system font: weight is an axis.
    pub variable_weight: bool,
}

#[derive(Clone, Debug)]
pub struct Font {
    pub face: usize,
    pub size: f64,
    /// Glyph-space transform (a, b, c, d, tx, ty), applied after scaling to size.
    pub matrix: [f64; 6],
    /// UIFontWeight (-0.8 … 0.62).
    pub weight: f64,
}

#[derive(Default)]
pub struct State {
    pub faces: Vec<Face>,
    /// CTFont / CGFont objects.
    pub fonts: HashMap<u64, Font>,
    /// CTFontDescriptor → (its attributes dictionary, the face it was made for, if any).
    descriptors: HashMap<u64, (u64, Option<usize>)>,
}

pub const SYSTEM_FAMILY: &str = ".AppleSystemUIFont";
pub const EMOJI_FAMILY: &str = "Apple Color Emoji";
const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// The faces, loaded on first use: the system font first, then emoji.
fn faces(emu: &mut Emu) -> &Vec<Face> {
    if emu.coretext.faces.is_empty() {
        if let Some(bytes) = crate::textkit::font_bytes(emu) {
            emu.coretext.faces.push(Face {
                family: SYSTEM_FAMILY.into(),
                postscript: ".SFUI-Regular".into(),
                full: "System Font Regular".into(),
                style: "Regular".into(),
                bytes,
                index: 0,
                color: false,
                variable_weight: true,
            });
        }
        if let Some(bytes) = crate::textkit::emoji_bytes(emu) {
            emu.coretext.faces.push(Face {
                family: EMOJI_FAMILY.into(),
                postscript: "AppleColorEmoji".into(),
                full: "Apple Color Emoji".into(),
                style: "Regular".into(),
                bytes,
                index: 0,
                color: true,
                variable_weight: false,
            });
        }
    }
    &emu.coretext.faces
}

fn face_named(emu: &mut Emu, family: &str) -> Option<usize> {
    let f = family.trim();
    faces(emu).iter().position(|x| x.family.eq_ignore_ascii_case(f) || x.postscript.eq_ignore_ascii_case(f) || x.full.eq_ignore_ascii_case(f)).or_else(|| {
        // The system font's other names.
        let system = [".SF UI Text", ".SF UI Display", "System Font", ".SFUI-Regular", "SF Pro", "SF Pro Text", "SF Pro Display", "Helvetica", "Helvetica Neue", "CupertinoSystemText", "CupertinoSystemDisplay"];
        system.iter().any(|s| s.eq_ignore_ascii_case(f)).then_some(0)
    })
}

/// Parse a face with the font's weight applied (for the variable system font).
fn with_face<T>(emu: &mut Emu, font: &Font, f: impl FnOnce(&ttf_parser::Face) -> T) -> Option<T> {
    let face = faces(emu).get(font.face)?.clone();
    let mut parsed = ttf_parser::Face::parse(&face.bytes, face.index).ok()?;
    if face.variable_weight {
        parsed.set_variation(ttf_parser::Tag::from_bytes(b"wght"), crate::textkit::ot_weight(font.weight) as f32);
        parsed.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), font.size.clamp(14.0, 32.0) as f32);
    }
    Some(f(&parsed))
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_f64(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn ret_rect(emu: &mut Emu, r: [f64; 4]) -> Result<()> {
    for (i, v) in r.iter().enumerate() {
        emu.cpu.q[i] = v.to_bits() as u128;
    }
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn read_affine(emu: &Emu, p: u64) -> Result<Option<[f64; 6]>> {
    if p == 0 {
        return Ok(None);
    }
    let mut m = [0.0; 6];
    for (i, v) in m.iter_mut().enumerate() {
        *v = f64::from_bits(emu.read_u64(p + i as u64 * 8)?);
    }
    Ok(Some(m))
}

/// A new CTFont object.
pub fn make_font(emu: &mut Emu, font: Font) -> Result<u64> {
    let cls = host_class(emu, "CTFont")?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.coretext.fonts.insert(o, font);
    Ok(o)
}

pub fn font(emu: &Emu, o: u64) -> Option<Font> {
    emu.coretext.fonts.get(&o).cloned()
}

fn system_font(size: f64) -> Font {
    Font { face: 0, size: if size > 0.0 { size } else { 12.0 }, matrix: IDENTITY, weight: 0.0 }
}

// ---- descriptors ----

fn make_descriptor(emu: &mut Emu, attrs: u64, face: Option<usize>) -> Result<u64> {
    let cls = host_class(emu, "CTFontDescriptor")?;
    let o = objc::create_instance(emu, cls, 0)?;
    if attrs != 0 {
        objc::send(emu, attrs, "retain", &[])?;
    }
    emu.coretext.descriptors.insert(o, (attrs, face));
    Ok(o)
}

/// A descriptor naming one of the faces.
fn face_descriptor(emu: &mut Emu, face: usize) -> Result<u64> {
    let f = faces(emu)[face].clone();
    let mut pairs = Vec::new();
    for (k, v) in [("kCTFontFamilyNameAttribute", &f.family), ("kCTFontNameAttribute", &f.postscript), ("kCTFontDisplayNameAttribute", &f.full), ("kCTFontStyleNameAttribute", &f.style)] {
        let k = foundation::new_nsstring(emu, k)?;
        let v = foundation::new_nsstring(emu, v)?;
        pairs.push((k, v));
    }
    let dict = foundation::new_dict_of(emu, &pairs)?;
    make_descriptor(emu, dict, Some(face))
}

/// A value from a CF/NS dictionary by a key constant's name (constants are their own names).
fn dict_get(emu: &mut Emu, dict: u64, key: &str) -> Result<u64> {
    if dict == 0 {
        return Ok(0);
    }
    let d = foundation::as_cf_dict(emu, dict)?;
    for (k, v) in foundation::dict_pairs(emu, d)? {
        if is_string(emu, k) && nsstring(emu, k)? == key {
            return Ok(v);
        }
    }
    Ok(0)
}

fn is_string(emu: &Emu, o: u64) -> bool {
    let isa = emu.read_u64(o).unwrap_or(0);
    isa != 0 && emu.objc.class_name(isa).contains("String")
}

fn number(emu: &mut Emu, n: u64) -> Result<Option<f64>> {
    if n == 0 {
        return Ok(None);
    }
    objc::send(emu, n, "doubleValue", &[])?;
    Ok(Some(f64::from_bits(emu.last_q0 as u64)))
}

/// What a descriptor's attributes ask for: (face, weight, size).
fn resolve(emu: &mut Emu, desc: u64) -> Result<(Option<usize>, f64, f64)> {
    let Some((attrs, face)) = emu.coretext.descriptors.get(&desc).cloned() else { return Ok((Some(0), 0.0, 0.0)) };
    let mut found = face;
    if found.is_none() {
        for key in ["kCTFontNameAttribute", "kCTFontFamilyNameAttribute", "kCTFontDisplayNameAttribute"] {
            let v = dict_get(emu, attrs, key)?;
            if v != 0 {
                let name = nsstring(emu, v)?;
                found = face_named(emu, &name);
                if found.is_none() {
                    return Ok((None, 0.0, 0.0)); // a family nobody has
                }
                break;
            }
        }
    }
    let traits = dict_get(emu, attrs, "kCTFontTraitsAttribute")?;
    let w = dict_get(emu, traits, "kCTFontWeightTrait")?;
    let weight = number(emu, w)?.unwrap_or(0.0);
    let s = dict_get(emu, attrs, "kCTFontSizeAttribute")?;
    let size = number(emu, s)?.unwrap_or(0.0);
    Ok((Some(found.unwrap_or(0)), weight, size))
}

// ---- functions ----

fn create_with_descriptor(emu: &mut Emu) -> Result<()> {
    let (desc, matrix) = (emu.arg(0), emu.arg(1));
    let size = d(emu, 0);
    let (face, weight, dsize) = resolve(emu, desc)?;
    let Some(face) = face else { return ret(emu, 0) };
    let size = if size > 0.0 { size } else if dsize > 0.0 { dsize } else { 12.0 };
    let matrix = read_affine(emu, matrix)?.unwrap_or(IDENTITY);
    let f = make_font(emu, Font { face, size, matrix, weight })?;
    ret(emu, f)
}

/// `CTFontCreateUIFontForLanguage(type, size, language)`: the system font (bold for the emphasized types).
fn create_ui_font(emu: &mut Emu) -> Result<()> {
    let ty = emu.arg(0) as i64;
    let mut font = system_font(d(emu, 0));
    // kCTFontUIFontEmphasizedSystem (3), WindowTitle, MenuTitle… emphasized ones are bold.
    if matches!(ty, 3 | 9 | 15 | 17 | 21) {
        font.weight = 0.4;
    }
    let f = make_font(emu, font)?;
    ret(emu, f)
}

fn create_copy_with_attributes(emu: &mut Emu) -> Result<()> {
    let (src, matrix, desc) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let size = d(emu, 0);
    let Some(mut f) = font(emu, src) else { return ret(emu, 0) };
    if size > 0.0 {
        f.size = size;
    }
    if let Some(m) = read_affine(emu, matrix)? {
        f.matrix = m;
    }
    if desc != 0 {
        let (face, weight, _) = resolve(emu, desc)?;
        if let Some(face) = face.filter(|_| emu.coretext.descriptors.get(&desc).is_some_and(|(_, fc)| fc.is_some())) {
            f.face = face;
        }
        if weight != 0.0 {
            f.weight = weight;
        }
    }
    let o = make_font(emu, f)?;
    ret(emu, o)
}

/// `CTFontCreateCopyWithSymbolicTraits(font, size, matrix, value, mask)`: bold changes the weight.
fn create_copy_with_traits(emu: &mut Emu) -> Result<()> {
    let (src, matrix, value, mask) = (emu.arg(0), emu.arg(1), emu.arg(2) as u32, emu.arg(3) as u32);
    let size = d(emu, 0);
    let Some(mut f) = font(emu, src) else { return ret(emu, 0) };
    if size > 0.0 {
        f.size = size;
    }
    if let Some(m) = read_affine(emu, matrix)? {
        f.matrix = m;
    }
    const BOLD: u32 = 1 << 1;
    if mask & BOLD != 0 {
        f.weight = if value & BOLD != 0 { 0.4 } else { 0.0 };
    }
    let o = make_font(emu, f)?;
    ret(emu, o)
}

/// `CTFontCreateForStringWithLanguage(font, string, range, language)`: a font that has the first character.
fn create_for_string(emu: &mut Emu) -> Result<()> {
    let (src, string, loc) = (emu.arg(0), emu.arg(1), emu.arg(2) as usize);
    let Some(f) = font(emu, src) else { return ret(emu, 0) };
    let s: Vec<char> = nsstring(emu, string)?.encode_utf16().skip(loc).collect::<Vec<u16>>().iter().copied().map(|u| u as u32).filter_map(char::from_u32).collect();
    let text = nsstring(emu, string)?;
    let ch = String::from_utf16_lossy(&text.encode_utf16().skip(loc).take(2).collect::<Vec<u16>>()).chars().next().or(s.first().copied()).unwrap_or(' ');
    let has = |emu: &mut Emu, face: usize| -> bool {
        let ff = Font { face, ..f.clone() };
        with_face(emu, &ff, |p| p.glyph_index(ch).is_some()).unwrap_or(false)
    };
    let face = if has(emu, f.face) {
        f.face
    } else {
        let n = faces(emu).len();
        (0..n).find(|&i| has(emu, i)).unwrap_or(f.face)
    };
    let o = make_font(emu, Font { face, ..f })?;
    ret(emu, o)
}

/// Metrics in points: (font units → points) for the font.
fn scale(emu: &mut Emu, f: &Font) -> f64 {
    with_face(emu, f, |p| f.size / p.units_per_em() as f64).unwrap_or(0.0)
}

fn metric(emu: &mut Emu, get: fn(&ttf_parser::Face) -> f64) -> Result<()> {
    let Some(f) = font(emu, emu.arg(0)) else { return ret_f64(emu, 0.0) };
    let s = scale(emu, &f);
    let v = with_face(emu, &f, get).unwrap_or(0.0) * s;
    ret_f64(emu, v)
}

/// `CTFontGetGlyphsForCharacters(font, const UniChar *chars, CGGlyph *glyphs, count)`.
fn glyphs_for_characters(emu: &mut Emu) -> Result<()> {
    let (src, chars, out, n) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3) as usize);
    let Some(f) = font(emu, src) else { return ret(emu, 0) };
    let units: Vec<u16> = (0..n).map(|i| emu.p.memory.read_u16(chars + i as u64 * 2)).collect::<std::result::Result<_, _>>()?;
    let mut glyphs = vec![0u16; n];
    let mut all = true;
    let mut i = 0;
    while i < n {
        let (ch, len) = match char::decode_utf16(units[i..].iter().copied()).next() {
            Some(Ok(c)) => (c, c.len_utf16()),
            _ => ('\u{fffd}', 1),
        };
        let g = with_face(emu, &f, |p| p.glyph_index(ch).map_or(0, |g| g.0)).unwrap_or(0);
        if g == 0 {
            all = false;
        }
        glyphs[i] = g;
        i += len;
    }
    for (k, g) in glyphs.iter().enumerate() {
        emu.p.memory.write_u16(out + k as u64 * 2, *g)?;
    }
    ret(emu, all as u64)
}

/// `CTFontGetAdvancesForGlyphs(font, orientation, glyphs, CGSize *advances, count)` → total.
fn advances(emu: &mut Emu) -> Result<()> {
    let (src, vertical, glyphs, out, n) = (emu.arg(0), emu.arg(1) == 2, emu.arg(2), emu.arg(3), emu.arg(4) as usize);
    let Some(f) = font(emu, src) else { return ret_f64(emu, 0.0) };
    let s = scale(emu, &f);
    let ids: Vec<u16> = (0..n).map(|i| emu.p.memory.read_u16(glyphs + i as u64 * 2)).collect::<std::result::Result<_, _>>()?;
    let adv: Vec<f64> = with_face(emu, &f, |p| {
        ids.iter().map(|&g| if vertical { p.glyph_ver_advance(ttf_parser::GlyphId(g)).unwrap_or(0) } else { p.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0) } as f64).collect()
    })
    .unwrap_or_default();
    let mut total = 0.0;
    for (i, a) in adv.iter().enumerate() {
        let a = a * s;
        total += a;
        if out != 0 {
            let (w, h) = if vertical { (0.0, a) } else { (a, 0.0) };
            emu.write_u64(out + i as u64 * 16, f64::to_bits(w))?;
            emu.write_u64(out + i as u64 * 16 + 8, f64::to_bits(h))?;
        }
    }
    ret_f64(emu, total)
}

/// A glyph's bounds in points (y up), with the font matrix applied.
fn glyph_bounds(emu: &mut Emu, f: &Font, g: u16) -> [f64; 4] {
    let s = scale(emu, f);
    let r = with_face(emu, f, |p| p.glyph_bounding_box(ttf_parser::GlyphId(g))).flatten();
    let Some(r) = r else { return [0.0; 4] };
    let m = f.matrix;
    let corners = [(r.x_min, r.y_min), (r.x_max, r.y_min), (r.x_min, r.y_max), (r.x_max, r.y_max)].map(|(x, y)| {
        let (x, y) = (x as f64 * s, y as f64 * s);
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    });
    let (x0, x1) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c.0), b.max(c.0)));
    let (y0, y1) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), c| (a.min(c.1), b.max(c.1)));
    [x0, y0, x1 - x0, y1 - y0]
}

/// `CTFontGetBoundingRectsForGlyphs(font, orientation, glyphs, CGRect *rects, count)` → their union.
fn bounding_rects(emu: &mut Emu) -> Result<()> {
    let (src, glyphs, out, n) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4) as usize);
    let Some(f) = font(emu, src) else { return ret_rect(emu, [0.0; 4]) };
    let mut union: Option<[f64; 4]> = None;
    for i in 0..n {
        let g = emu.p.memory.read_u16(glyphs + i as u64 * 2)?;
        let r = glyph_bounds(emu, &f, g);
        if out != 0 {
            for (k, v) in r.iter().enumerate() {
                emu.write_u64(out + i as u64 * 32 + k as u64 * 8, v.to_bits())?;
            }
        }
        if r[2] > 0.0 || r[3] > 0.0 {
            union = Some(match union {
                None => r,
                Some(u) => {
                    let (x0, y0) = (u[0].min(r[0]), u[1].min(r[1]));
                    let (x1, y1) = ((u[0] + u[2]).max(r[0] + r[2]), (u[1] + u[3]).max(r[1] + r[3]));
                    [x0, y0, x1 - x0, y1 - y0]
                }
            });
        }
    }
    ret_rect(emu, union.unwrap_or([0.0; 4]))
}

fn bounding_box(emu: &mut Emu) -> Result<()> {
    let Some(f) = font(emu, emu.arg(0)) else { return ret_rect(emu, [0.0; 4]) };
    let s = scale(emu, &f);
    let r = with_face(emu, &f, |p| p.global_bounding_box()).unwrap_or(ttf_parser::Rect { x_min: 0, y_min: 0, x_max: 0, y_max: 0 });
    ret_rect(emu, [r.x_min as f64 * s, r.y_min as f64 * s, (r.x_max - r.x_min) as f64 * s, (r.y_max - r.y_min) as f64 * s])
}

/// A glyph's outline in points (y up): the font matrix, then `extra`.
pub fn glyph_path(emu: &mut Emu, f: &Font, g: u16, extra: Option<[f64; 6]>) -> Vec<crate::render::El> {
    let s = scale(emu, f);
    let mut b = crate::render::ElBuilder::default();
    with_face(emu, f, |p| p.outline_glyph(ttf_parser::GlyphId(g), &mut b));
    let m = f.matrix;
    let glyph = [(m[0] * s) as f32, (m[1] * s) as f32, (m[2] * s) as f32, (m[3] * s) as f32, m[4] as f32, m[5] as f32];
    let t = match extra {
        Some(e) => crate::render::concat(&glyph, &e.map(|v| v as f32)),
        None => glyph,
    };
    crate::render::transform_els(&b.els, &t)
}

fn create_path_for_glyph(emu: &mut Emu) -> Result<()> {
    let (src, g, m) = (emu.arg(0), emu.arg(1) as u16, emu.arg(2));
    let Some(f) = font(emu, src) else { return ret(emu, 0) };
    let extra = read_affine(emu, m)?;
    let els = glyph_path(emu, &f, g, extra);
    let p = crate::render::new_path(emu, els)?;
    ret(emu, p)
}

/// `CTFontDrawGlyphs(font, glyphs, positions, count, context)`.
fn draw_glyphs(emu: &mut Emu) -> Result<()> {
    let (src, glyphs, positions, n, ctx) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3) as usize, emu.arg(4));
    let Some(f) = font(emu, src) else { return Ok(()) };
    let mut list = Vec::with_capacity(n);
    for i in 0..n {
        let g = emu.p.memory.read_u16(glyphs + i as u64 * 2)?;
        let x = f64::from_bits(emu.read_u64(positions + i as u64 * 16)?);
        let y = f64::from_bits(emu.read_u64(positions + i as u64 * 16 + 8)?);
        list.push((g, x, y));
    }
    let face = faces(emu).get(f.face).cloned();
    let Some(face) = face else { return Ok(()) };
    crate::render::draw_font_glyphs(emu, ctx, &face, &f, &list)
}

fn copy_table(emu: &mut Emu, src: u64, tag: u32) -> Result<u64> {
    let Some(f) = font(emu, src) else { return Ok(0) };
    let bytes = with_face(emu, &f, |p| p.raw_face().table(ttf_parser::Tag(tag)).map(<[u8]>::to_vec)).flatten();
    match bytes {
        Some(b) => foundation::new_data(emu, &b),
        None => Ok(0),
    }
}

fn table_tags(emu: &mut Emu, f: &Font) -> Vec<u32> {
    let face = faces(emu).get(f.face).cloned();
    let Some(face) = face else { return Vec::new() };
    let b = &face.bytes;
    // The sfnt table directory (a single face; collections start with 'ttcf').
    let base = if b.starts_with(b"ttcf") { u32::from_be_bytes(b[12..16].try_into().unwrap_or_default()) as usize } else { 0 };
    let n = b.get(base + 4..base + 6).map_or(0, |x| u16::from_be_bytes([x[0], x[1]]) as usize);
    (0..n).filter_map(|i| b.get(base + 12 + i * 16..base + 16 + i * 16).map(|t| u32::from_be_bytes(t.try_into().unwrap()))).collect()
}

fn names(emu: &mut Emu, src: u64) -> Option<Face> {
    let f = font(emu, src)?;
    faces(emu).get(f.face).cloned()
}

fn ret_name(emu: &mut Emu, which: fn(&Face) -> &str) -> Result<()> {
    let Some(face) = names(emu, emu.arg(0)) else { return ret(emu, 0) };
    let s = foundation::new_nsstring(emu, which(&face))?;
    ret(emu, s)
}

fn symbolic_traits(emu: &mut Emu, f: &Font) -> u32 {
    let color = faces(emu).get(f.face).is_some_and(|x| x.color);
    let mut t = 0;
    if f.weight >= 0.3 {
        t |= 1 << 1; // bold
    }
    if color {
        t |= 1 << 13; // kCTFontTraitColorGlyphs
    }
    t
}

fn traits_dict(emu: &mut Emu, f: &Font) -> Result<u64> {
    let sym = symbolic_traits(emu, f);
    let mut pairs = Vec::new();
    for (k, v) in [("kCTFontWeightTrait", f.weight), ("kCTFontWidthTrait", 0.0), ("kCTFontSlantTrait", 0.0)] {
        let k = foundation::new_nsstring(emu, k)?;
        let v = foundation::new_number_f64(emu, v)?;
        pairs.push((k, v));
    }
    let k = foundation::new_nsstring(emu, "kCTFontSymbolicTrait")?;
    let v = foundation::new_number_i64(emu, sym as i64)?;
    pairs.push((k, v));
    foundation::new_dict_of(emu, &pairs)
}

/// An attribute of a font (or a face-backed descriptor) by its key constant.
fn attribute(emu: &mut Emu, f: &Font, key: &str) -> Result<u64> {
    let Some(face) = faces(emu).get(f.face).cloned() else { return Ok(0) };
    Ok(match key {
        k if k.ends_with("FamilyNameAttribute") => foundation::new_nsstring(emu, &face.family)?,
        "kCTFontNameAttribute" => foundation::new_nsstring(emu, &face.postscript)?,
        k if k.ends_with("DisplayNameAttribute") => foundation::new_nsstring(emu, &face.full)?,
        k if k.ends_with("StyleNameAttribute") => foundation::new_nsstring(emu, &face.style)?,
        k if k.ends_with("TraitsAttribute") => traits_dict(emu, f)?,
        k if k.ends_with("SizeAttribute") => foundation::new_number_f64(emu, f.size)?,
        k if k.ends_with("FormatAttribute") => foundation::new_number_i64(emu, 2)?, // OpenType TrueType
        _ => 0,
    })
}

/// `CTFontCopyCharacterSet`: the cmap's characters.
fn character_set(emu: &mut Emu) -> Result<()> {
    let Some(f) = font(emu, emu.arg(0)) else { return ret(emu, 0) };
    let mut cps: Vec<u32> = with_face(emu, &f, |p| {
        let mut v = Vec::new();
        if let Some(cmap) = p.tables().cmap {
            for sub in cmap.subtables {
                if sub.is_unicode() {
                    sub.codepoints(|c| v.push(c));
                }
            }
        }
        v
    })
    .unwrap_or_default();
    cps.sort_unstable();
    cps.dedup();
    let set = foundation::cf_call(emu, "_CFCharacterSetCreateMutable", &[0])?;
    let mut i = 0;
    while i < cps.len() {
        let start = cps[i];
        let mut end = start;
        while i + 1 < cps.len() && cps[i + 1] == end + 1 {
            i += 1;
            end = cps[i];
        }
        foundation::cf_call(emu, "_CFCharacterSetAddCharactersInRange", &[set, start as u64, (end - start + 1) as u64])?;
        i += 1;
    }
    ret(emu, set)
}

/// `CTFontManagerCreateFontDescriptorFromData(data)`: an app's own font.
fn descriptor_from_data(emu: &mut Emu) -> Result<()> {
    let data = emu.arg(0);
    let bytes = foundation::data_bytes(emu, data)?;
    let Ok(parsed) = ttf_parser::Face::parse(&bytes, 0) else { return ret(emu, 0) };
    let name = |id: u16| {
        parsed.names().into_iter().find(|n| n.name_id == id && n.is_unicode()).and_then(|n| n.to_string()).unwrap_or_default()
    };
    let (family, style, full, ps) = (name(1), name(2), name(4), name(6));
    let variable = parsed.variation_axes().into_iter().any(|a| a.tag == ttf_parser::Tag::from_bytes(b"wght"));
    faces(emu);
    emu.coretext.faces.push(Face { family, postscript: ps, full, style, bytes: Rc::new(bytes), index: 0, color: false, variable_weight: variable });
    let i = emu.coretext.faces.len() - 1;
    let d = face_descriptor(emu, i)?;
    ret(emu, d)
}

fn all_descriptors(emu: &mut Emu) -> Result<u64> {
    let n = faces(emu).len();
    let mut v = Vec::new();
    for i in 0..n {
        v.push(face_descriptor(emu, i)?);
    }
    foundation::new_array_of(emu, &v)
}

/// The font a descriptor (made for a face, or by attributes) stands for.
fn descriptor_font(emu: &mut Emu, desc: u64) -> Result<Option<Font>> {
    let (face, weight, size) = resolve(emu, desc)?;
    Ok(face.map(|face| Font { face, size: if size > 0.0 { size } else { 12.0 }, matrix: IDENTITY, weight }))
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CTFontGetTypeID", |e| ret(e, CTFONT_TYPE_ID)),
    ("_CTFontDescriptorGetTypeID", |e| ret(e, CTFONT_DESCRIPTOR_TYPE_ID)),
    ("_CTFontCollectionCreateFromAvailableFonts", |e| {
        let a = all_descriptors(e)?;
        ret(e, a)
    }),
    ("_CTFontCollectionCreateMatchingFontDescriptors", |e| {
        let a = all_descriptors(e)?;
        ret(e, a)
    }),
    ("_CTFontCollectionCreateMatchingFontDescriptorsForFamily", |e| {
        let a = all_descriptors(e)?;
        ret(e, a)
    }),
    ("_CTFontCollectionCopyFontAttribute", |e| {
        let a = foundation::new_array_of(e, &[])?;
        ret(e, a)
    }),
    ("_CTFontManagerCopyAvailableFontFamilyNames", |e| {
        let n = faces(e).len();
        let mut v = Vec::new();
        for i in 0..n {
            let fam = e.coretext.faces[i].family.clone();
            v.push(foundation::new_nsstring(e, &fam)?);
        }
        let a = foundation::new_array_of(e, &v)?;
        ret(e, a)
    }),
    ("_CTFontManagerCreateFontDescriptorFromData", descriptor_from_data),
    ("_CTFontManagerRegisterGraphicsFont", |e| ret(e, 1)),
    ("_CTFontManagerRegisterFontsForURL", |e| ret(e, 1)),
    ("_CTFontDescriptorCreateWithAttributes", |e| {
        let attrs = e.arg(0);
        let d = make_descriptor(e, attrs, None)?;
        ret(e, d)
    }),
    ("_CTFontDescriptorCreateWithNameAndSize", |e| {
        let name = nsstring(e, e.arg(0))?;
        let face = face_named(e, &name);
        let d = match face {
            Some(f) => face_descriptor(e, f)?,
            None => make_descriptor(e, 0, None)?,
        };
        ret(e, d)
    }),
    ("_CTFontDescriptorCopyAttribute", |e| {
        let (desc, key) = (e.arg(0), e.arg(1));
        let key = nsstring(e, key)?;
        let (attrs, _) = e.coretext.descriptors.get(&desc).cloned().unwrap_or((0, None));
        let v = dict_get(e, attrs, &key)?;
        if v != 0 {
            objc::send(e, v, "retain", &[])?;
            return ret(e, v);
        }
        let v = match descriptor_font(e, desc)? {
            Some(f) => attribute(e, &f, &key)?,
            None => 0,
        };
        ret(e, v)
    }),
    ("_CTFontDescriptorCopyAttributes", |e| {
        let desc = e.arg(0);
        let (attrs, _) = e.coretext.descriptors.get(&desc).cloned().unwrap_or((0, None));
        let a = if attrs != 0 { attrs } else { foundation::new_dict_of(e, &[])? };
        objc::send(e, a, "retain", &[])?;
        ret(e, a)
    }),
    ("_CTFontDescriptorCreateMatchingFontDescriptor", |e| {
        let desc = e.arg(0);
        let (face, _, _) = resolve(e, desc)?;
        let d = match face {
            Some(f) => face_descriptor(e, f)?,
            None => 0,
        };
        ret(e, d)
    }),
    ("_CTFontDescriptorCreateMatchingFontDescriptors", |e| {
        let desc = e.arg(0);
        let (face, _, _) = resolve(e, desc)?;
        let a = match face {
            Some(f) => {
                let d = face_descriptor(e, f)?;
                foundation::new_array_of(e, &[d])?
            }
            None => 0,
        };
        ret(e, a)
    }),
    ("_CTFontCreateWithFontDescriptor", create_with_descriptor),
    ("_CTFontCreateWithFontDescriptorAndOptions", create_with_descriptor),
    ("_CTFontCreateWithName", |e| {
        let (name, matrix) = (nsstring(e, e.arg(0))?, e.arg(1));
        let face = face_named(e, &name).unwrap_or(0);
        let size = d(e, 0);
        let matrix = read_affine(e, matrix)?.unwrap_or(IDENTITY);
        let f = make_font(e, Font { face, size: if size > 0.0 { size } else { 12.0 }, matrix, weight: 0.0 })?;
        ret(e, f)
    }),
    ("_CTFontCreateUIFontForLanguage", create_ui_font),
    ("_CTFontCreateCopyWithAttributes", create_copy_with_attributes),
    ("_CTFontCreateCopyWithSymbolicTraits", create_copy_with_traits),
    ("_CTFontCreateCopyWithFamily", |e| {
        let (src, matrix, family) = (e.arg(0), e.arg(1), e.arg(2));
        let size = d(e, 0);
        let Some(mut f) = font(e, src) else { return ret(e, 0) };
        let name = nsstring(e, family)?;
        let Some(face) = face_named(e, &name) else { return ret(e, 0) };
        f.face = face;
        if size > 0.0 {
            f.size = size;
        }
        if let Some(m) = read_affine(e, matrix)? {
            f.matrix = m;
        }
        let o = make_font(e, f)?;
        ret(e, o)
    }),
    ("_CTFontCreateForString", create_for_string),
    ("_CTFontCreateForStringWithLanguage", create_for_string),
    ("_CTFontCopyFontDescriptor", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 0) };
        let d = face_descriptor(e, f.face)?;
        ret(e, d)
    }),
    ("_CTFontCopyAttribute", |e| {
        let (src, key) = (e.arg(0), e.arg(1));
        let key = nsstring(e, key)?;
        let Some(f) = font(e, src) else { return ret(e, 0) };
        let v = attribute(e, &f, &key)?;
        ret(e, v)
    }),
    ("_CTFontCopyTraits", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 0) };
        let t = traits_dict(e, &f)?;
        ret(e, t)
    }),
    ("_CTFontCopyFamilyName", |e| ret_name(e, |f| &f.family)),
    ("_CTFontCopyPostScriptName", |e| ret_name(e, |f| &f.postscript)),
    ("_CTFontCopyFullName", |e| ret_name(e, |f| &f.full)),
    ("_CTFontCopyDisplayName", |e| ret_name(e, |f| &f.full)),
    ("_CTFontCopyName", |e| {
        let key = nsstring(e, e.arg(1))?;
        let Some(face) = names(e, e.arg(0)) else { return ret(e, 0) };
        let v = if key.contains("Family") {
            face.family
        } else if key.contains("PostScript") {
            face.postscript
        } else if key.contains("Style") || key.contains("SubFamily") {
            face.style
        } else {
            face.full
        };
        let s = foundation::new_nsstring(e, &v)?;
        ret(e, s)
    }),
    ("_CTFontCopyLocalizedName", |e| {
        let (src, key, lang_out) = (e.arg(0), e.arg(1), e.arg(2));
        let key = nsstring(e, key)?;
        let Some(face) = names(e, src) else { return ret(e, 0) };
        let v = if key.contains("Family") {
            face.family
        } else if key.contains("PostScript") {
            face.postscript
        } else if key.contains("Style") || key.contains("SubFamily") {
            face.style
        } else {
            face.full
        };
        if lang_out != 0 {
            let l = foundation::new_nsstring(e, "en")?;
            e.write_u64(lang_out, l)?;
        }
        let s = foundation::new_nsstring(e, &v)?;
        ret(e, s)
    }),
    ("_CTFontGetSize", |e| {
        let s = font(e, e.arg(0)).map_or(0.0, |f| f.size);
        ret_f64(e, s)
    }),
    ("_CTFontGetMatrix", |e| {
        let m = font(e, e.arg(0)).map_or(IDENTITY, |f| f.matrix);
        let out = e.cpu.x[8];
        for (i, v) in m.iter().enumerate() {
            e.write_u64(out + i as u64 * 8, v.to_bits())?;
        }
        Ok(())
    }),
    ("_CTFontGetUnitsPerEm", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 1000) };
        let u = with_face(e, &f, |p| p.units_per_em() as u64).unwrap_or(1000);
        ret(e, u)
    }),
    ("_CTFontGetGlyphCount", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 0) };
        let n = with_face(e, &f, |p| p.number_of_glyphs() as u64).unwrap_or(0);
        ret(e, n)
    }),
    ("_CTFontGetSymbolicTraits", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 0) };
        let t = symbolic_traits(e, &f);
        ret(e, t as u64)
    }),
    ("_CTFontGetAscent", |e| metric(e, |p| p.ascender() as f64)),
    ("_CTFontGetDescent", |e| metric(e, |p| -(p.descender() as f64))),
    ("_CTFontGetLeading", |e| metric(e, |p| p.line_gap() as f64)),
    ("_CTFontGetCapHeight", |e| metric(e, |p| p.capital_height().unwrap_or(p.ascender()) as f64)),
    ("_CTFontGetXHeight", |e| metric(e, |p| p.x_height().unwrap_or(p.ascender() / 2) as f64)),
    ("_CTFontGetUnderlinePosition", |e| metric(e, |p| p.underline_metrics().map_or(-100, |m| m.position) as f64)),
    ("_CTFontGetUnderlineThickness", |e| metric(e, |p| p.underline_metrics().map_or(50, |m| m.thickness) as f64)),
    ("_CTFontGetSlantAngle", |e| ret_f64(e, 0.0)),
    ("_CTFontGetBoundingBox", bounding_box),
    ("_CTFontGetGlyphsForCharacters", glyphs_for_characters),
    ("_CTFontGetAdvancesForGlyphs", advances),
    ("_CTFontGetBoundingRectsForGlyphs", bounding_rects),
    ("_CTFontCreatePathForGlyph", create_path_for_glyph),
    ("_CTFontDrawGlyphs", draw_glyphs),
    ("_CTFontCopyCharacterSet", character_set),
    ("_CTFontCopyTable", |e| {
        let (src, tag) = (e.arg(0), e.arg(1) as u32);
        let t = copy_table(e, src, tag)?;
        ret(e, t)
    }),
    ("_CTFontCopyAvailableTables", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 0) };
        let tags = table_tags(e, &f);
        // Tags stored as raw values (no callbacks), as CoreText does.
        let buf = e.alloc((tags.len().max(1) * 8) as u64)?;
        for (i, t) in tags.iter().enumerate() {
            e.write_u64(buf + i as u64 * 8, *t as u64)?;
        }
        let a = foundation::cf_call(e, "_CFArrayCreate", &[0, buf, tags.len() as u64, 0])?;
        ret(e, a)
    }),
    // No variation axes are exposed: the system font's weight is the font's own.
    ("_CTFontCopyVariationAxes", |e| ret(e, 0)),
    ("_CTFontCopyVariation", |e| ret(e, 0)),
    // CGFont: the same objects as CTFonts.
    ("_CTFontCopyGraphicsFont", |e| {
        let src = e.arg(0);
        objc::send(e, src, "retain", &[])?;
        ret(e, src)
    }),
    ("_CGFontCopyTableForTag", |e| {
        let (src, tag) = (e.arg(0), e.arg(1) as u32);
        let t = copy_table(e, src, tag)?;
        ret(e, t)
    }),
    ("_CGFontCopyTableTags", |e| {
        let Some(f) = font(e, e.arg(0)) else { return ret(e, 0) };
        let tags = table_tags(e, &f);
        let buf = e.alloc((tags.len().max(1) * 8) as u64)?;
        for (i, t) in tags.iter().enumerate() {
            e.write_u64(buf + i as u64 * 8, *t as u64)?;
        }
        let a = foundation::cf_call(e, "_CFArrayCreate", &[0, buf, tags.len() as u64, 0])?;
        ret(e, a)
    }),
    ("_CGFontRelease", |_| Ok(())),
    ("_CGFontRetain", |e| {
        let f = e.arg(0);
        ret(e, f)
    }),
];
