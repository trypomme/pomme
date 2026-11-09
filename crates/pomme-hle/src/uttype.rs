//! Uniform Type Identifiers (CoreServices' UTType functions): the system's
//! well-known types with their filename extensions, MIME types and the types
//! they conform to. A tag no type claims gets a dynamic identifier, as on iOS.

use crate::foundation;
use crate::{Emu, HostFn, Result};

/// (identifier, filename extensions, MIME types, conforms to).
const TYPES: &[(&str, &[&str], &[&str], &[&str])] = &[
    ("public.item", &[], &[], &[]),
    ("public.content", &[], &[], &[]),
    ("public.data", &[], &["application/octet-stream"], &["public.item"]),
    ("public.text", &["txt"], &["text/plain"], &["public.data", "public.content"]),
    ("public.plain-text", &["txt", "text"], &["text/plain"], &["public.text"]),
    ("public.utf8-plain-text", &[], &[], &["public.plain-text"]),
    ("public.utf16-plain-text", &[], &[], &["public.plain-text"]),
    ("public.html", &["html", "htm"], &["text/html"], &["public.text"]),
    ("public.xml", &["xml"], &["application/xml", "text/xml"], &["public.text"]),
    ("public.json", &["json"], &["application/json"], &["public.text"]),
    ("public.rtf", &["rtf"], &["text/rtf"], &["public.text"]),
    ("public.url", &[], &[], &["public.data"]),
    ("public.file-url", &[], &[], &["public.url"]),
    ("com.adobe.pdf", &["pdf"], &["application/pdf"], &["public.data", "public.composite-content"]),
    ("public.image", &[], &[], &["public.data", "public.content"]),
    ("public.png", &["png"], &["image/png"], &["public.image"]),
    ("public.jpeg", &["jpeg", "jpg", "jpe"], &["image/jpeg", "image/jpg"], &["public.image"]),
    ("com.compuserve.gif", &["gif"], &["image/gif"], &["public.image"]),
    ("org.webmproject.webp", &["webp"], &["image/webp"], &["public.image"]),
    ("public.heic", &["heic"], &["image/heic"], &["public.heif-standard"]),
    ("public.heif", &["heif"], &["image/heif"], &["public.heif-standard"]),
    ("public.heif-standard", &[], &[], &["public.image"]),
    ("public.avif", &["avif"], &["image/avif"], &["public.heif-standard"]),
    ("public.tiff", &["tiff", "tif"], &["image/tiff"], &["public.image"]),
    ("com.microsoft.bmp", &["bmp"], &["image/bmp"], &["public.image"]),
    ("com.microsoft.ico", &["ico"], &["image/vnd.microsoft.icon"], &["public.image"]),
    ("public.svg-image", &["svg"], &["image/svg+xml"], &["public.image", "public.xml"]),
    ("public.audiovisual-content", &[], &[], &["public.data", "public.content"]),
    ("public.movie", &[], &[], &["public.audiovisual-content"]),
    ("public.video", &[], &[], &["public.movie"]),
    ("public.mpeg-4", &["mp4"], &["video/mp4"], &["public.movie"]),
    ("com.apple.quicktime-movie", &["mov"], &["video/quicktime"], &["public.movie"]),
    ("public.audio", &[], &[], &["public.audiovisual-content"]),
    ("public.mp3", &["mp3"], &["audio/mpeg"], &["public.audio"]),
    ("com.apple.flat-rtfd", &["rtfd"], &[], &["public.data"]),
];

const EXTENSION: &str = "public.filename-extension";
const MIME: &str = "public.mime-type";

fn find(id: &str) -> Option<&'static (&'static str, &'static [&'static str], &'static [&'static str], &'static [&'static str])> {
    TYPES.iter().find(|t| t.0 == id)
}

/// A type's preferred MIME type.
pub fn mime_of(id: &str) -> Option<&'static str> {
    find(id).and_then(|t| t.2.first().copied())
}

/// Whether `id` is `other` or conforms to it, through its parents.
pub fn conforms(id: &str, other: &str) -> bool {
    if id == other {
        return true;
    }
    find(id).is_some_and(|t| t.3.iter().any(|p| conforms(p, other)))
}

/// The type for a filename extension or MIME type (that conforms to `conforming`, if given).
pub fn for_tag(class: &str, tag: &str, conforming: Option<&str>) -> Option<&'static str> {
    let tag = tag.to_ascii_lowercase();
    TYPES
        .iter()
        .filter(|t| conforming.is_none_or(|c| conforms(t.0, c)))
        .find(|t| match class {
            EXTENSION => t.1.contains(&tag.as_str()),
            MIME => t.2.contains(&tag.as_str()),
            _ => false,
        })
        .map(|t| t.0)
}

fn string_arg(emu: &mut Emu, i: usize) -> Result<Option<String>> {
    let v = emu.arg(i);
    if v == 0 { Ok(None) } else { foundation::nsstring(emu, v).map(Some) }
}

fn ret_string(emu: &mut Emu, s: Option<&str>) -> Result<()> {
    let v = match s {
        Some(s) => foundation::new_nsstring(emu, s)?,
        None => 0,
    };
    emu.ret(v);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    // (tagClass, tag, conformingToUTI) → a new string; unknown tags get a dynamic type.
    ("_UTTypeCreatePreferredIdentifierForTag", |e| {
        let (class, tag, conforming) = (string_arg(e, 0)?.unwrap_or_default(), string_arg(e, 1)?.unwrap_or_default(), string_arg(e, 2)?);
        let id = match for_tag(&class, &tag, conforming.as_deref()) {
            Some(id) => id.to_string(),
            None => format!("dyn.{}", tag.bytes().map(|b| format!("{b:02x}")).collect::<String>()),
        };
        ret_string(e, Some(&id))
    }),
    // (UTI, tagClass) → its preferred extension or MIME type.
    ("_UTTypeCopyPreferredTagWithClass", |e| {
        let (id, class) = (string_arg(e, 0)?.unwrap_or_default(), string_arg(e, 1)?.unwrap_or_default());
        let tag = find(&id).and_then(|t| match class.as_str() {
            EXTENSION => t.1.first().copied(),
            MIME => t.2.first().copied(),
            _ => None,
        });
        ret_string(e, tag)
    }),
    ("_UTTypeConformsTo", |e| {
        let (a, b) = (string_arg(e, 0)?.unwrap_or_default(), string_arg(e, 1)?.unwrap_or_default());
        e.ret(conforms(&a, &b) as u64);
        Ok(())
    }),
    ("_UTTypeEqual", |e| {
        let (a, b) = (string_arg(e, 0)?, string_arg(e, 1)?);
        e.ret((a.is_some() && a == b) as u64);
        Ok(())
    }),
    ("_UTTypeIsDynamic", |e| {
        let id = string_arg(e, 0)?.unwrap_or_default();
        e.ret(id.starts_with("dyn.") as u64);
        Ok(())
    }),
    ("_UTTypeIsDeclared", |e| {
        let id = string_arg(e, 0)?.unwrap_or_default();
        e.ret(find(&id).is_some() as u64);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_conformance() {
        assert_eq!(for_tag(EXTENSION, "JPG", None), Some("public.jpeg"));
        assert_eq!(for_tag(MIME, "image/png", Some("public.image")), Some("public.png"));
        assert_eq!(for_tag(EXTENSION, "txt", Some("public.image")), None);
        assert!(conforms("public.png", "public.data"));
        assert!(conforms("public.utf8-plain-text", "public.text"));
        assert!(!conforms("public.text", "public.image"));
    }
}
