//! Fonts for Chinese, Japanese and Korean text, which the system font (Inter)
//! doesn't have.
//!
//! iOS falls back from the system font through a cascade list, and the
//! phone's first language decides which CJK font comes first
//! (fixtures/intltext, `cascade.*`): a Japanese phone draws kanji with
//! Hiragino (Japanese shapes), an English one with PingFang SC (Chinese
//! shapes). pomme draws them with the open-source Noto Sans fonts in the same
//! order: JP for Hiragino, SC and TC for PingFang, KR for Apple SD Gothic Neo.
//!
//! The fonts are used as they are, as Inter stands in for SF Pro: each glyph
//! takes the Noto font's own width, set solid (no punctuation compression or
//! pair spacing; kerning comes with text shaping). So CJK text breaks by the
//! same rules as on iOS, but not always at the same characters.
//!
//! The fonts are coretext faces of their own (not answering to names), so
//! TextKit and drawing treat a fallback glyph like a glyph in an app's font.

use crate::coretext;
use crate::render::Glyph;
use crate::{Emu, Result};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Japanese,
    SimplifiedChinese,
    TraditionalChinese,
    Korean,
}

impl Kind {
    fn file(self) -> &'static str {
        match self {
            Kind::Japanese => "NotoSansJP.ttf",
            Kind::SimplifiedChinese => "NotoSansSC.ttf",
            Kind::TraditionalChinese => "NotoSansTC.ttf",
            Kind::Korean => "NotoSansKR.ttf",
        }
    }

    fn family(self) -> &'static str {
        match self {
            Kind::Japanese => "Noto Sans JP",
            Kind::SimplifiedChinese => "Noto Sans SC",
            Kind::TraditionalChinese => "Noto Sans TC",
            Kind::Korean => "Noto Sans KR",
        }
    }
}

/// The cascade's CJK fonts in iOS's order for the phone's first language
/// (fixtures/intltext, `cascade.*`): the phone's own script's font first,
/// then Simplified Chinese, Traditional Chinese, Japanese, Korean.
fn order(first_language: &str) -> [Kind; 4] {
    use Kind::*;
    let l = first_language.to_lowercase();
    if l.starts_with("ja") {
        [Japanese, SimplifiedChinese, TraditionalChinese, Korean]
    } else if l.starts_with("ko") {
        [Korean, SimplifiedChinese, TraditionalChinese, Japanese]
    } else if ["zh-hant", "zh-tw", "zh-hk", "zh-mo"].iter().any(|p| l.starts_with(p)) {
        [TraditionalChinese, SimplifiedChinese, Japanese, Korean]
    } else {
        [SimplifiedChinese, TraditionalChinese, Japanese, Korean]
    }
}

/// A character's fallback: the face that draws it and its width there (em).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fallback {
    pub face: usize,
    pub advance: f64,
}

#[derive(Clone, Default)]
pub struct State {
    /// (kind, bold) → its coretext face, once registered (None: no font file).
    faces: HashMap<(Kind, bool), Option<usize>>,
    /// (char, bold) → its fallback, once looked up.
    found: HashMap<(char, bool), Option<Fallback>>,
    /// Characters the system font has, once looked up.
    in_system: HashMap<char, bool>,
}

/// UIFont weights from semibold up are drawn bold.
fn is_bold(weight: f64) -> bool {
    weight >= 0.3
}

/// The face for a kind of font at a weight, registered on first use.
fn face(emu: &mut Emu, kind: Kind, bold: bool) -> Option<usize> {
    if let Some(f) = emu.cjk.faces.get(&(kind, bold)) {
        return *f;
    }
    let f = crate::textkit::system_font(emu, kind.file()).map(|bytes| {
        let wght = if bold { 700.0 } else { 400.0 };
        coretext::register_fallback_face(emu, kind.family(), bytes, vec![(*b"wght", wght)])
    });
    emu.cjk.faces.insert((kind, bold), f);
    f
}

/// Whether the system font (Inter) has a glyph for `ch`.
fn system_has(emu: &mut Emu, ch: char) -> bool {
    if let Some(&has) = emu.cjk.in_system.get(&ch) {
        return has;
    }
    let has = crate::textkit::font_bytes(emu).is_some_and(|b| ttf_parser::Face::parse(&b, 0).ok().and_then(|f| f.glyph_index(ch)).is_some());
    emu.cjk.in_system.insert(ch, has);
    has
}

/// The fallback for a character the system font lacks; None for one it has,
/// or one no CJK font has.
pub(crate) fn fallback(emu: &mut Emu, ch: char, weight: f64) -> Result<Option<Fallback>> {
    let bold = is_bold(weight);
    if let Some(f) = emu.cjk.found.get(&(ch, bold)) {
        return Ok(*f);
    }
    let mut found = None;
    if !ch.is_ascii() && !system_has(emu, ch) {
        let first = crate::locale::first_preferred_language(emu)?;
        for kind in order(&first) {
            let Some(face) = face(emu, kind, bold) else { continue };
            if let Some(advance) = face_advance(emu, face, ch) {
                found = Some(Fallback { face, advance });
                break;
            }
        }
    }
    emu.cjk.found.insert((ch, bold), found);
    Ok(found)
}

/// `ch`'s width in a face (em); None if the face lacks it.
fn face_advance(emu: &mut Emu, face: usize, ch: char) -> Option<f64> {
    let (bytes, index, _) = coretext::face_data(emu, face)?;
    let parsed = ttf_parser::Face::parse(&bytes, index).ok()?;
    let g = parsed.glyph_index(ch)?;
    Some(parsed.glyph_hor_advance(g).map_or(1.0, |a| a as f64 / parsed.units_per_em() as f64))
}

/// The CJK fonts' faces in cascade order, for the phone's first language
/// (at regular weight; those with a font file).
pub(crate) fn cascade(emu: &mut Emu) -> Result<Vec<usize>> {
    let first = crate::locale::first_preferred_language(emu)?;
    Ok(order(&first).into_iter().filter_map(|k| face(emu, k, false)).collect())
}

/// Draw each glyph the system font (or the glyph's own app font) lacks from its fallback face.
pub(crate) fn assign(emu: &mut Emu, glyphs: &mut [Glyph]) -> Result<()> {
    for g in glyphs.iter_mut() {
        if g.ch.is_ascii() || (g.face != 0 && face_advance(emu, g.face, g.ch).is_some()) {
            continue;
        }
        if let Some(f) = fallback(emu, g.ch, g.weight)? {
            g.face = f.face;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phones_language_picks_the_first_font() {
        assert_eq!(order("ja-JP")[0], Kind::Japanese);
        assert_eq!(order("en-US")[0], Kind::SimplifiedChinese);
        assert_eq!(order("zh-Hans-CN")[0], Kind::SimplifiedChinese);
        assert_eq!(order("zh-Hant-TW")[0], Kind::TraditionalChinese);
        assert_eq!(order("ko")[0], Kind::Korean);
    }
}
