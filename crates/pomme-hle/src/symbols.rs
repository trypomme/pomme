//! SF Symbols: `+[UIImage systemImageNamed:]` and
//! UIImageSymbolConfiguration. A symbol is an image at the size iOS gives
//! it, drawn as a template (in its view's tint). iOS's symbols can't ship
//! with pomme, so each is drawn with the Material Symbols Rounded glyph
//! that stands for it (symbols/map.txt; the font's FILL axis for `.fill`
//! names, its weight axis for the symbol's weight), fitted to the size iOS
//! has: each symbol's size and baseline at the small, medium and large
//! scales, recorded on the iOS 26 simulator (tests/fixtures/symbolmetrics),
//! scaling with the point size and growing with the weight as recorded there.
//! A name pomme has no stand-in for draws as an outlined rounded square.

use crate::objc::{self, bag_get, bag_set};
use crate::render::{Image, ImageLook};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;
use std::sync::OnceLock;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// A symbol configuration: what's unset (0, None) comes from the one it's
/// applied over, and finally the defaults (body, regular, medium).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    /// In points; 0: unset.
    pub point_size: f64,
    /// A UIFontTextStyle's name.
    pub text_style: Option<String>,
    /// UIImageSymbolWeight: 0 unspecified, ultraLight (1) … black (9).
    pub weight: i64,
    /// UIImageSymbolScale: 0 unspecified (and -1, default: medium), small (1), medium (2), large (3).
    pub scale: i64,
}

impl Config {
    /// `other` over this one.
    pub fn applying(&self, other: &Config) -> Config {
        // (A point size and a text style each replace the other.)
        let sized = other.point_size > 0.0 || other.text_style.is_some();
        Config {
            point_size: if sized { other.point_size } else { self.point_size },
            text_style: if sized { other.text_style.clone() } else { self.text_style.clone() },
            weight: if other.weight != 0 { other.weight } else { self.weight },
            scale: if other.scale > 0 { other.scale } else { self.scale },
        }
    }

    fn points(&self) -> f64 {
        if self.point_size > 0.0 {
            self.point_size
        } else {
            self.text_style.as_deref().map_or(17.0, |s| crate::uiview::text_style(s).0)
        }
    }

    /// OpenType weight (100 … 900).
    fn wght(&self) -> f64 {
        if (1..=9).contains(&self.weight) { self.weight as f64 * 100.0 } else { 400.0 }
    }
}

/// A symbol image: its name and configuration.
#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub config: Config,
}

/// Width, height and baseline (from the bottom), in points, at 17 pt regular.
type Metrics = [f64; 3];

/// Each recorded symbol's metrics at the small, medium and large scales.
fn recorded() -> &'static HashMap<String, [Metrics; 3]> {
    static TABLE: OnceLock<HashMap<String, [Metrics; 3]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: HashMap<String, [Metrics; 3]> = HashMap::new();
        for line in include_str!("../../pomme-cli/tests/fixtures/symbolmetrics/ios26.txt").lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let slot = match f.first() {
                Some(&"SMALL") => 0,
                Some(&"SYMBOL") => 1,
                Some(&"LARGE") => 2,
                _ => continue,
            };
            let Some(m) = f.get(2..5).and_then(|v| v.iter().map(|x| x.parse().ok()).collect::<Option<Vec<f64>>>()) else { continue };
            t.entry(f[1].to_string()).or_default()[slot] = [m[0], m[1], m[2]];
        }
        t
    })
}

/// SF Symbol name → (Material Symbols name, filled).
fn stand_ins() -> &'static HashMap<&'static str, (&'static str, bool)> {
    static MAP: OnceLock<HashMap<&'static str, (&'static str, bool)>> = OnceLock::new();
    MAP.get_or_init(|| {
        include_str!("symbols/map.txt")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                Some((*f.first()?, (*f.get(1)?, f.get(2) == Some(&"fill"))))
            })
            .collect()
    })
}

/// How much a symbol grows with its weight, at 17 pt (width, height, by
/// OpenType weight): star.fill's, recorded at ultraLight, regular, bold and
/// black, and in between proportionally.
fn weight_growth(wght: f64) -> (f64, f64) {
    const STOPS: [(f64, f64, f64); 4] = [(100.0, -1.0, -0.667), (400.0, 0.0, 0.0), (700.0, 1.0, 1.333), (900.0, 1.667, 2.0)];
    for p in STOPS.windows(2) {
        let ((a, wa, ha), (b, wb, hb)) = (p[0], p[1]);
        if wght <= b {
            let t = ((wght - a) / (b - a)).clamp(0.0, 1.0);
            return (wa + (wb - wa) * t, ha + (hb - ha) * t);
        }
    }
    (STOPS[3].1, STOPS[3].2)
}

/// The size (points) and baseline a symbol has with a configuration, on a
/// screen of `scale` pixels per point.
pub fn metrics(name: &str, config: &Config, scale: f64) -> Metrics {
    let slot = match config.scale {
        1 => 0,
        3 => 2,
        _ => 1,
    };
    // (A name without recorded metrics: the size of a square.)
    let base = recorded().get(name).or_else(|| recorded().get("square")).map_or([17.0, 17.0, 2.67], |m| m[slot]);
    let f = config.points() / 17.0;
    let (gw, gh) = weight_growth(config.wght());
    let snap = |v: f64| (v * scale).round() / scale;
    // (A heavier symbol grows about its middle: its baseline drops by half its growth.)
    [snap((base[0] + gw) * f), snap((base[1] + gh) * f), snap((base[2] + gh / 2.0) * f)]
}

/// Glyph outlines as a path.
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

/// The Material symbol's glyph, as a path in font units (y up), at a weight
/// and fill.
fn glyph_path(emu: &mut Emu, material: &str, wght: f64, fill: bool, points: f64) -> Option<tiny_skia::Path> {
    let codes = crate::textkit::system_font(emu, "MaterialSymbolsRounded.codepoints")?;
    let font = crate::textkit::system_font(emu, "MaterialSymbolsRounded.ttf")?;
    let code = std::str::from_utf8(&codes).ok()?.lines().find_map(|l| {
        let (n, c) = l.split_once(' ')?;
        (n == material).then(|| u32::from_str_radix(c.trim(), 16).ok()).flatten()
    })?;
    let mut face = ttf_parser::Face::parse(&font, 0).ok()?;
    face.set_variation(ttf_parser::Tag::from_bytes(b"FILL"), if fill { 1.0 } else { 0.0 });
    // (Material's strokes are heavier than SF's at the same weight: SF's
    // ultraLight … black over Material's 100 … 700, regular at 325.)
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), (100.0 + (wght - 100.0) * 0.75).clamp(100.0, 700.0) as f32);
    face.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), points.clamp(20.0, 48.0) as f32);
    let gid = face.glyph_index(char::from_u32(code)?)?;
    let mut b = Outline(PathBuilder::new());
    face.outline_glyph(gid, &mut b)?;
    b.0.finish()
}

/// A symbol's pixels: its glyph fitted to its size, centered, in black (a
/// template's alpha).
pub(crate) fn draw(emu: &mut Emu, name: &str, config: &Config, size: [f64; 2], scale: f64) -> Option<Pixmap> {
    let (w, h) = ((size[0] * scale).round().max(1.0) as u32, (size[1] * scale).round().max(1.0) as u32);
    let mut pix = Pixmap::new(w, h)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    let glyph = stand_ins().get(name).and_then(|&(m, fill)| glyph_path(emu, m, config.wght(), fill, config.points()));
    match glyph {
        Some(path) => {
            let b = path.bounds();
            let s = (w as f32 / b.width()).min(h as f32 / b.height());
            // (Font units are y-up: flipped, then centered.)
            let tx = (w as f32 - b.width() * s) / 2.0 - b.left() * s;
            let ty = (h as f32 - b.height() * s) / 2.0 + b.bottom() * s;
            pix.fill_path(&path, &paint, FillRule::Winding, Transform::from_row(s, 0.0, 0.0, -s, tx, ty), None);
        }
        None => {
            // No stand-in: an outlined rounded square.
            let line = (config.points() * scale / 12.0) as f32;
            let inset = line / 2.0 + 1.0;
            let r = tiny_skia::Rect::from_xywh(inset, inset, w as f32 - 2.0 * inset, h as f32 - 2.0 * inset)?;
            let path = PathBuilder::from_rect(r);
            pix.stroke_path(&path, &paint, &Stroke { width: line, ..Default::default() }, Transform::identity(), None);
        }
    }
    Some(pix)
}

/// A symbol image (a UIImage), or 0 for none.
pub fn image(emu: &mut Emu, name: &str, config: Config) -> Result<u64> {
    if name.is_empty() {
        return Ok(0);
    }
    let scale = emu.device.scale;
    let m = metrics(name, &config, scale);
    let Some(pix) = draw(emu, name, &config, [m[0], m[1]], scale) else { return Ok(0) };
    let look = ImageLook { baseline: Some(m[2]), symbol: Some(Symbol { name: name.to_string(), config }), ..Default::default() };
    crate::image::new_image(emu, Image { pix: std::rc::Rc::new(pix), size: [m[0], m[1]], scale, caps: [0.0; 4], orientation: 0, look })
}

// ---- UIImageSymbolConfiguration ----

const CFG: &str = "UIImageSymbolConfiguration";

/// A configuration object's settings.
pub fn config_of(emu: &mut Emu, c: u64) -> Result<Config> {
    if c == 0 {
        return Ok(Config::default());
    }
    let style = bag_get(emu, c, "textStyle").unwrap_or(0);
    Ok(Config {
        point_size: f64::from_bits(bag_get(emu, c, "pointSize").unwrap_or(0)),
        text_style: if style == 0 { None } else { Some(crate::foundation::nsstring(emu, style)?) },
        weight: bag_get(emu, c, "weight").unwrap_or(0) as i64,
        scale: bag_get(emu, c, "scale").map_or(0, |s| s as i64),
    })
}

fn new_config(emu: &mut Emu, config: &Config) -> Result<u64> {
    let cls = objc::host_class(emu, CFG)?;
    let c = objc::create_instance(emu, cls, 0)?;
    bag_set(emu, c, "pointSize", config.point_size.to_bits());
    if let Some(s) = &config.text_style {
        let s = crate::foundation::new_nsstring(emu, s)?;
        bag_set(emu, c, "textStyle", s);
    }
    bag_set(emu, c, "weight", config.weight as u64);
    bag_set(emu, c, "scale", config.scale as u64);
    Ok(c)
}

fn ret_config(emu: &mut Emu, config: Config) -> Result<()> {
    let c = new_config(emu, &config)?;
    emu.ret(c);
    Ok(())
}

fn d0(emu: &Emu) -> f64 {
    f64::from_bits(emu.cpu.q[0] as u64)
}

/// The symbol weight nearest a UIFont weight (-0.8 … 0.62).
fn weight_of_font(w: f64) -> i64 {
    (crate::textkit::ot_weight(w) / 100.0).round().clamp(1.0, 9.0) as i64
}

/// A UIFont's configuration: its size and weight.
fn font_config(emu: &mut Emu, font: u64, scale: i64) -> Result<Config> {
    let size = f64::from_bits(emu.read_u64(font + 16)?);
    let weight = f64::from_bits(emu.read_u64(font + 24)?);
    Ok(Config { point_size: size, weight: weight_of_font(weight), scale, ..Default::default() })
}

fn text_style_config(emu: &mut Emu, style: u64, scale: i64) -> Result<Config> {
    let s = if style == 0 { None } else { Some(crate::foundation::nsstring(emu, style)?) };
    Ok(Config { text_style: s, scale, ..Default::default() })
}

/// A UIImage's symbol, if it is one.
fn symbol_of(emu: &Emu, img: u64) -> Option<Symbol> {
    emu.render.images.get(&img).and_then(|i| i.look.symbol.clone())
}

/// `systemImageNamed:` and its variants: the name, and a configuration (x3) if `configured`.
fn system_image(emu: &mut Emu, configured: bool) -> Result<()> {
    let (name, cfg) = (emu.arg(2), if configured { emu.arg(3) } else { 0 });
    let name = if name == 0 { String::new() } else { crate::foundation::nsstring(emu, name)? };
    let config = config_of(emu, cfg)?;
    let img = image(emu, &name, config)?;
    emu.ret(img);
    Ok(())
}

/// The symbol with `cfg` applied (`imageWithConfiguration:`); another image as it is.
fn with_config(emu: &mut Emu) -> Result<()> {
    let (img, cfg) = (emu.arg(0), emu.arg(2));
    let Some(sym) = symbol_of(emu, img) else {
        emu.ret(img);
        return Ok(());
    };
    let config = sym.config.applying(&config_of(emu, cfg)?);
    let look = emu.render.images.get(&img).map(|i| i.look.clone()).unwrap_or_default();
    let out = image(emu, &sym.name, config)?;
    if let Some(i) = emu.render.images.get_mut(&out) {
        i.look.rendering_mode = look.rendering_mode;
        i.look.tint = look.tint;
    }
    emu.ret(out);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIImage", true, "systemImageNamed:", |e| system_image(e, false)),
    ("UIImage", true, "systemImageNamed:withConfiguration:", |e| system_image(e, true)),
    ("UIImage", true, "systemImageNamed:compatibleWithTraitCollection:", |e| system_image(e, false)),
    ("UIImage", true, "_systemImageNamed:", |e| system_image(e, false)),
    ("UIImage", false, "imageWithConfiguration:", with_config),
    ("UIImage", false, "imageByApplyingSymbolConfiguration:", with_config),
    ("UIImage", false, "symbolConfiguration", |e| {
        let config = symbol_of(e, e.arg(0)).map(|s| s.config);
        match config {
            Some(c) => ret_config(e, c),
            None => {
                e.ret(0);
                Ok(())
            }
        }
    }),
    ("UIImage", false, "configuration", |e| {
        let config = symbol_of(e, e.arg(0)).map(|s| s.config).unwrap_or_default();
        ret_config(e, config)
    }),
    ("UIImage", false, "isSymbolImage", |e| {
        let is = symbol_of(e, e.arg(0)).is_some();
        e.ret(is as u64);
        Ok(())
    }),
    ("UIImage", false, "hasBaseline", |e| {
        let has = e.render.images.get(&e.arg(0)).is_some_and(|i| i.look.baseline.is_some());
        e.ret(has as u64);
        Ok(())
    }),
    ("UIImage", false, "baselineOffsetFromBottom", |e| {
        let b = e.render.images.get(&e.arg(0)).and_then(|i| i.look.baseline).unwrap_or(0.0);
        crate::uikit::ret_f64(e, b);
        Ok(())
    }),
    (CFG, true, "unspecifiedConfiguration", |e| ret_config(e, Config::default())),
    (CFG, true, "configurationWithPointSize:", |e| {
        let p = d0(e);
        ret_config(e, Config { point_size: p, ..Default::default() })
    }),
    (CFG, true, "configurationWithPointSize:weight:", |e| {
        let (p, w) = (d0(e), e.arg(2) as i64);
        ret_config(e, Config { point_size: p, weight: w, ..Default::default() })
    }),
    (CFG, true, "configurationWithPointSize:weight:scale:", |e| {
        let (p, w, s) = (d0(e), e.arg(2) as i64, e.arg(3) as i64);
        ret_config(e, Config { point_size: p, weight: w, scale: s, ..Default::default() })
    }),
    (CFG, true, "configurationWithWeight:", |e| {
        let w = e.arg(2) as i64;
        ret_config(e, Config { weight: w, ..Default::default() })
    }),
    (CFG, true, "configurationWithScale:", |e| {
        let s = e.arg(2) as i64;
        ret_config(e, Config { scale: s, ..Default::default() })
    }),
    (CFG, true, "configurationWithTextStyle:", |e| {
        let c = text_style_config(e, e.arg(2), 0)?;
        ret_config(e, c)
    }),
    (CFG, true, "configurationWithTextStyle:scale:", |e| {
        let s = e.arg(3) as i64;
        let c = text_style_config(e, e.arg(2), s)?;
        ret_config(e, c)
    }),
    (CFG, true, "configurationWithFont:", |e| {
        let c = font_config(e, e.arg(2), 0)?;
        ret_config(e, c)
    }),
    (CFG, true, "configurationWithFont:scale:", |e| {
        let s = e.arg(3) as i64;
        let c = font_config(e, e.arg(2), s)?;
        ret_config(e, c)
    }),
    (CFG, false, "configurationByApplyingConfiguration:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let c = config_of(e, a)?.applying(&config_of(e, b)?);
        ret_config(e, c)
    }),
    (CFG, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = b != 0 && config_of(e, a)? == config_of(e, b)?;
        e.ret(same as u64);
        Ok(())
    }),
    (CFG, false, "pointSize", |e| {
        let p = config_of(e, e.arg(0))?.point_size;
        crate::uikit::ret_f64(e, p);
        Ok(())
    }),
    (CFG, false, "weight", |e| {
        let w = config_of(e, e.arg(0))?.weight;
        e.ret(w as u64);
        Ok(())
    }),
    (CFG, false, "scale", |e| {
        let s = config_of(e, e.arg(0))?.scale;
        e.ret(s as u64);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stand_in_has_ios_metrics() {
        for name in stand_ins().keys() {
            assert!(recorded().get(*name).is_some_and(|m| m.iter().all(|x| x[0] > 0.0)), "no recorded size for {name}");
        }
    }

    #[test]
    fn sizes_follow_the_configuration() {
        let star = |c: Config| metrics("star.fill", &c, 3.0);
        assert_eq!(star(Config::default()), [22.0, 20.0, 10.0 / 3.0]);
        assert_eq!(star(Config { scale: 3, ..Default::default() })[..2], [28.0, 80.0 / 3.0]);
        let big = star(Config { point_size: 34.0, ..Default::default() });
        assert!((big[0] - 44.0).abs() < 0.5 && (big[1] - 40.33).abs() < 0.5, "{big:?}");
        let bold = star(Config { weight: 7, ..Default::default() });
        assert!((bold[0] - 23.0).abs() < 0.4 && (bold[1] - 21.33).abs() < 0.4, "{bold:?}");
        let title = star(Config { text_style: Some("UIFontTextStyleTitle1".into()), ..Default::default() });
        assert!((title[0] - 36.33).abs() < 0.5 && (title[1] - 33.33).abs() < 0.5, "{title:?}");
    }
}
