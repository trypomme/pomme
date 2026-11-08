//! CoreText, as far as Skia's font manager (SkFontMgr_Mac) needs it.
//!
//! The font collection is empty for now: Skia starts up and draws shapes,
//! paths and images; Skia text will want the system fonts (Inter, the emoji
//! font) listed here as CTFontDescriptors.

use crate::{Emu, HostFn, Result, foundation};

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// An (empty) array, standing in for a CTFontCollection or a list of descriptors.
fn empty_array(emu: &mut Emu) -> Result<()> {
    let a = foundation::new_array_of(emu, &[])?;
    ret(emu, a)
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CTFontCollectionCreateFromAvailableFonts", empty_array),
    ("_CTFontCollectionCreateMatchingFontDescriptors", empty_array),
    ("_CTFontCollectionCreateMatchingFontDescriptorsForFamily", empty_array),
    ("_CTFontCollectionCopyFontAttribute", empty_array),
];
