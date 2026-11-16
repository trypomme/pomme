//! UITextView: multiline text editing (React Native's multiline TextInput is
//! one, RCTUITextView).
//!
//! Editing, the first responder and UITextInput are shared with UITextField
//! (textinput.rs, `Kind::View`): the delegate is asked
//! `textView:shouldChangeTextInRange:replacementText:`, then told
//! `textViewDidChangeSelection:` and `textViewDidChange:`, and
//! `UITextViewTextDidChangeNotification` is posted. Return types a newline.
//!
//! This module has what's particular to a text view: its text container
//! (`textContainerInset`, default 8 pt above and below, and the container's
//! 5 pt `lineFragmentPadding` at the sides), its typing attributes, its
//! layout (wrapped by textkit.rs at the container's width) and its scrolling:
//! the text view is a scroll view whose content is the laid-out text, and
//! typing scrolls to keep the caret in sight (`contentOffset` is the bounds'
//! origin, as for every UIScrollView).

use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::textkit::{self, Line, Run};
use crate::uikit::ret_rect;
use crate::uiview::{view, view_mut};
use crate::{Emu, HostFn, Result};

/// UITextView's default `textContainerInset` (top, left, bottom, right).
const DEFAULT_INSET: [f64; 4] = [8.0, 0.0, 8.0, 0.0];
/// UITextView's font when none is set.
const DEFAULT_FONT_SIZE: f64 = 12.0;
/// NSTextContainer's default `lineFragmentPadding`.
const DEFAULT_PADDING: f64 = 5.0;
const INSET_KEYS: [&str; 4] = ["insetTop", "insetLeft", "insetBottom", "insetRight"];

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

/// `editable` (YES unless the app turned it off).
pub(crate) fn is_editable(emu: &Emu, v: u64) -> bool {
    objc::bag_get(emu, v, "editable").is_none_or(|x| x & 0xff != 0)
}

pub(crate) fn inset(emu: &Emu, v: u64) -> [f64; 4] {
    let mut r = DEFAULT_INSET;
    for (i, k) in INSET_KEYS.iter().enumerate() {
        if let Some(x) = objc::bag_get(emu, v, k) {
            r[i] = f64::from_bits(x);
        }
    }
    r
}

/// The text container, made on first use (React Native sets its padding to 0).
fn text_container(emu: &mut Emu, v: u64) -> Result<u64> {
    if let Some(tc) = objc::bag_get(emu, v, "textContainer").filter(|&x| x != 0) {
        return Ok(tc);
    }
    let cls = host_class(emu, "NSTextContainer")?;
    let tc = send(emu, cls, "alloc", &[])?;
    let tc = send(emu, tc, "init", &[])?;
    send(emu, tc, "retain", &[])?;
    objc::bag_set(emu, v, "textContainer", tc);
    Ok(tc)
}

fn padding(emu: &mut Emu, v: u64) -> Result<f64> {
    match objc::bag_get(emu, v, "textContainer").filter(|&x| x != 0) {
        Some(tc) => Ok(f64::from_bits(emu.read_u64(tc + 24)?)),
        None => Ok(DEFAULT_PADDING),
    }
}

/// `typingAttributes`: what the app set, else the view's font (12 pt system
/// unless set) and text color.
fn typing_attributes(emu: &mut Emu, v: u64) -> Result<u64> {
    if let Some(a) = objc::bag_get(emu, v, "typingAttributes").filter(|&a| a != 0) {
        return Ok(a);
    }
    let font = match objc::bag_get(emu, v, "font").filter(|&x| x != 0) {
        Some(x) => x,
        None => crate::uiview::new_font(emu, ".SFUI-Regular", DEFAULT_FONT_SIZE, 0.0)?,
    };
    let mut pairs = vec![(foundation::new_nsstring(emu, "NSFontAttributeName")?, font)];
    if let Some(c) = objc::bag_get(emu, v, "textColor").filter(|&x| x != 0) {
        pairs.push((foundation::new_nsstring(emu, "NSForegroundColorAttributeName")?, c));
    }
    foundation::new_dict_of(emu, &pairs)
}

/// A text view's text, laid out in its text container.
pub struct Laid {
    pub text: Vec<u16>,
    pub lines: Vec<Line>,
    pub font: textkit::TextFont,
    pub color: [f64; 4],
    /// NSTextAlignment: 1 center, 2 right; anything else is left.
    pub alignment: u64,
    /// Where the text container's lines start, in the view's content.
    pub origin: (f64, f64),
    /// The container's width (where lines wrap).
    pub width: f64,
    /// Height of the text (at least one line).
    pub text_height: f64,
    pub line_height: f64,
    pub inset: [f64; 4],
}

impl Laid {
    /// The content's size: the view's width by the text and the insets.
    pub fn content_height(&self) -> f64 {
        self.inset[0] + self.text_height + self.inset[2]
    }

    /// The caret's line for a UTF-16 offset: (top, height) in the text.
    pub fn caret_line(&self, offset: usize) -> (f64, f64) {
        let ends_in_newline = self.text.last().is_some_and(|&c| c == '\n' as u16 || c == '\r' as u16);
        if self.lines.is_empty() || (offset >= self.text.len() && ends_in_newline) {
            // On the empty line after the last newline (or the only line).
            return (self.text_height - self.line_height, self.line_height);
        }
        let l = self.lines.iter().find(|l| offset < l.start + l.len).unwrap_or_else(|| self.lines.last().unwrap());
        (l.y, l.height)
    }
}

pub fn lay_out(emu: &mut Emu, v: u64) -> Result<Laid> {
    let text = crate::textinput::text_units(emu, v);
    let attrs = typing_attributes(emu, v)?;
    let font = textkit::font_in_attributes(emu, attrs)?;
    let f = textkit::font_info(emu, font)?;
    let mut color = crate::appearance::label(emu);
    let key = foundation::new_nsstring(emu, "NSForegroundColorAttributeName")?;
    let c = send(emu, attrs, "objectForKey:", &[key])?;
    if let Some(rgba) = emu.ui.views.colors.get(&c) {
        color = *rgba;
    }
    let inset = inset(emu, v);
    let pad = padding(emu, v)?;
    let b = view(emu, v).bounds;
    let width = (b[2] - inset[1] - inset[3] - 2.0 * pad).max(0.0);
    let runs = [Run::new(emu, text.len(), f)];
    let lines = textkit::layout(&text, &runs, |ch, f| textkit::advance_in(emu, ch, f), width, f64::INFINITY, 0);
    let line_height = runs[0].line_height;
    let ends_in_newline = text.last().is_some_and(|&c| c == '\n' as u16 || c == '\r' as u16);
    let text_height = match lines.last() {
        None => line_height,
        Some(l) => l.y + l.height + if ends_in_newline { line_height } else { 0.0 },
    };
    let alignment = objc::bag_get(emu, v, "textAlignment").unwrap_or(4) & 0xff;
    let alignment = crate::direction::resolve_alignment(emu, v, alignment)?;
    Ok(Laid { text, lines, font: f, color, alignment, origin: (inset[1] + pad, inset[0]), width, text_height, line_height, inset })
}

/// Scroll the content to `y` (clamped to it), telling the delegate.
fn scroll_to(emu: &mut Emu, v: u64, y: f64) -> Result<()> {
    let vw = view_mut(emu, v);
    if (vw.bounds[1] - y).abs() < 1e-9 {
        return Ok(());
    }
    vw.bounds[1] = y;
    let delegate = objc::bag_get(emu, v, "delegate").unwrap_or(0);
    let sel = objc::intern(emu, "scrollViewDidScroll:")?;
    if delegate != 0 && objc::responds(emu, delegate, sel)? {
        send(emu, delegate, "scrollViewDidScroll:", &[v])?;
    }
    crate::host::frame_changed(emu);
    Ok(())
}

fn max_offset(emu: &mut Emu, v: u64, laid: &Laid) -> f64 {
    (laid.content_height() - view(emu, v).bounds[3]).max(0.0)
}

/// Scroll so the text at `offset` is in sight (`scrollRangeToVisible:`).
fn scroll_to_offset(emu: &mut Emu, v: u64, offset: usize) -> Result<()> {
    if objc::bag_get(emu, v, "scrollEnabled").is_some_and(|x| x & 0xff == 0) {
        return Ok(());
    }
    let laid = lay_out(emu, v)?;
    let (top, h) = laid.caret_line(offset);
    let b = view(emu, v).bounds;
    let (top, bottom) = (laid.inset[0] + top, laid.inset[0] + top + h + laid.inset[2]);
    let mut y = b[1];
    if bottom > y + b[3] {
        y = bottom - b[3];
    }
    if top - laid.inset[0] < y {
        y = top - laid.inset[0];
    }
    let y = y.clamp(0.0, max_offset(emu, v, &laid));
    scroll_to(emu, v, y)
}

/// After typing: keep the caret in sight.
pub(crate) fn scroll_to_caret(emu: &mut Emu, v: u64) -> Result<()> {
    let caret = crate::textinput::selection(emu, v).1;
    scroll_to_offset(emu, v, caret)
}

fn size_that_fits(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let (w, _) = (d(emu, 0), d(emu, 1));
    // Lay out at the width asked for, then answer the used size.
    let saved = view(emu, v).bounds;
    view_mut(emu, v).bounds[2] = if w.is_finite() && w > 0.0 && w < 1e7 { w } else { 1e7 };
    let laid = lay_out(emu, v);
    view_mut(emu, v).bounds = saved;
    let laid = laid?;
    let used = laid.lines.iter().map(|l| l.width).fold(0.0, f64::max);
    let fit_w = if w.is_finite() && w > 0.0 && w < 1e7 { w } else { used + laid.origin.0 * 2.0 };
    ret_rect(emu, [fit_w, laid.content_height().ceil(), 0.0, 0.0]);
    Ok(())
}

const TV: &str = "UITextView";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (TV, false, "isEditable", |e| {
        let v = is_editable(e, e.arg(0)) as u64;
        ret(e, v)
    }),
    (TV, false, "editable", |e| {
        let v = is_editable(e, e.arg(0)) as u64;
        ret(e, v)
    }),
    (TV, false, "setEditable:", |e| {
        let (v, x) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, v, "editable", x);
        if x == 0 && e.input.first_responder == v {
            send(e, v, "resignFirstResponder", &[])?;
        }
        Ok(())
    }),
    (TV, false, "isSelectable", |e| {
        let v = objc::bag_get(e, e.arg(0), "selectable").is_none_or(|x| x & 0xff != 0) as u64;
        ret(e, v)
    }),
    (TV, false, "font", |e| {
        let f = objc::bag_get(e, e.arg(0), "font").unwrap_or(0);
        ret(e, f)
    }),
    // Setting the font or color restyles the text, and typing goes on in it.
    (TV, false, "setFont:", |e| {
        let (v, f) = (e.arg(0), e.arg(2));
        objc::bag_set(e, v, "font", f);
        objc::bag_set(e, v, "typingAttributes", 0);
        Ok(())
    }),
    (TV, false, "textColor", |e| {
        let c = objc::bag_get(e, e.arg(0), "textColor").unwrap_or(0);
        ret(e, c)
    }),
    (TV, false, "setTextColor:", |e| {
        let (v, c) = (e.arg(0), e.arg(2));
        objc::bag_set(e, v, "textColor", c);
        objc::bag_set(e, v, "typingAttributes", 0);
        Ok(())
    }),
    (TV, false, "typingAttributes", |e| {
        let a = typing_attributes(e, e.arg(0))?;
        ret(e, a)
    }),
    (TV, false, "setTypingAttributes:", |e| {
        let (v, a) = (e.arg(0), e.arg(2));
        if a != 0 {
            send(e, a, "retain", &[])?;
        }
        objc::bag_set(e, v, "typingAttributes", a);
        Ok(())
    }),
    (TV, false, "textContainer", |e| {
        let tc = text_container(e, e.arg(0))?;
        ret(e, tc)
    }),
    (TV, false, "textContainerInset", |e| {
        let r = inset(e, e.arg(0));
        ret_rect(e, r);
        Ok(())
    }),
    (TV, false, "setTextContainerInset:", |e| {
        let v = e.arg(0);
        let r = [0, 1, 2, 3].map(|i| d(e, i));
        for (k, x) in INSET_KEYS.iter().zip(r) {
            objc::bag_set(e, v, k, x.to_bits());
        }
        view_mut(e, v).needs_layout = true;
        Ok(())
    }),
    // NSRange in x0 / x1.
    (TV, false, "selectedRange", |e| {
        let (a, b) = crate::textinput::selection(e, e.arg(0));
        e.ret(a as u64);
        e.cpu.x[1] = (b - a) as u64;
        Ok(())
    }),
    (TV, false, "setSelectedRange:", |e| {
        let (v, loc, len) = (e.arg(0), e.arg(2) as usize, e.arg(3) as usize);
        crate::textinput::set_selection(e, v, loc, loc.saturating_add(len));
        Ok(())
    }),
    (TV, false, "scrollRangeToVisible:", |e| {
        let (v, loc, len) = (e.arg(0), e.arg(2) as usize, e.arg(3) as usize);
        scroll_to_offset(e, v, loc.saturating_add(len))
    }),
    (TV, false, "contentSize", |e| {
        let v = e.arg(0);
        let laid = lay_out(e, v)?;
        let w = view(e, v).bounds[2];
        ret_rect(e, [w, laid.content_height(), 0.0, 0.0]);
        Ok(())
    }),
    (TV, false, "sizeThatFits:", size_that_fits),
    // Resized (React Native grows a multiline input with its text): the
    // content offset comes back within the content.
    (TV, false, "layoutSubviews", |e| {
        let v = e.arg(0);
        let laid = lay_out(e, v)?;
        let max = max_offset(e, v, &laid);
        let y = view(e, v).bounds[1];
        if y > max {
            scroll_to(e, v, max)?;
        }
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn laid(text: &str, lines: Vec<Line>) -> Laid {
        let line_height = 20.0;
        let text: Vec<u16> = text.encode_utf16().collect();
        let ends = text.last() == Some(&('\n' as u16));
        let text_height = lines.last().map_or(line_height, |l| l.y + l.height + if ends { line_height } else { 0.0 });
        Laid { text, lines, font: textkit::TextFont::system(17.0, 0.0), color: [0.0; 4], alignment: 0, origin: (5.0, 8.0), width: 100.0, text_height, line_height, inset: DEFAULT_INSET }
    }

    fn line(start: usize, len: usize, y: f64) -> Line {
        Line { start, len, y, height: 20.0, ..Default::default() }
    }

    #[test]
    fn the_caret_after_a_newline_is_on_a_new_line() {
        let l = laid("ab\n", vec![line(0, 3, 0.0)]);
        assert_eq!(l.caret_line(3), (20.0, 20.0));
        assert_eq!(l.content_height(), 8.0 + 40.0 + 8.0);
        let l = laid("ab\ncd", vec![line(0, 3, 0.0), line(3, 2, 20.0)]);
        assert_eq!(l.caret_line(1), (0.0, 20.0));
        assert_eq!(l.caret_line(5), (20.0, 20.0));
        // Empty: one line's height.
        assert_eq!(laid("", vec![]).content_height(), 36.0);
    }
}
