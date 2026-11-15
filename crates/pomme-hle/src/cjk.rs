//! Fonts for Chinese, Japanese and Korean text, which the system font (Inter)
//! doesn't have.
//!
//! iOS falls back from the system font through a cascade list, and the
//! phone's first language decides which CJK font comes first
//! (fixtures/intltext): a Japanese phone draws kanji with Hiragino (Japanese
//! shapes), an English one with PingFang SC (Chinese shapes). pomme draws them
//! with the open-source Noto Sans fonts in the same order: JP for Hiragino, SC
//! and TC for PingFang, KR for Apple SD Gothic Neo.
//!
//! The shapes are Noto's, but the advances are iOS's, as measured: Apple's UI
//! fonts are narrower than a full em, and on a Japanese phone brackets and
//! `、。` take half a cell. Each glyph moves within its cell as Apple's do:
//! centred, or (opening brackets) against the right edge.
//!
//! The fonts are coretext faces of their own (not answering to names), so
//! TextKit and drawing treat a fallback glyph like a glyph in an app's font.

use crate::coretext;
use crate::render::Glyph;
use crate::{Emu, Result};
use std::collections::HashMap;
use std::rc::Rc;

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
/// (fixtures/intltext, `cascade.*`): its own script's font first, then
/// Simplified Chinese, Traditional Chinese, Japanese, Korean.
fn order(first_language: &str) -> [Kind; 4] {
    use Kind::*;
    let l = first_language.to_lowercase();
    if l.starts_with("ja") {
        [Japanese, SimplifiedChinese, TraditionalChinese, Korean]
    } else if l.starts_with("ko") {
        [Korean, SimplifiedChinese, TraditionalChinese, Japanese]
    } else if l.starts_with("zh-hant") || l == "zh-tw" || l == "zh-hk" || l == "zh-mo" {
        [TraditionalChinese, SimplifiedChinese, Japanese, Korean]
    } else {
        [SimplifiedChinese, TraditionalChinese, Japanese, Korean]
    }
}

/// Characters that take half a cell in iOS's Japanese UI font (Hiragino
/// Kaku Gothic Interface): brackets, and the comma and full stop.
const JAPANESE_HALF: &str = "、。〈〉《》「」『』【】〔〕（），．";
/// Of those, the ones whose ink sits on the right of a full cell.
const OPENING: &str = "〈《「『【〔（";

fn is_kana(ch: char) -> bool {
    matches!(ch, '\u{3041}'..='\u{30ff}') || "々ゝゞヽヾ".contains(ch)
}

fn is_hangul(ch: char) -> bool {
    matches!(ch, '\u{ac00}'..='\u{d7a3}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}')
}

/// A character's advance on iOS, in em, as measured with the system font at
/// 17 pt (fixtures/intltext, `width.*`): on a Japanese phone (Hiragino),
/// 0.9355, or half that for brackets and `、。`; otherwise (PingFang, and its
/// kana fallback) 0.9912, kana 0.9486; Hangul (Apple SD Gothic Neo) 0.8653.
fn ios_advance(ch: char, japanese_first: bool) -> f64 {
    if is_hangul(ch) {
        0.8653
    } else if japanese_first {
        if JAPANESE_HALF.contains(ch) { 0.4677 } else { 0.9355 }
    } else if is_kana(ch) {
        0.9486
    } else {
        0.9912
    }
}

/// Where a glyph of `font_advance` sits in a cell of `advance` (both in em),
/// from the cell's left edge: centred, or opening brackets against its right.
fn ink_shift(ch: char, advance: f64, font_advance: f64) -> f64 {
    if advance >= font_advance {
        return (advance - font_advance) / 2.0;
    }
    if JAPANESE_HALF.contains(ch) {
        if OPENING.contains(ch) { advance - font_advance } else { 0.0 }
    } else {
        (advance - font_advance) / 2.0
    }
}

/// A character's fallback: the face that draws it, its advance and where its
/// glyph sits in the cell (em).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fallback {
    pub face: usize,
    pub advance: f64,
    pub shift: f64,
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
        let japanese_first = first.to_lowercase().starts_with("ja");
        for kind in order(&first) {
            let Some(face) = face(emu, kind, bold) else { continue };
            let Some((bytes, index, _)) = coretext::face_data(emu, face) else { continue };
            let Ok(parsed) = ttf_parser::Face::parse(&bytes, index) else { continue };
            let Some(g) = parsed.glyph_index(ch) else { continue };
            let font_advance = parsed.glyph_hor_advance(g).map_or(1.0, |a| a as f64 / parsed.units_per_em() as f64);
            let advance = ios_advance(ch, japanese_first);
            found = Some(Fallback { face, advance, shift: ink_shift(ch, advance, font_advance) });
            break;
        }
    }
    emu.cjk.found.insert((ch, bold), found);
    Ok(found)
}

/// The CJK fonts' faces in cascade order, for the phone's first language
/// (at regular weight; those with a font file).
pub(crate) fn cascade(emu: &mut Emu) -> Result<Vec<usize>> {
    let first = crate::locale::first_preferred_language(emu)?;
    Ok(order(&first).into_iter().filter_map(|k| face(emu, k, false)).collect())
}

/// iOS's Japanese punctuation spacing (japanese-spacing.txt): for a pair of
/// characters, how much space goes between them (em), and whether it belongs
/// to the right one (before an opening bracket: it moves to the next line
/// with it) or the left (after `、。` and closing brackets: it stays at the
/// end of a line).
pub(crate) fn pair_spacing(a: char, b: char) -> Option<(f64, bool)> {
    static TABLE: std::sync::OnceLock<HashMap<(char, char), f64>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = HashMap::new();
        for line in include_str!("japanese-spacing.txt").lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split_whitespace().collect();
            let ch = |s: &str| s.strip_prefix("U+").and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32);
            if let (Some(a), Some(b), Some(v)) = (ch(f[0]), ch(f[1]), f.get(2).and_then(|v| v.parse::<f64>().ok())) {
                t.insert((a, b), v / 17.0);
            }
        }
        t
    });
    let rep = |c: char| -> Option<char> {
        if table.keys().any(|&(x, y)| x == c || y == c) && !c.is_ascii() {
            Some(c)
        } else if c.is_ascii_digit() {
            Some('1')
        } else if c.is_ascii_alphabetic() {
            Some('a')
        } else if is_kana(c) || matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{ff10}'..='\u{ff5a}') {
            Some('あ')
        } else {
            None
        }
    };
    // (Between two Latin characters: the Latin font's own kerning, not this.)
    if a.is_ascii() && b.is_ascii() {
        return None;
    }
    let em = *table.get(&(rep(a)?, rep(b)?))?;
    Some((em, OPENING.contains(b)))
}

/// Whether text gets iOS's Japanese punctuation spacing: on a Japanese phone.
pub(crate) fn japanese_spacing(emu: &mut Emu) -> Result<bool> {
    Ok(crate::locale::first_preferred_language(emu)?.to_lowercase().starts_with("ja"))
}

/// Draw each glyph the system font (or the glyph's own app font) lacks from
/// its fallback face, moved within its cell as iOS places it.
pub(crate) fn assign(emu: &mut Emu, glyphs: &mut [Glyph]) -> Result<()> {
    for g in glyphs.iter_mut() {
        if g.ch.is_ascii() {
            continue;
        }
        if g.face != 0 && app_face_has(emu, g.face, g.ch) {
            continue;
        }
        if let Some(f) = fallback(emu, g.ch, g.weight)? {
            g.face = f.face;
            g.x += f.shift * g.size;
        }
    }
    Ok(())
}

fn app_face_has(emu: &mut Emu, face: usize, ch: char) -> bool {
    let Some((bytes, index, _)): Option<(Rc<Vec<u8>>, u32, _)> = coretext::face_data(emu, face) else { return false };
    ttf_parser::Face::parse(&bytes, index).ok().and_then(|f| f.glyph_index(ch)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phones_language_picks_the_first_font() {
        assert_eq!(order("ja-JP")[0], Kind::Japanese);
        assert_eq!(order("en-US")[0], Kind::SimplifiedChinese);
        assert_eq!(order("zh-Hant-TW")[0], Kind::TraditionalChinese);
        assert_eq!(order("ko")[0], Kind::Korean);
    }

    #[test]
    fn advances_as_measured() {
        assert_eq!(ios_advance('の', true), 0.9355);
        assert_eq!(ios_advance('、', true), 0.4677);
        assert_eq!(ios_advance('「', true), 0.4677);
        assert_eq!(ios_advance('［', true), 0.9355);
        assert_eq!(ios_advance('の', false), 0.9486);
        assert_eq!(ios_advance('漢', false), 0.9912);
        assert_eq!(ios_advance('、', false), 0.9912);
        assert_eq!(ios_advance('한', false), 0.8653);
    }

    #[test]
    fn japanese_spacing_as_measured() {
        // Half an em after 、 before kana; it stays with the 、.
        assert_eq!(pair_spacing('、', 'の'), Some((0.5, false)));
        // Before an opening bracket, after kanji: it goes with the bracket.
        let (em, leading) = pair_spacing('漢', '「').unwrap();
        assert!((em - 3.953 / 17.0).abs() < 1e-9 && leading);
        // An eighth of an em between Japanese and Latin.
        assert_eq!(pair_spacing('の', 'P'), Some((0.125, false)));
        assert_eq!(pair_spacing('P', 'o'), None);
        assert_eq!(pair_spacing('の', 'の'), None);
    }

    #[test]
    fn glyphs_sit_as_apples_do() {
        // A full-width Noto glyph in a narrower cell: centred.
        assert!((ink_shift('の', 0.9355, 1.0) + 0.03225).abs() < 1e-9);
        // Half-width brackets: an opening one against the right edge, a closing one at the left.
        assert!((ink_shift('「', 0.4677, 1.0) + 0.5323).abs() < 1e-9);
        assert_eq!(ink_shift('」', 0.4677, 1.0), 0.0);
        assert_eq!(ink_shift('。', 0.4677, 1.0), 0.0);
    }
}
