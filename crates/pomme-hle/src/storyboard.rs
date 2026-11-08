//! Storyboards: compiled `.storyboardc` directories of `NIBArchive` files.
//!
//! A storyboard's Info.plist names its entry point and maps view controller
//! identifiers to nibs. pomme reads the nib's object graph far enough to find
//! the view controller's class (a `UIClassSwapper` names the custom class, as
//! in Flutter's `Main.storyboard`) and makes it the way UIKit does, with
//! `initWithCoder:`. The coder answers "no value" for every key for now:
//! properties archived in the nib (titles, colors, outlets) aren't restored.

use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result, foundation};

/// A value in a NIBArchive.
#[derive(Debug, Clone, PartialEq)]
pub enum NibValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    Data(Vec<u8>),
    Nil,
    Object(u32),
}

#[derive(Debug, Clone)]
pub struct NibObject {
    pub class: String,
    pub values: Vec<(String, NibValue)>,
}

/// Parse a NIBArchive (`UINib`'s compiled format).
pub fn parse_nib(b: &[u8]) -> Option<Vec<NibObject>> {
    if !b.starts_with(b"NIBArchive") {
        return None;
    }
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize);
    let h = |i: usize| u32_at(10 + 4 * i);
    let (n_obj, obj_off, n_keys, keys_off, n_vals, vals_off, n_cls, cls_off) = (h(2)?, h(3)?, h(4)?, h(5)?, h(6)?, h(7)?, h(8)?, h(9)?);
    // Variable-length integers: 7 bits per byte, the high bit marks the last byte.
    let varint = |o: &mut usize| -> Option<usize> {
        let (mut v, mut shift) = (0usize, 0);
        loop {
            let byte = *b.get(*o)?;
            *o += 1;
            v |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 != 0 {
                return Some(v);
            }
            shift += 7;
        }
    };
    let mut o = keys_off;
    let mut keys = Vec::with_capacity(n_keys);
    for _ in 0..n_keys {
        let len = varint(&mut o)?;
        keys.push(String::from_utf8_lossy(b.get(o..o + len)?).into_owned());
        o += len;
    }
    let mut o = cls_off;
    let mut classes = Vec::with_capacity(n_cls);
    for _ in 0..n_cls {
        let len = varint(&mut o)?;
        let extras = varint(&mut o)?;
        o += extras * 4;
        let name = b.get(o..o + len)?;
        classes.push(String::from_utf8_lossy(name).trim_end_matches('\0').to_string());
        o += len;
    }
    let mut o = vals_off;
    let mut values = Vec::with_capacity(n_vals);
    for _ in 0..n_vals {
        let key = keys.get(varint(&mut o)?)?.clone();
        let ty = *b.get(o)?;
        o += 1;
        let v = match ty {
            0 => {
                o += 1;
                NibValue::Int(b[o - 1] as i8 as i64)
            }
            1 => {
                o += 2;
                NibValue::Int(i16::from_le_bytes(b.get(o - 2..o)?.try_into().ok()?) as i64)
            }
            2 => {
                o += 4;
                NibValue::Int(i32::from_le_bytes(b.get(o - 4..o)?.try_into().ok()?) as i64)
            }
            3 => {
                o += 8;
                NibValue::Int(i64::from_le_bytes(b.get(o - 8..o)?.try_into().ok()?))
            }
            4 => NibValue::Bool(true),
            5 => NibValue::Bool(false),
            6 => {
                o += 4;
                NibValue::Float(f32::from_le_bytes(b.get(o - 4..o)?.try_into().ok()?) as f64)
            }
            7 => {
                o += 8;
                NibValue::Float(f64::from_le_bytes(b.get(o - 8..o)?.try_into().ok()?))
            }
            8 => {
                let len = varint(&mut o)?;
                o += len;
                NibValue::Data(b.get(o - len..o)?.to_vec())
            }
            9 => NibValue::Nil,
            10 => {
                o += 4;
                NibValue::Object(u32::from_le_bytes(b.get(o - 4..o)?.try_into().ok()?))
            }
            _ => return None,
        };
        values.push((key, v));
    }
    let mut o = obj_off;
    let mut objects = Vec::with_capacity(n_obj);
    for _ in 0..n_obj {
        let class = classes.get(varint(&mut o)?)?.clone();
        let start = varint(&mut o)?;
        let count = varint(&mut o)?;
        objects.push(NibObject { class, values: values.get(start..start + count)?.to_vec() });
    }
    Some(objects)
}

/// The view controller class a storyboard nib makes: a swapped-in custom class,
/// or the first view controller archived.
pub fn view_controller_class(objects: &[NibObject]) -> Option<String> {
    let text = |o: &NibObject, key: &str| {
        o.values.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
            NibValue::Object(i) => objects.get(*i as usize).and_then(|s| s.values.iter().find_map(|(k, v)| (k == "NS.bytes").then_some(v))).and_then(|v| match v {
                NibValue::Data(d) => Some(String::from_utf8_lossy(d).into_owned()),
                _ => None,
            }),
            NibValue::Data(d) => Some(String::from_utf8_lossy(d).into_owned()),
            _ => None,
        })
    };
    if let Some(name) = objects.iter().filter(|o| o.class == "UIClassSwapper").find_map(|o| text(o, "UIClassName")) {
        return Some(name);
    }
    objects.iter().find(|o| o.class.ends_with("ViewController")).map(|o| o.class.clone())
}

// ---- UIStoryboard ----

/// A storyboard object: its compiled directory's guest path.
fn storyboard_dir(emu: &Emu, sb: u64) -> Option<String> {
    emu.ui.storyboards.get(&sb).cloned()
}

/// `+storyboardWithName:bundle:` — `<bundle>/Base.lproj/<name>.storyboardc` or `<bundle>/<name>.storyboardc`.
fn storyboard_with_name(emu: &mut Emu) -> Result<()> {
    let name = foundation::nsstring(emu, emu.arg(2))?;
    let bundle = crate::fs::guest_bundle_path(emu);
    let dir = [format!("{bundle}/Base.lproj/{name}.storyboardc"), format!("{bundle}/{name}.storyboardc")]
        .into_iter()
        .find(|d| crate::fs::read_file(emu, &format!("{d}/Info.plist")).is_some());
    let Some(dir) = dir else {
        emu.ret(0);
        return Ok(());
    };
    let cls = host_class(emu, "UIStoryboard")?;
    let sb = objc::create_instance(emu, cls, 0)?;
    emu.ui.storyboards.insert(sb, dir);
    emu.ret(sb);
    Ok(())
}

fn storyboard_info(emu: &mut Emu, dir: &str) -> Option<plist::Dictionary> {
    let bytes = crate::fs::read_file(emu, &format!("{dir}/Info.plist"))?;
    plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()?.into_dictionary()
}

/// Make the view controller with storyboard identifier `id` (or the entry point).
pub fn instantiate(emu: &mut Emu, dir: &str, id: Option<&str>) -> Result<u64> {
    let Some(info) = storyboard_info(emu, dir) else { return Ok(0) };
    let id = match id {
        Some(i) => i.to_string(),
        None => match info.get("UIStoryboardDesignatedEntryPointIdentifier").and_then(|v| v.as_string()) {
            Some(i) => i.to_string(),
            None => return Ok(0),
        },
    };
    let nib = info
        .get("UIViewControllerIdentifiersToNibNames")
        .and_then(|v| v.as_dictionary())
        .and_then(|m| m.get(&id))
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .unwrap_or(id);
    let Some(bytes) = crate::fs::read_file(emu, &format!("{dir}/{nib}.nib")) else { return Ok(0) };
    let objects = parse_nib(&bytes).unwrap_or_default();
    let class = view_controller_class(&objects).unwrap_or_else(|| "UIViewController".into());
    let cls = objc::class_by_name(emu, &class)?;
    if cls == 0 {
        println!("[storyboard] {dir}: no class {class}");
        return Ok(0);
    }
    let coder = {
        let c = host_class(emu, "UINibDecoder")?;
        objc::create_instance(emu, c, 0)?
    };
    let vc = send(emu, cls, "alloc", &[])?;
    let vc = send(emu, vc, "initWithCoder:", &[coder])?;
    if vc != 0 {
        send(emu, vc, "awakeFromNib", &[])?;
    }
    Ok(vc)
}

fn instantiate_initial(emu: &mut Emu) -> Result<()> {
    let sb = emu.arg(0);
    let vc = match storyboard_dir(emu, sb) {
        Some(dir) => instantiate(emu, &dir, None)?,
        None => 0,
    };
    emu.ret(vc);
    Ok(())
}

fn instantiate_with_identifier(emu: &mut Emu) -> Result<()> {
    let (sb, id) = (emu.arg(0), emu.arg(2));
    let id = foundation::nsstring(emu, id)?;
    let vc = match storyboard_dir(emu, sb) {
        Some(dir) => instantiate(emu, &dir, Some(&id))?,
        None => 0,
    };
    emu.ret(vc);
    Ok(())
}

/// The storyboard directory a scene configuration names (`UISceneStoryboardFile`), if any.
pub fn scene_storyboard(emu: &mut Emu) -> Option<String> {
    let info = crate::bundle::host_info_plist(emu)?;
    let configs = info.as_dictionary()?.get("UIApplicationSceneManifest")?.as_dictionary()?.get("UISceneConfigurations")?.as_dictionary()?;
    let first = configs.get("UIWindowSceneSessionRoleApplication")?.as_array()?.first()?.as_dictionary()?;
    let name = first.get("UISceneStoryboardFile")?.as_string()?.to_string();
    let bundle = crate::fs::guest_bundle_path(emu);
    [format!("{bundle}/Base.lproj/{name}.storyboardc"), format!("{bundle}/{name}.storyboardc")]
        .into_iter()
        .find(|d| crate::fs::read_file(emu, &format!("{d}/Info.plist")).is_some())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIStoryboard", true, "storyboardWithName:bundle:", storyboard_with_name),
    ("UIStoryboard", false, "instantiateInitialViewController", instantiate_initial),
    ("UIStoryboard", false, "instantiateViewControllerWithIdentifier:", instantiate_with_identifier),
    // The coder storyboards hand to initWithCoder: (no archived values yet).
    ("UINibDecoder", false, "containsValueForKey:", |e| {
        e.ret(0);
        Ok(())
    }),
    ("UINibDecoder", false, "allowsKeyedCoding", |e| {
        e.ret(1);
        Ok(())
    }),
    ("UIViewController", false, "initWithCoder:", |e| {
        let vc = e.arg(0);
        e.ret(vc);
        Ok(())
    }),
    ("UIViewController", false, "awakeFromNib", |_| Ok(())),
    ("UIViewController", false, "storyboard", |e| {
        e.ret(0);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Flutter's Main.storyboard entry point: a UIViewController swapped for FlutterViewController.
    #[test]
    fn finds_the_flutter_view_controller() {
        let nib = include_bytes!("../tests/fixtures/flutter-main-vc.nib");
        let objects = parse_nib(nib).unwrap();
        assert!(objects.len() > 5, "{objects:?}");
        assert!(objects.iter().any(|o| o.class == "UIClassSwapper"));
        assert_eq!(view_controller_class(&objects).as_deref(), Some("FlutterViewController"));
    }
}
