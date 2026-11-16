//! The rules for lines of Chinese, Japanese and Korean text, shared by the
//! tests that lay out the same Japanese paragraph (intltext, hello_flutter).
//! pomme sets CJK text in Noto at its own widths, so its lines break at other
//! characters than iOS's; these are the rules both keep.

#![allow(dead_code)]

/// The paragraph the intltext probe and hello-flutter lay out.
pub const PARAGRAPH: &str = "日本語の文章は、単語の間に空白がなくても行の途中で折り返されます。小さい「ょ」や「っ」、句読点は行の頭に来ません。Pommeでも同じように折り返されるはずです。";
/// The widths they lay it out at, and its font size.
pub const WIDTHS: [u32; 5] = [180, 200, 220, 250, 300];
pub const SIZE: f64 = 17.0;

/// Characters a line may not start with, and may not end with.
const NO_START: &str = "、。，．」』）〕］｝〉》】ぁぃぅぇぉゃゅょっゎァィゥェォャュョッー！？：；・";
const NO_END: &str = "「『（〔［｛〈《【";

pub fn is_cjk(ch: char) -> bool {
    matches!(ch, '\u{3000}'..='\u{9fff}' | '\u{ff01}'..='\u{ff5e}' | '\u{ac00}'..='\u{d7a3}')
}

/// What breaks a rule in `lines` (text, width) of PARAGRAPH laid out at
/// `width`: text lost, a line starting or ending where it may not, too wide,
/// or not full (more than a few characters short, moving `「ょ」` or a Latin
/// word to the next line), but for the last line. `slack` allows for kerning.
pub fn faults(name: &str, lines: &[(String, f64)], width: f64, slack: f64) -> Vec<String> {
    let mut wrong = Vec::new();
    let all: String = lines.iter().map(|l| l.0.as_str()).collect();
    if all != PARAGRAPH {
        wrong.push(format!("{name}: the lines hold {all:?}"));
    }
    for (i, (text, w)) in lines.iter().enumerate() {
        let line = format!("{name}.line{i} {text:?}");
        let (first, last) = (text.chars().next().unwrap_or(' '), text.chars().last().unwrap_or(' '));
        if i > 0 && NO_START.contains(first) {
            wrong.push(format!("{line}: starts with {first}"));
        }
        if i + 1 < lines.len() && NO_END.contains(last) {
            wrong.push(format!("{line}: ends with {last}"));
        }
        if *w > width + 0.01 {
            wrong.push(format!("{line}: {w:.2} wide, in {width}"));
        }
        if i + 1 < lines.len() && *w < width - 4.0 * SIZE - slack {
            wrong.push(format!("{line}: only {w:.2} of {width}"));
        }
    }
    wrong
}
