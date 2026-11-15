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
//! The shapes are Noto's, but the widths and spacing are iOS's, as measured
//! for each phone language (cjk-metrics.txt, made from the recordings by
//! scripts/measure/intltext/cjk-metrics.sh): Apple's CJK UI fonts are
//! narrower than an em, and on a Chinese or Japanese phone iOS halves the
//! punctuation and spaces pairs of characters as their typesetting does (an
//! English phone, with the same Chinese font, does neither). Each glyph moves
//! within its cell as Apple's do: centred, opening brackets against the right
//! edge, other half-width punctuation against the left (centred on a
//! Traditional Chinese phone, as Taiwan sets it).
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

/// How iOS sets CJK text, by the phone's first language (its key in cjk-metrics.txt).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Any other language (measured on an English phone).
    #[default]
    Other,
    Japanese,
    Korean,
    SimplifiedChinese,
    TraditionalChinese,
}

impl Profile {
    pub fn of(first_language: &str) -> Profile {
        let l = first_language.to_lowercase();
        if l.starts_with("ja") {
            Profile::Japanese
        } else if l.starts_with("ko") {
            Profile::Korean
        } else if l.starts_with("zh-hant") || l == "zh-tw" || l == "zh-hk" || l == "zh-mo" || l.starts_with("zh-tw") || l.starts_with("zh-hk") {
            Profile::TraditionalChinese
        } else if l.starts_with("zh") {
            Profile::SimplifiedChinese
        } else {
            Profile::Other
        }
    }

    fn key(self) -> &'static str {
        match self {
            Profile::Other => "en",
            Profile::Japanese => "ja",
            Profile::Korean => "ko",
            Profile::SimplifiedChinese => "zh-Hans",
            Profile::TraditionalChinese => "zh-Hant",
        }
    }

    /// The cascade's CJK fonts in iOS's order (fixtures/intltext, `cascade.*`):
    /// the phone's own script's font first, then Simplified Chinese,
    /// Traditional Chinese, Japanese, Korean.
    fn order(self) -> [Kind; 4] {
        use Kind::*;
        match self {
            Profile::Japanese => [Japanese, SimplifiedChinese, TraditionalChinese, Korean],
            Profile::Korean => [Korean, SimplifiedChinese, TraditionalChinese, Japanese],
            Profile::TraditionalChinese => [TraditionalChinese, SimplifiedChinese, Japanese, Korean],
            Profile::SimplifiedChinese | Profile::Other => [SimplifiedChinese, TraditionalChinese, Japanese, Korean],
        }
    }
}

/// Punctuation whose ink sits on the right of a full cell.
const OPENING: &str = "〈《「『【〔（［｛";
/// Punctuation a Traditional Chinese font centres in its cell.
const CENTRED: &str = "、。，．：；！？";

fn is_hiragana(ch: char) -> bool {
    matches!(ch, '\u{3041}'..='\u{309f}') || "々ゝゞ".contains(ch)
}

fn is_katakana(ch: char) -> bool {
    matches!(ch, '\u{30a0}'..='\u{30ff}') && !"・ー".contains(ch)
}

fn is_hangul(ch: char) -> bool {
    matches!(ch, '\u{ac00}'..='\u{d7a3}' | '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}')
}

/// The measured table (cjk-metrics.txt), in em.
struct Metrics {
    widths: HashMap<(&'static str, char), f64>,
    defaults: HashMap<(&'static str, &'static str), f64>,
    pairs: HashMap<(&'static str, char, char), f64>,
    /// The characters the pairs were measured for.
    pair_chars: Vec<char>,
}

fn metrics() -> &'static Metrics {
    static M: std::sync::OnceLock<Metrics> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        let mut m = Metrics { widths: HashMap::new(), defaults: HashMap::new(), pairs: HashMap::new(), pair_chars: Vec::new() };
        let ch = |s: &str| s.strip_prefix("U+").and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32);
        let key = |s: &str| -> &'static str { ["en", "ja", "ko", "zh-Hans", "zh-Hant"].into_iter().find(|k| *k == s).unwrap_or("en") };
        for line in include_str!("cjk-metrics.txt").lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split_whitespace().collect();
            let em = |s: &str| s.parse::<f64>().ok().map(|v| v / 17.0);
            match f.as_slice() {
                ["width", l, c, v] => {
                    if let (Some(c), Some(v)) = (ch(c), em(v)) {
                        m.widths.insert((key(l), c), v);
                    }
                }
                ["default", l, kind, v] => {
                    let kind = ["kana", "kanji", "hangul"].into_iter().find(|k| k == kind).unwrap_or("kanji");
                    if let Some(v) = em(v) {
                        m.defaults.insert((key(l), kind), v);
                    }
                }
                ["pair", l, a, b, v] => {
                    if let (Some(a), Some(b), Some(v)) = (ch(a), ch(b), em(v)) {
                        m.pairs.insert((key(l), a, b), v);
                        for c in [a, b] {
                            if !m.pair_chars.contains(&c) {
                                m.pair_chars.push(c);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        m
    })
}

/// A character's width on iOS, in em: as measured, else its kind's.
fn ios_advance(ch: char, profile: Profile) -> f64 {
    let m = metrics();
    let k = profile.key();
    if let Some(&w) = m.widths.get(&(k, ch)) {
        return w;
    }
    let kind = if is_hangul(ch) {
        "hangul"
    } else if is_hiragana(ch) || is_katakana(ch) {
        "kana"
    } else {
        "kanji"
    };
    m.defaults.get(&(k, kind)).copied().unwrap_or(1.0)
}

/// Where a glyph of `font_advance` sits in a cell of `advance` (both in em),
/// from the cell's left edge.
fn ink_shift(ch: char, advance: f64, font_advance: f64, profile: Profile) -> f64 {
    if advance >= font_advance || (profile == Profile::TraditionalChinese && CENTRED.contains(ch)) {
        (advance - font_advance) / 2.0
    } else if OPENING.contains(ch) {
        advance - font_advance
    } else if advance <= font_advance * 0.75 {
        // Half-width punctuation: against the left edge.
        0.0
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
        let profile = profile(emu)?;
        for kind in profile.order() {
            let Some(face) = face(emu, kind, bold) else { continue };
            let Some((bytes, index, _)) = coretext::face_data(emu, face) else { continue };
            let Ok(parsed) = ttf_parser::Face::parse(&bytes, index) else { continue };
            let Some(g) = parsed.glyph_index(ch) else { continue };
            let font_advance = parsed.glyph_hor_advance(g).map_or(1.0, |a| a as f64 / parsed.units_per_em() as f64);
            let advance = ios_advance(ch, profile);
            found = Some(Fallback { face, advance, shift: ink_shift(ch, advance, font_advance, profile) });
            break;
        }
    }
    emu.cjk.found.insert((ch, bold), found);
    Ok(found)
}

/// The CJK fonts' faces in cascade order, for the phone's first language
/// (at regular weight; those with a font file).
pub(crate) fn cascade(emu: &mut Emu) -> Result<Vec<usize>> {
    let profile = profile(emu)?;
    Ok(profile.order().into_iter().filter_map(|k| face(emu, k, false)).collect())
}

/// How this phone sets CJK text (its first language's profile).
pub(crate) fn profile(emu: &mut Emu) -> Result<Profile> {
    Ok(Profile::of(&crate::locale::first_preferred_language(emu)?))
}

/// iOS's CJK punctuation spacing on this phone (cjk-metrics.txt, `pair`): for
/// a pair of characters, how much space goes between them (em), and whether
/// it belongs to the right one (before an opening bracket: it moves to the
/// next line with it) or the left (after `、。` and closing brackets: it stays
/// at the end of a line).
pub(crate) fn pair_spacing(a: char, b: char, profile: Profile) -> Option<(f64, bool)> {
    // (Between two Latin characters: the Latin font's own kerning, not this.)
    if a.is_ascii() && b.is_ascii() {
        return None;
    }
    let m = metrics();
    let rep = |c: char| -> Option<char> {
        if m.pair_chars.contains(&c) && !c.is_ascii() {
            Some(c)
        } else if c.is_ascii_digit() {
            Some('1')
        } else if c.is_ascii_alphabetic() {
            Some('a')
        } else if is_hiragana(c) {
            Some('あ')
        } else if is_katakana(c) {
            Some('ア')
        } else if matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{ff10}'..='\u{ff5a}') {
            Some('漢')
        } else {
            None
        }
    };
    let em = *m.pairs.get(&(profile.key(), rep(a)?, rep(b)?))?;
    Some((em, OPENING.contains(b)))
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
        assert_eq!(Profile::of("ja-JP").order()[0], Kind::Japanese);
        assert_eq!(Profile::of("en-US").order()[0], Kind::SimplifiedChinese);
        assert_eq!(Profile::of("zh-Hant-TW").order()[0], Kind::TraditionalChinese);
        assert_eq!(Profile::of("zh-Hans-CN"), Profile::SimplifiedChinese);
        assert_eq!(Profile::of("ko").order()[0], Kind::Korean);
    }

    #[test]
    fn advances_as_measured() {
        let em = |pts: f64| pts / 17.0;
        assert_eq!(ios_advance('の', Profile::Japanese), em(15.903));
        assert_eq!(ios_advance('、', Profile::Japanese), em(7.951));
        assert_eq!(ios_advance('の', Profile::Other), em(16.126));
        assert_eq!(ios_advance('漢', Profile::Other), em(16.851));
        assert_eq!(ios_advance('、', Profile::Other), em(16.851));
        assert_eq!(ios_advance('、', Profile::SimplifiedChinese), em(8.583));
        assert_eq!(ios_advance('한', Profile::Other), em(14.710));
        // Not measured one by one: its kind's.
        assert_eq!(ios_advance('猫', Profile::Japanese), em(15.903));
    }

    #[test]
    fn spacing_as_measured() {
        // Half an em after 、 before kana on a Japanese phone; it stays with the 、.
        assert_eq!(pair_spacing('、', 'の', Profile::Japanese), Some((0.5, false)));
        // Before an opening bracket, after kanji: it goes with the bracket.
        let (em, leading) = pair_spacing('猫', '「', Profile::Japanese).unwrap();
        assert!((em - 3.953 / 17.0).abs() < 1e-9 && leading);
        // An eighth of an em between Japanese and Latin.
        assert_eq!(pair_spacing('の', 'P', Profile::Japanese), Some((0.125, false)));
        assert_eq!(pair_spacing('P', 'o', Profile::Japanese), None);
        // None on an English phone.
        assert_eq!(pair_spacing('、', 'の', Profile::Other), None);
        // Some on a Chinese one.
        assert!(pair_spacing('、', '漢', Profile::SimplifiedChinese).is_some());
    }

    #[test]
    fn glyphs_sit_as_apples_do() {
        // A full-width Noto glyph in a narrower cell: centred.
        assert!((ink_shift('の', 0.9355, 1.0, Profile::Japanese) + 0.03225).abs() < 1e-9);
        // Half-width brackets: an opening one against the right edge, a closing one at the left.
        assert!((ink_shift('「', 0.4677, 1.0, Profile::Japanese) + 0.5323).abs() < 1e-9);
        assert_eq!(ink_shift('」', 0.4677, 1.0, Profile::Japanese), 0.0);
        assert_eq!(ink_shift('。', 0.4677, 1.0, Profile::Japanese), 0.0);
        // Taiwan centres its full stop.
        assert!((ink_shift('。', 0.5, 1.0, Profile::TraditionalChinese) + 0.25).abs() < 1e-9);
    }
}
