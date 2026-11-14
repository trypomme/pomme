//! The clipboard: UIPasteboard's general pasteboard over a pluggable backend
//! (in memory by default, so runs are repeatable; or the host machine's, from
//! the CLI's `--clipboard host`), and the edit actions that use it
//! (`paste:`, `copy:`, `cut:`, `selectAll:`) on text fields.
//!
//! Paste permission follows iOS 16+: when an app reads text that another app
//! put on the pasteboard, iOS asks the user ("Allow Paste"). Reads of the
//! app's own copies don't ask, nor do reads that follow a paste the user
//! chose (pomme gives the app a second to read after one, for frameworks
//! that read asynchronously, as Flutter does). `hasStrings` and friends never
//! ask. Each prompt is counted (`prompts`) and answered by `permission`.
//!
//! Pastes are scripted like touches and typing: `--paste 'text@secs'` puts
//! the text on the clipboard as another app would and pastes it into the
//! first responder (`--paste '@secs'` pastes what's there).

use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// Where the clipboard's text lives.
pub trait Backend {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, text: Option<&str>);
    /// For a snapshot: a clipboard of pomme's own is copied (it's the
    /// phone's); one that's really the host's is the same clipboard.
    fn duplicate(&self) -> Box<dyn Backend>;
}

impl Clone for Box<dyn Backend> {
    fn clone(&self) -> Self {
        self.duplicate()
    }
}

/// A clipboard of pomme's own: starts with `0` (or nothing).
#[derive(Debug, Default)]
pub struct Memory(pub Option<String>);

impl Backend for Memory {
    fn read(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn write(&mut self, text: Option<&str>) {
        self.0 = text.map(str::to_string);
    }
    fn duplicate(&self) -> Box<dyn Backend> {
        Box::new(Memory(self.0.clone()))
    }
}

/// How the "Allow Paste" prompt is answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Permission {
    /// Tap "Allow Paste".
    #[default]
    Allow,
    /// Tap "Don't Allow": the read gets nothing.
    Deny,
}

impl std::str::FromStr for Permission {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            _ => Err(format!("{s:?}: expected allow or deny")),
        }
    }
}

/// A paste scripted at a virtual time: `text` (if any) is copied by "another
/// app" first, then the user pastes into the first responder.
#[derive(Clone, Debug)]
pub struct PasteInput {
    pub at_ns: u64,
    pub text: Option<String>,
}

/// How long after a paste the user chose the app may read without a prompt.
const USER_PASTE_GRANT_NS: u64 = 1_000_000_000;

#[derive(Clone)]
pub struct State {
    pub backend: Box<dyn Backend>,
    pub permission: Permission,
    pub script: Vec<PasteInput>,
    /// Reads that showed (or would have shown) the "Allow Paste" prompt.
    pub prompts: u32,
    change_count: u64,
    /// The text as pomme last saw it, to notice changes made outside the app.
    seen: Option<String>,
    /// Whether the app itself put the current text there.
    ours: bool,
    /// Until when (virtual ns) reads follow a paste the user chose.
    granted_until: u64,
    general: u64,
    /// Pasteboards other than the general one (`pasteboardWithUniqueName`, …): the app's own.
    private: HashMap<u64, Option<String>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            backend: Box::new(Memory::default()),
            permission: Permission::default(),
            script: Vec::new(),
            prompts: 0,
            change_count: 0,
            seen: None,
            ours: false,
            granted_until: 0,
            general: 0,
            private: HashMap::new(),
        }
    }
}

impl State {
    /// Notice a change from outside (the host, or `set_text`).
    fn sync(&mut self) -> Option<String> {
        let now = self.backend.read();
        if now != self.seen {
            self.seen = now.clone();
            self.change_count += 1;
            self.ours = false;
        }
        now
    }

    /// Copy `text` as another app would (a test, an agent, the user on the host).
    pub fn set_text(&mut self, text: Option<&str>) {
        self.backend.write(text);
        self.sync();
        self.ours = false;
    }

    /// What's on the clipboard (no prompt: this is the test looking, not the app).
    pub fn text(&mut self) -> Option<String> {
        self.sync()
    }

    /// The app copies.
    fn app_write(&mut self, text: Option<&str>) {
        self.backend.write(text);
        self.seen = text.map(str::to_string);
        self.change_count += 1;
        self.ours = true;
    }
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn general(emu: &mut Emu) -> Result<u64> {
    if emu.clipboard.general == 0 {
        let cls = host_class(emu, "UIPasteboard")?;
        emu.clipboard.general = objc::create_instance(emu, cls, 0)?;
    }
    Ok(emu.clipboard.general)
}

/// The text on pasteboard `p` without asking (for `hasStrings`, …).
fn peek(emu: &mut Emu, p: u64) -> Option<String> {
    if p == emu.clipboard.general {
        emu.clipboard.sync()
    } else {
        emu.clipboard.private.get(&p).cloned().flatten()
    }
}

/// The text on pasteboard `p` as the app reads it: another app's text on the
/// general pasteboard asks the user first.
fn read(emu: &mut Emu, p: u64) -> Option<String> {
    let text = peek(emu, p)?;
    if p != emu.clipboard.general || emu.clipboard.ours || crate::threads::now_ns(emu) < emu.clipboard.granted_until {
        return Some(text);
    }
    emu.clipboard.prompts += 1;
    if emu.trace {
        println!("[clipboard] the app reads another app's text: \"Allow Paste\" prompt ({:?})", emu.clipboard.permission);
    }
    match emu.clipboard.permission {
        Permission::Allow => Some(text),
        Permission::Deny => None,
    }
}

fn write(emu: &mut Emu, p: u64, text: Option<&str>) -> Result<()> {
    if p == emu.clipboard.general {
        emu.clipboard.app_write(text);
        foundation::post_named(emu, "UIPasteboardChangedNotification", p)?;
    } else {
        emu.clipboard.private.insert(p, text.map(str::to_string));
    }
    Ok(())
}

fn ret_string(emu: &mut Emu, s: Option<String>) -> Result<()> {
    let o = match s {
        Some(s) => foundation::new_nsstring(emu, &s)?,
        None => 0,
    };
    ret(emu, o)
}

fn string_arg(emu: &mut Emu, o: u64) -> Result<Option<String>> {
    if o == 0 { Ok(None) } else { Ok(Some(nsstring(emu, o)?)) }
}

fn is_url(s: &str) -> bool {
    s.split_once("://").is_some_and(|(scheme, rest)| !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) && !rest.is_empty())
}

/// Text types pasteboard items carry text under.
const TEXT_TYPES: &[&str] = &["public.utf8-plain-text", "public.plain-text", "public.text", "public.utf16-plain-text", "public.url"];

fn is_text_type(emu: &mut Emu, t: u64) -> Result<bool> {
    Ok(t != 0 && TEXT_TYPES.contains(&nsstring(emu, t)?.as_str()))
}

/// `setItems:` / `addItems:`: the first item's text.
fn text_of_items(emu: &mut Emu, items: u64) -> Result<Option<String>> {
    if items == 0 {
        return Ok(None);
    }
    for item in foundation::array_values(emu, items)? {
        for t in TEXT_TYPES {
            let k = foundation::new_nsstring(emu, t)?;
            let v = send(emu, item, "objectForKey:", &[k])?;
            if v != 0 {
                return text_value(emu, v);
            }
        }
    }
    Ok(None)
}

/// A pasteboard value as text: a string, a URL or UTF-8 data.
fn text_value(emu: &mut Emu, v: u64) -> Result<Option<String>> {
    if v == 0 {
        return Ok(None);
    }
    let cls = host_class(emu, "NSString")?;
    if objc::is_kind_of(emu, v, cls)? {
        return string_arg(emu, v);
    }
    let s = objc::intern(emu, "absoluteString")?;
    if objc::responds(emu, v, s)? {
        let a = send(emu, v, "absoluteString", &[])?;
        return string_arg(emu, a);
    }
    let cls = host_class(emu, "NSData")?;
    if objc::is_kind_of(emu, v, cls)? {
        let bytes = foundation::data_bytes(emu, v)?;
        return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
    }
    Ok(None)
}

// ---- edit actions on text fields ----

/// `paste:` on a text field or view: the clipboard's text replaces the selection, as typing does.
fn tf_paste(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let p = general(emu)?;
    // The user chose Paste: no prompt.
    if let Some(text) = peek(emu, p) {
        crate::textinput::replace_selection(emu, f, &text)?;
    }
    Ok(())
}

fn tf_copy(emu: &mut Emu, cut: bool) -> Result<()> {
    let f = emu.arg(0);
    let Some(text) = crate::textinput::selected_text(emu, f) else { return Ok(()) };
    let p = general(emu)?;
    write(emu, p, Some(&text))?;
    if cut {
        crate::textinput::replace_selection(emu, f, "")?;
    }
    Ok(())
}

/// The user pastes (the scripted `--paste`): into the first responder's `paste:`.
fn fire_paste(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(1) as usize;
    let Some(inp) = emu.clipboard.script.get(i).cloned() else { return Ok(()) };
    if let Some(t) = &inp.text {
        emu.clipboard.set_text(Some(t));
    }
    let r = emu.input.first_responder;
    if r == 0 {
        if emu.trace {
            println!("[clipboard] paste at {} ns: nothing has focus", inp.at_ns);
        }
        return Ok(());
    }
    emu.clipboard.granted_until = crate::threads::now_ns(emu) + USER_PASTE_GRANT_NS;
    let sel = objc::intern(emu, "paste:")?;
    if objc::responds(emu, r, sel)? {
        send(emu, r, "paste:", &[0])?;
    } else if let Some(text) = emu.clipboard.text() {
        // A plain UIKeyInput responder: the text arrives as typing.
        let s = foundation::new_nsstring(emu, &text)?;
        send(emu, r, "insertText:", &[s])?;
    }
    Ok(())
}

/// Put the scripted pastes on the main run loop.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.clipboard.script.len() {
        let at = emu.clipboard.script[i].at_ns;
        let delay = at.saturating_sub(now) as f64 / 1e9;
        crate::uikit::after_on_main(emu, delay, fire_paste, i as u64)?;
    }
    Ok(())
}

const P: &str = "UIPasteboard";
const TF: &str = "UITextField";
const TV: &str = "UITextView";

/// The edit actions (the edit menu's Paste, Copy, …) of both text controls.
macro_rules! edit_actions {
    ($c:expr) => {
        [
        ($c, false, "paste:", tf_paste),
        ($c, false, "copy:", |e| tf_copy(e, false)),
        ($c, false, "cut:", |e| tf_copy(e, true)),
        ($c, false, "selectAll:", |e| {
            let f = e.arg(0);
            crate::textinput::select_all(e, f);
            Ok(())
        }),
        ($c, false, "canPerformAction:withSender:", |e| {
            let (f, action) = (e.arg(0), e.arg(2));
            let name = objc::sel_name(e, action);
            let has_selection = crate::textinput::selected_text(e, f).is_some();
            let ok = match name.as_str() {
                "paste:" => {
                    let p = general(e)?;
                    peek(e, p).is_some()
                }
                "copy:" | "cut:" => has_selection,
                "selectAll:" => true,
                _ => false,
            };
            ret(e, ok as u64)
        }),
        ]
    };
}

pub(crate) const EDIT_ACTIONS: [&[(&str, bool, &str, HostFn)]; 2] = [&edit_actions!(TF), &edit_actions!(TV)];

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (P, true, "generalPasteboard", |e| {
        let p = general(e)?;
        ret(e, p)
    }),
    (P, true, "pasteboardWithUniqueName", |e| {
        let cls = host_class(e, P)?;
        let p = objc::create_instance(e, cls, 0)?;
        e.clipboard.private.insert(p, None);
        ret(e, p)
    }),
    (P, true, "pasteboardWithName:create:", |e| {
        let cls = host_class(e, P)?;
        let p = objc::create_instance(e, cls, 0)?;
        e.clipboard.private.insert(p, None);
        ret(e, p)
    }),
    (P, true, "removePasteboardWithName:", |_| Ok(())),
    (P, false, "name", |e| {
        let s = foundation::new_nsstring(e, "com.apple.UIKit.pboard.general")?;
        ret(e, s)
    }),
    (P, false, "string", |e| {
        let s = read(e, e.arg(0));
        ret_string(e, s)
    }),
    (P, false, "setString:", |e| {
        let (p, s) = (e.arg(0), e.arg(2));
        let s = string_arg(e, s)?;
        write(e, p, s.as_deref())
    }),
    (P, false, "strings", |e| {
        let s = read(e, e.arg(0));
        let items = match s {
            Some(s) => vec![foundation::new_nsstring(e, &s)?],
            None => vec![],
        };
        let a = foundation::new_array_of(e, &items)?;
        ret(e, a)
    }),
    (P, false, "setStrings:", |e| {
        let (p, a) = (e.arg(0), e.arg(2));
        let first = if a == 0 { 0 } else { foundation::array_values(e, a)?.first().copied().unwrap_or(0) };
        let s = string_arg(e, first)?;
        write(e, p, s.as_deref())
    }),
    (P, false, "URL", |e| {
        let s = read(e, e.arg(0)).filter(|s| is_url(s));
        let Some(s) = s else { return ret(e, 0) };
        let ns = foundation::new_nsstring(e, &s)?;
        let cls = host_class(e, "NSURL")?;
        let u = send(e, cls, "URLWithString:", &[ns])?;
        ret(e, u)
    }),
    (P, false, "setURL:", |e| {
        let (p, u) = (e.arg(0), e.arg(2));
        let s = text_value(e, u)?;
        write(e, p, s.as_deref())
    }),
    (P, false, "hasStrings", |e| {
        let has = peek(e, e.arg(0)).is_some();
        ret(e, has as u64)
    }),
    (P, false, "hasURLs", |e| {
        let has = peek(e, e.arg(0)).is_some_and(|s| is_url(&s));
        ret(e, has as u64)
    }),
    (P, false, "hasImages", |e| ret(e, 0)),
    (P, false, "hasColors", |e| ret(e, 0)),
    (P, false, "image", |e| ret(e, 0)),
    (P, false, "images", |e| ret(e, 0)),
    (P, false, "numberOfItems", |e| {
        let n = peek(e, e.arg(0)).is_some() as u64;
        ret(e, n)
    }),
    (P, false, "changeCount", |e| {
        let p = e.arg(0);
        if p == e.clipboard.general {
            e.clipboard.sync();
        }
        let n = e.clipboard.change_count;
        ret(e, n)
    }),
    (P, false, "pasteboardTypes", |e| {
        let types = match peek(e, e.arg(0)) {
            Some(_) => vec![foundation::new_nsstring(e, "public.utf8-plain-text")?],
            None => vec![],
        };
        let a = foundation::new_array_of(e, &types)?;
        ret(e, a)
    }),
    (P, false, "containsPasteboardTypes:", |e| {
        let (p, a) = (e.arg(0), e.arg(2));
        let mut any = false;
        if a != 0 && peek(e, p).is_some() {
            for t in foundation::array_values(e, a)? {
                any |= is_text_type(e, t)?;
            }
        }
        ret(e, any as u64)
    }),
    (P, false, "items", |e| {
        let s = read(e, e.arg(0));
        let items = match s {
            Some(s) => {
                let k = foundation::new_nsstring(e, "public.utf8-plain-text")?;
                let v = foundation::new_nsstring(e, &s)?;
                vec![foundation::new_dict_of(e, &[(k, v)])?]
            }
            None => vec![],
        };
        let a = foundation::new_array_of(e, &items)?;
        ret(e, a)
    }),
    (P, false, "setItems:", |e| {
        let (p, a) = (e.arg(0), e.arg(2));
        let s = text_of_items(e, a)?;
        write(e, p, s.as_deref())
    }),
    (P, false, "setItems:options:", |e| {
        let (p, a) = (e.arg(0), e.arg(2));
        let s = text_of_items(e, a)?;
        write(e, p, s.as_deref())
    }),
    (P, false, "addItems:", |e| {
        let (p, a) = (e.arg(0), e.arg(2));
        let s = text_of_items(e, a)?;
        write(e, p, s.as_deref())
    }),
    (P, false, "valueForPasteboardType:", |e| {
        let (p, t) = (e.arg(0), e.arg(2));
        if !is_text_type(e, t)? {
            return ret(e, 0);
        }
        let s = read(e, p);
        ret_string(e, s)
    }),
    (P, false, "dataForPasteboardType:", |e| {
        let (p, t) = (e.arg(0), e.arg(2));
        if !is_text_type(e, t)? {
            return ret(e, 0);
        }
        let Some(s) = read(e, p) else { return ret(e, 0) };
        let d = foundation::new_data(e, s.as_bytes())?;
        ret(e, d)
    }),
    (P, false, "setValue:forPasteboardType:", |e| {
        let (p, v, t) = (e.arg(0), e.arg(2), e.arg(3));
        if is_text_type(e, t)? {
            let s = text_value(e, v)?;
            write(e, p, s.as_deref())?;
        }
        Ok(())
    }),
    (P, false, "setData:forPasteboardType:", |e| {
        let (p, v, t) = (e.arg(0), e.arg(2), e.arg(3));
        if is_text_type(e, t)? {
            let s = text_value(e, v)?;
            write(e, p, s.as_deref())?;
        }
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_from_outside_are_noticed() {
        let mut s = State::default();
        assert_eq!(s.text(), None);
        s.set_text(Some("from another app"));
        assert_eq!((s.text().as_deref(), s.change_count, s.ours), (Some("from another app"), 1, false));
        s.app_write(Some("copied in the app"));
        assert_eq!((s.change_count, s.ours), (2, true));
        // Reading back what the app wrote changes nothing.
        assert_eq!(s.text().as_deref(), Some("copied in the app"));
        assert_eq!((s.change_count, s.ours), (2, true));
        // The host's clipboard changed under pomme: noticed on the next look.
        s.backend.write(Some("copied on the host"));
        assert_eq!(s.text().as_deref(), Some("copied on the host"));
        assert_eq!((s.change_count, s.ours), (3, false));
    }

    #[test]
    fn urls() {
        assert!(is_url("https://expo.dev/x"));
        assert!(is_url("myapp:///settings"));
        assert!(!is_url("hello"));
        assert!(!is_url("://x"));
    }
}
