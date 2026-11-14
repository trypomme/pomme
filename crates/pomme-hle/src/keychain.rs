//! The Keychain (Security's SecItem API) and LocalAuthentication.
//!
//! Items live in memory for the run, like the data container (a fresh
//! install each time). An item is its class (generic or internet password,
//! …), its attributes and its data. Generic passwords are unique by service,
//! account and access group; internet passwords also by server, protocol,
//! port and path. A query matches an item of its class whose attributes equal
//! every attribute the query names; its `r_*` / `m_Limit` options say what
//! comes back. Status codes are Security's.
//!
//! Biometry is modeled as the iOS Simulator's Face ID / Touch ID menu has it:
//! not enrolled (the default) or enrolled. An enrolled phone's prompts wait
//! for the person to present a matching or non-matching face (Features ▸
//! Face ID ▸ Matching Face / Non-matching Face, a [`Face`] from the window),
//! or are answered at once with one chosen up front (`--biometry match` /
//! `no-match`, for runs nobody watches). Items made with an access control
//! that asks for biometry need a match to be read. Like a fresh Simulator,
//! the phone has no device passcode, so nothing falls back to one.

use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

const SUCCESS: i64 = 0;
const PARAM: i64 = -50;
const DUPLICATE_ITEM: i64 = -25299;
const ITEM_NOT_FOUND: i64 = -25300;
const AUTH_FAILED: i64 = -25293;

/// How the phone's biometry answers (the Simulator's Features ▸ Face ID menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Biometry {
    #[default]
    NotEnrolled,
    /// Enrolled; each prompt waits for a [`Face`].
    Enrolled,
    /// Enrolled; every prompt sees a matching face.
    Matching,
    /// Enrolled; every prompt sees a face that doesn't match.
    NonMatching,
}

/// What the person does from the Simulator's Features ▸ Face ID menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Enrolled on or off. Turning it off fails any waiting prompt.
    ToggleEnrolled,
    /// Present a matching face to the waiting prompts.
    Matching,
    /// Present a face that doesn't match.
    NonMatching,
}

impl std::str::FromStr for Biometry {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        Ok(match s {
            "off" | "not-enrolled" => Biometry::NotEnrolled,
            "enrolled" | "ask" => Biometry::Enrolled,
            "match" | "matching" => Biometry::Matching,
            "no-match" | "non-matching" => Biometry::NonMatching,
            o => return Err(format!("{o}: expected off, enrolled, match or no-match")),
        })
    }
}

impl std::str::FromStr for Face {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        Ok(match s {
            "match" | "matching" => Face::Matching,
            "no-match" | "non-matching" => Face::NonMatching,
            "toggle-enrolled" => Face::ToggleEnrolled,
            o => return Err(format!("{o}: expected match, no-match or toggle-enrolled")),
        })
    }
}

#[derive(Clone, Debug)]
struct Item {
    class: String,
    /// Attribute key ("svce", "acct", …) → its value object.
    attrs: Vec<(String, u64)>,
    data: u64,
    /// Made with an access control asking for the user's presence (biometry).
    needs_auth: bool,
}

#[derive(Clone, Default)]
pub struct State {
    items: Vec<Item>,
    pub biometry: Biometry,
    /// SecAccessControl objects → whether they ask for biometry.
    access_controls: HashMap<u64, bool>,
    /// LAContext replies waiting to run: (reply block, success, error).
    replies: Vec<Option<(u64, bool, u64)>>,
    /// Prompts shown on an enrolled phone, waiting for a face, and their answers.
    prompts: Vec<(Prompt, Option<i64>)>,
    /// Scripted faces: (virtual ns, what the person does then), in time order.
    pub faces: Vec<(u64, Face)>,
    faces_presented: usize,
}

/// When the next scripted face is due (a deadline for the scheduler).
pub(crate) fn next_face_at(emu: &Emu) -> Option<u64> {
    emu.keychain.faces.get(emu.keychain.faces_presented).map(|f| f.0)
}

/// Present the scripted faces that are due. Like the window's, they come from
/// the system, not the app's run loop, so a prompt that blocks the main
/// thread still hears them.
pub(crate) fn present_due(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    while let Some(&(at, face)) = emu.keychain.faces.get(emu.keychain.faces_presented) {
        if at > now {
            break;
        }
        emu.keychain.faces_presented += 1;
        present(emu, face)?;
    }
    Ok(())
}

/// A biometry prompt waiting for the person.
#[derive(Clone, Copy, Debug)]
enum Prompt {
    /// `evaluatePolicy:…reply:`: the reply block, for the policy.
    Reply { block: u64, policy: u64 },
    /// `SecItemCopyMatching` reading a protected item: where the result goes
    /// (0: nowhere) and what it is (owned; released if the face fails).
    Item { result: u64, value: u64 },
}

/// The status a waiting `SecItemCopyMatching` prompt ended with (the
/// scheduler's check for [`crate::threads::Wait::Face`]).
pub(crate) fn answer(emu: &Emu, prompt: usize) -> Option<u64> {
    emu.keychain.prompts.get(prompt).and_then(|p| p.1).map(|s| s as u64)
}

/// Show a prompt: the host hears of it (the window says how to answer).
fn show(emu: &mut Emu, prompt: Prompt, reason: u64) -> Result<usize> {
    let reason = if reason != 0 { foundation::nsstring(emu, reason).unwrap_or_default() } else { String::new() };
    crate::host::biometry_prompt(emu, &reason);
    emu.keychain.prompts.push((prompt, None));
    Ok(emu.keychain.prompts.len() - 1)
}

/// The person did something from the Face ID menu.
pub(crate) fn present(emu: &mut Emu, face: Face) -> Result<()> {
    match face {
        Face::ToggleEnrolled if emu.keychain.biometry == Biometry::NotEnrolled => emu.keychain.biometry = Biometry::Enrolled,
        Face::ToggleEnrolled => {
            emu.keychain.biometry = Biometry::NotEnrolled;
            answer_all(emu, false)?;
        }
        Face::Matching => answer_all(emu, true)?,
        Face::NonMatching => answer_all(emu, false)?,
    }
    Ok(())
}

/// Answer every waiting prompt.
fn answer_all(emu: &mut Emu, matched: bool) -> Result<()> {
    if emu.keychain.prompts.iter().any(|p| p.1.is_none()) {
        crate::host::biometry_answered(emu);
    }
    for i in 0..emu.keychain.prompts.len() {
        let (prompt, answered) = emu.keychain.prompts[i];
        if answered.is_some() {
            continue;
        }
        let status = match prompt {
            Prompt::Reply { block, policy } => {
                let code = if matched { 0 } else { failure(emu, policy) };
                reply(emu, block, code)?;
                SUCCESS
            }
            Prompt::Item { result, value } if matched => {
                if result != 0 {
                    emu.write_u64(result, value)?;
                }
                SUCCESS
            }
            Prompt::Item { value, .. } => {
                if value != 0 {
                    send(emu, value, "release", &[])?;
                }
                AUTH_FAILED
            }
        };
        emu.keychain.prompts[i].1 = Some(status);
    }
    Ok(())
}

/// The LAError a failed prompt gives: authentication failed (-1) on an
/// enrolled phone; otherwise passcode not set (-5) for the passcode policy,
/// biometry not enrolled (-7) for biometrics.
fn failure(emu: &Emu, policy: u64) -> i64 {
    match emu.keychain.biometry {
        Biometry::NotEnrolled => {
            if policy == 2 {
                -5
            } else {
                -7
            }
        }
        _ => -1,
    }
}

/// Run an LAContext reply block on a global queue, as LocalAuthentication
/// does: success when `code` is 0, else an LAError with it.
fn reply(emu: &mut Emu, block: u64, code: i64) -> Result<()> {
    let err = if code == 0 { 0 } else { la_error(emu, code)? };
    let i = emu.keychain.replies.len();
    emu.keychain.replies.push(Some((block, code == 0, err)));
    let f = emu.host_thunk(run_reply)?;
    let q = crate::libsystem::global_queue(emu)?;
    crate::libsystem::dispatch_on(emu, q, f, i as u64)
}

/// On a background thread: an LAContext reply.
fn run_reply(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(0) as usize;
    let Some((block, ok, err)) = emu.keychain.replies.get_mut(i).and_then(Option::take) else { return Ok(()) };
    crate::libsystem::call_block(emu, block, &[ok as u64, err])?;
    Ok(())
}

/// The attributes that make an item of `class` unique.
fn primary_key(class: &str) -> &'static [&'static str] {
    match class {
        "genp" => &["agrp", "svce", "acct", "sync"],
        "inet" => &["agrp", "acct", "sdmn", "srvr", "ptcl", "atyp", "port", "path", "sync"],
        _ => &["agrp", "labl", "sync"],
    }
}

/// Query keys that say how to search and what to return, not what an item is.
fn is_control(key: &str) -> bool {
    key.starts_with("m_") || key.starts_with("r_") || key.starts_with("u_") || key.starts_with("v_") || key == "class" || key == "nleg" || key == "accc"
}

fn ret(emu: &mut Emu, v: i64) -> Result<()> {
    emu.ret(v as u64);
    Ok(())
}

/// A query or attribute dictionary as (key, value) pairs, keys as strings.
fn pairs(emu: &mut Emu, d: u64) -> Result<Vec<(String, u64)>> {
    if d == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (k, v) in foundation::dict_pairs(emu, d)? {
        out.push((foundation::nsstring(emu, k)?, v));
    }
    Ok(out)
}

fn get<'a>(p: &'a [(String, u64)], key: &str) -> Option<u64> {
    p.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

fn truthy(emu: &mut Emu, v: Option<u64>) -> Result<bool> {
    match v {
        Some(v) if v != 0 => Ok(send(emu, v, "boolValue", &[])? & 0xff != 0),
        _ => Ok(false),
    }
}

fn equal(emu: &mut Emu, a: u64, b: u64) -> Result<bool> {
    Ok(a == b || (a != 0 && send(emu, a, "isEqual:", &[b])? & 0xff != 0))
}

/// Whether item `it` is what the query asks for. `kSecAttrSynchronizableAny` matches either.
fn matches(emu: &mut Emu, it: &Item, class: &str, query: &[(String, u64)]) -> Result<bool> {
    if it.class != class {
        return Ok(false);
    }
    for (k, v) in query {
        if is_control(k) {
            continue;
        }
        if k == "sync" && foundation::nsstring(emu, *v).is_ok_and(|s| s == "syna") {
            continue;
        }
        match get(&it.attrs, k) {
            Some(mine) if equal(emu, mine, *v)? => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn class_of(emu: &mut Emu, query: &[(String, u64)]) -> Result<Option<String>> {
    match get(query, "class") {
        Some(c) => foundation::nsstring(emu, c).map(Some),
        None => Ok(None),
    }
}

/// Indices of the items the query matches.
fn find(emu: &mut Emu, query: &[(String, u64)]) -> Result<Option<(String, Vec<usize>)>> {
    let Some(class) = class_of(emu, query)? else { return Ok(None) };
    let items = emu.keychain.items.clone();
    let mut hits = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if matches(emu, it, &class, query)? {
            hits.push(i);
        }
    }
    Ok(Some((class, hits)))
}


/// `SecItemAdd(attributes, &result)`.
fn item_add(emu: &mut Emu) -> Result<()> {
    let (attrs, result) = (emu.arg(0), emu.arg(1));
    let a = pairs(emu, attrs)?;
    let Some(class) = class_of(emu, &a)? else { return ret(emu, PARAM) };
    // A duplicate: an item of the class with the same primary-key attributes.
    let key: Vec<(String, u64)> = a.iter().filter(|(k, _)| primary_key(&class).contains(&k.as_str())).cloned().collect();
    let items = emu.keychain.items.clone();
    for it in &items {
        if it.class != class {
            continue;
        }
        let mut same = true;
        for k in primary_key(&class) {
            let (x, y) = (get(&it.attrs, k), get(&key, k));
            same &= match (x, y) {
                (Some(x), Some(y)) => equal(emu, x, y)?,
                (None, None) => true,
                _ => false,
            };
        }
        if same {
            return ret(emu, DUPLICATE_ITEM);
        }
    }
    let mut stored = Vec::new();
    for (k, v) in &a {
        if !is_control(k) {
            let v = send(emu, *v, "copy", &[])?;
            stored.push((k.clone(), v));
        }
    }
    let data = match get(&a, "v_Data") {
        Some(d) => send(emu, d, "copy", &[])?,
        None => 0,
    };
    let needs_auth = get(&a, "accc").is_some_and(|ac| emu.keychain.access_controls.get(&ac).copied().unwrap_or(false));
    emu.keychain.items.push(Item { class, attrs: stored, data, needs_auth });
    if result != 0 {
        emu.write_u64(result, 0)?;
    }
    ret(emu, SUCCESS)
}

/// What a match gives back: its data, its attributes, or both in one dictionary.
fn result_for(emu: &mut Emu, it: &Item, data: bool, attrs: bool) -> Result<u64> {
    if !attrs {
        return Ok(if data { it.data } else { 0 });
    }
    let mut kv = Vec::new();
    for (k, v) in it.attrs.clone() {
        let k = foundation::new_nsstring(emu, &k)?;
        kv.push((k, v));
    }
    let class = foundation::new_nsstring(emu, &it.class)?;
    let ck = foundation::new_nsstring(emu, "class")?;
    kv.push((ck, class));
    if data && it.data != 0 {
        let k = foundation::new_nsstring(emu, "v_Data")?;
        kv.push((k, it.data));
    }
    foundation::new_dict_of(emu, &kv)
}

/// `SecItemCopyMatching(query, &result)`.
fn copy_matching(emu: &mut Emu) -> Result<()> {
    let (query, result) = (emu.arg(0), emu.arg(1));
    let q = pairs(emu, query)?;
    let Some((_, hits)) = find(emu, &q)? else { return ret(emu, PARAM) };
    if hits.is_empty() {
        return ret(emu, ITEM_NOT_FOUND);
    }
    let data = truthy(emu, get(&q, "r_Data"))?;
    let attrs = truthy(emu, get(&q, "r_Attributes"))?;
    let all = match get(&q, "m_Limit") {
        Some(l) => foundation::nsstring(emu, l).is_ok_and(|s| s == "m_LimitAll"),
        None => false,
    };
    let items: Vec<Item> = hits.iter().map(|&i| emu.keychain.items[i].clone()).collect();
    // Reading a protected item's data asks for the user.
    let protected = data && items.iter().take(if all { usize::MAX } else { 1 }).any(|it| it.needs_auth);
    let biometry = emu.keychain.biometry;
    if protected && matches!(biometry, Biometry::NotEnrolled | Biometry::NonMatching) {
        return ret(emu, AUTH_FAILED);
    }
    let mut r = 0;
    if result != 0 && (data || attrs) {
        r = if all {
            let mut list = Vec::new();
            for it in &items {
                list.push(result_for(emu, it, data, attrs)?);
            }
            foundation::new_array_of(emu, &list)?
        } else {
            result_for(emu, &items[0], data, attrs)?
        };
        if r != 0 {
            send(emu, r, "retain", &[])?; // a Copy function: the caller owns it
        }
    }
    if protected && biometry == Biometry::Enrolled {
        // The call blocks while the prompt waits for a face; the answer
        // stores the result and is the status.
        let reason = get(&q, "u_OpPrompt").unwrap_or(0);
        let prompt = show(emu, Prompt::Item { result, value: r }, reason)?;
        let status = crate::threads::block(emu, crate::threads::Wait::Face { prompt })?;
        emu.ret(status);
        return Ok(());
    }
    if result != 0 && (data || attrs) {
        emu.write_u64(result, r)?;
    }
    ret(emu, SUCCESS)
}

/// `SecItemUpdate(query, attributesToUpdate)`.
fn item_update(emu: &mut Emu) -> Result<()> {
    let (query, changes) = (emu.arg(0), emu.arg(1));
    let q = pairs(emu, query)?;
    let c = pairs(emu, changes)?;
    let Some((_, hits)) = find(emu, &q)? else { return ret(emu, PARAM) };
    if hits.is_empty() {
        return ret(emu, ITEM_NOT_FOUND);
    }
    for i in hits {
        for (k, v) in &c {
            let v = send(emu, *v, "copy", &[])?;
            if k == "v_Data" {
                emu.keychain.items[i].data = v;
            } else if k == "accc" {
                let needs = emu.keychain.access_controls.get(&v).copied().unwrap_or(false);
                emu.keychain.items[i].needs_auth = needs;
            } else if !is_control(k) {
                let it = &mut emu.keychain.items[i];
                it.attrs.retain(|(x, _)| x != k);
                it.attrs.push((k.clone(), v));
            }
        }
    }
    ret(emu, SUCCESS)
}

/// `SecItemDelete(query)`: every item it matches.
fn item_delete(emu: &mut Emu) -> Result<()> {
    let q = pairs(emu, emu.arg(0))?;
    let Some((_, hits)) = find(emu, &q)? else { return ret(emu, PARAM) };
    if hits.is_empty() {
        return ret(emu, ITEM_NOT_FOUND);
    }
    let mut i = 0;
    emu.keychain.items.retain(|_| {
        let keep = !hits.contains(&i);
        i += 1;
        keep
    });
    ret(emu, SUCCESS)
}

/// SecAccessControlCreateFlags that ask for the user: biometry (any or current set),
/// the device passcode, or either.
const USER_PRESENCE: u64 = 1;
const BIOMETRY_ANY: u64 = 1 << 1;
const BIOMETRY_CURRENT_SET: u64 = 1 << 3;
const DEVICE_PASSCODE: u64 = 1 << 4;

fn biometry_type(emu: &Emu) -> u64 {
    // LABiometryType: 1 Touch ID, 2 Face ID.
    if emu.device.home_button { 1 } else { 2 }
}

/// An NSError in LAErrorDomain.
fn la_error(emu: &mut Emu, code: i64) -> Result<u64> {
    let domain = foundation::new_nsstring(emu, "com.apple.LocalAuthentication")?;
    let cls = host_class(emu, "NSError")?;
    send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, code as u64, 0])
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_SecItemAdd", item_add),
    ("_SecItemCopyMatching", copy_matching),
    ("_SecItemUpdate", item_update),
    ("_SecItemDelete", item_delete),
    // (allocator, protection, flags, &error)
    ("_SecAccessControlCreateWithFlags", |e| {
        let flags = e.arg(2);
        let cls = host_class(e, "SecAccessControl")?;
        let ac = objc::create_instance(e, cls, 0)?;
        let asks = flags & (USER_PRESENCE | BIOMETRY_ANY | BIOMETRY_CURRENT_SET | DEVICE_PASSCODE) != 0;
        e.keychain.access_controls.insert(ac, asks);
        e.ret(ac);
        Ok(())
    }),
];

const LA: &str = "LAContext";

fn la_get(emu: &mut Emu, key: &str) -> Result<()> {
    let v = objc::bag_get(emu, emu.arg(0), key).unwrap_or(0);
    emu.ret(v);
    Ok(())
}

fn la_set(emu: &mut Emu, key: &str) -> Result<()> {
    let (o, v) = (emu.arg(0), emu.arg(2));
    objc::bag_set(emu, o, key, v);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // LAPolicy 1: biometrics only; 2: biometrics or the device passcode. With no
    // passcode set (a fresh Simulator), both come down to enrolled biometry.
    (LA, false, "canEvaluatePolicy:error:", |e| {
        let (policy, error) = (e.arg(2), e.arg(3));
        let ok = e.keychain.biometry != Biometry::NotEnrolled;
        if !ok && error != 0 {
            // LAErrorPasscodeNotSet (-5) for the passcode policy, LAErrorBiometryNotEnrolled (-7) for biometrics.
            let err = la_error(e, if policy == 2 { -5 } else { -7 })?;
            e.write_u64(error, err)?;
        }
        e.ret(ok as u64);
        Ok(())
    }),
    (LA, false, "biometryType", |e| {
        let t = biometry_type(e);
        e.ret(t);
        Ok(())
    }),
    // (policy, reason, reply(success, error)), answered on another queue as LocalAuthentication does.
    (LA, false, "evaluatePolicy:localizedReason:reply:", |e| {
        let (policy, reason, block) = (e.arg(2), e.arg(3), e.arg(4));
        let block = crate::libc::copy_block(e, block)?;
        match e.keychain.biometry {
            Biometry::Enrolled => {
                show(e, Prompt::Reply { block, policy }, reason)?;
                Ok(())
            }
            Biometry::Matching => reply(e, block, 0),
            Biometry::NonMatching | Biometry::NotEnrolled => {
                let code = failure(e, policy);
                reply(e, block, code)
            }
        }
    }),
    // The prompt's labels and options: kept for the app to read back.
    (LA, false, "localizedFallbackTitle", |e| la_get(e, "localizedFallbackTitle")),
    (LA, false, "setLocalizedFallbackTitle:", |e| la_set(e, "localizedFallbackTitle")),
    (LA, false, "localizedCancelTitle", |e| la_get(e, "localizedCancelTitle")),
    (LA, false, "setLocalizedCancelTitle:", |e| la_set(e, "localizedCancelTitle")),
    (LA, false, "localizedReason", |e| la_get(e, "localizedReason")),
    (LA, false, "setLocalizedReason:", |e| la_set(e, "localizedReason")),
    (LA, false, "interactionNotAllowed", |e| la_get(e, "interactionNotAllowed")),
    (LA, false, "setInteractionNotAllowed:", |e| la_set(e, "interactionNotAllowed")),
    (LA, false, "invalidate", |_| Ok(())),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_and_keys() {
        assert!(is_control("r_Data") && is_control("m_Limit") && is_control("class") && is_control("v_Data"));
        assert!(!is_control("svce") && !is_control("acct"));
        assert!(primary_key("genp").contains(&"svce"));
        assert_eq!("match".parse::<Biometry>(), Ok(Biometry::Matching));
    }
}
