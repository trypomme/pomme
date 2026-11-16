//! UILabel: text (plain or attributed) in one font and color, wrapped at the
//! label's width up to `numberOfLines` (1 by default, 0 for no limit),
//! vertically centered in its bounds. React Native draws its own text, but
//! uses a UILabel for a multiline TextInput's placeholder.

use crate::foundation;
use crate::objc::{self, send};
use crate::textkit::{self, Line, Run};
use crate::uikit::ret_rect;
use crate::{Emu, HostFn, Result};

const DEFAULT_SIZE: f64 = 17.0;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

pub fn is_label(emu: &mut Emu, v: u64) -> bool {
    let Ok(cls) = objc::host_class(emu, "UILabel") else { return false };
    objc::is_kind_of(emu, v, cls).unwrap_or(false)
}

/// The label's text and the attributes it's drawn in (those of the
/// attributed text's first character, or the label's font and color).
fn content(emu: &mut Emu, v: u64) -> Result<(String, u64)> {
    if let Some(a) = objc::bag_get(emu, v, "attributedText").filter(|&a| a != 0) {
        let s = send(emu, a, "string", &[])?;
        let text = foundation::nsstring(emu, s)?;
        let attrs = if text.is_empty() { 0 } else { send(emu, a, "attributesAtIndex:effectiveRange:", &[0, 0])? };
        return Ok((text, attrs));
    }
    let text = match objc::bag_get(emu, v, "text").filter(|&s| s != 0) {
        Some(s) => foundation::nsstring(emu, s)?,
        None => String::new(),
    };
    Ok((text, 0))
}

pub struct LabelText {
    pub text: Vec<u16>,
    pub lines: Vec<Line>,
    pub font: textkit::TextFont,
    pub color: [f64; 4],
    /// NSTextAlignment: 1 center, 2 right; anything else is left.
    pub alignment: u64,
}

impl LabelText {
    pub fn height(&self) -> f64 {
        self.lines.last().map_or(0.0, |l| l.y + l.height)
    }
}

/// The label's text laid out at `width`.
pub fn lay_out(emu: &mut Emu, v: u64, width: f64) -> Result<LabelText> {
    let (text, attrs) = content(emu, v)?;
    let mut font = textkit::font_in_attributes(emu, attrs)?;
    if font == 0 {
        font = objc::bag_get(emu, v, "font").unwrap_or(0);
    }
    let f = if font == 0 { textkit::TextFont::system(DEFAULT_SIZE, 0.0) } else { textkit::font_info(emu, font)? };
    let mut color = crate::appearance::label(emu);
    let mut c = 0;
    if attrs != 0 {
        let key = foundation::new_nsstring(emu, "NSForegroundColorAttributeName")?;
        c = send(emu, attrs, "objectForKey:", &[key])?;
    }
    if c == 0 {
        c = objc::bag_get(emu, v, "textColor").unwrap_or(0);
    }
    if let Some(rgba) = emu.ui.views.colors.get(&c) {
        color = *rgba;
    }
    let max_lines = objc::bag_get(emu, v, "numberOfLines").unwrap_or(1) as usize;
    let text: Vec<u16> = text.encode_utf16().collect();
    let runs = [Run::new(emu, text.len(), f)];
    // (A label sets its text as CoreText does.)
    let shaped = textkit::shaped_advances(emu, &text, &runs);
    let lines = textkit::layout(&text, &runs, |u, _, _| shaped[u], width, f64::INFINITY, max_lines);
    // (Natural by default, as drawn in the view's direction.)
    let alignment = objc::bag_get(emu, v, "textAlignment").unwrap_or(4) & 0xff;
    let alignment = crate::direction::resolve_alignment(emu, v, alignment)?;
    Ok(LabelText { text, lines, font: f, color, alignment })
}

fn size_that_fits(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let w = d(emu, 0);
    let w = if w.is_finite() && w > 0.0 { w } else { f64::INFINITY };
    let t = lay_out(emu, v, w)?;
    let used = t.lines.iter().map(|l| l.width).fold(0.0, f64::max);
    ret_rect(emu, [used.ceil(), t.height().ceil(), 0.0, 0.0]);
    Ok(())
}

fn intrinsic_size(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let t = lay_out(emu, v, f64::INFINITY)?;
    let used = t.lines.iter().map(|l| l.width).fold(0.0, f64::max);
    ret_rect(emu, [used.ceil(), t.height().ceil(), 0.0, 0.0]);
    Ok(())
}

/// Store an object property, retained.
fn set_object(emu: &mut Emu, key: &str) -> Result<()> {
    let (v, x) = (emu.arg(0), emu.arg(2));
    if x != 0 {
        send(emu, x, "retain", &[])?;
    }
    objc::bag_set(emu, v, key, x);
    crate::uiview::view_mut(emu, v).needs_layout = true;
    crate::host::frame_changed(emu);
    Ok(())
}

const L: &str = "UILabel";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (L, false, "text", |e| {
        let v = e.arg(0);
        let (text, _) = content(e, v)?;
        let had = objc::bag_get(e, v, "attributedText").is_some_and(|a| a != 0) || objc::bag_get(e, v, "text").is_some_and(|s| s != 0);
        if !had {
            return ret(e, 0);
        }
        let s = foundation::new_nsstring(e, &text)?;
        ret(e, s)
    }),
    // Setting either replaces the other.
    (L, false, "setText:", |e| {
        let v = e.arg(0);
        objc::bag_set(e, v, "attributedText", 0);
        let s = e.arg(2);
        let s = if s == 0 { 0 } else { send(e, s, "copy", &[])? };
        objc::bag_set(e, v, "text", s);
        crate::host::frame_changed(e);
        Ok(())
    }),
    (L, false, "attributedText", |e| {
        let a = objc::bag_get(e, e.arg(0), "attributedText").unwrap_or(0);
        ret(e, a)
    }),
    (L, false, "setAttributedText:", |e| {
        let v = e.arg(0);
        objc::bag_set(e, v, "text", 0);
        set_object(e, "attributedText")
    }),
    (L, false, "numberOfLines", |e| {
        let n = objc::bag_get(e, e.arg(0), "numberOfLines").unwrap_or(1);
        ret(e, n)
    }),
    (L, false, "sizeThatFits:", size_that_fits),
    (L, false, "intrinsicContentSize", intrinsic_size),
];

