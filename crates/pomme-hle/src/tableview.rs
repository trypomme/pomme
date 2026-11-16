//! UITableView: rows from the app's data source, laid out one after another
//! in sections (plain, grouped, inset grouped), with the standard cells,
//! headers and footers, scrolling (it's a scroll view), reuse and selection.
//! NSIndexPath too.
//!
//! Every number is the iOS 26 Simulator's (fixtures/tableview, light): a row
//! is 52 pt (62 for a subtitle cell with detail text); in a plain table a
//! titled header is 28 pt, 22 pt below its section's top, and a footer 28;
//! in a grouped one the first titled header is 55.33 pt and the others
//! 45.33, a titled footer 30, and without titles the first section starts
//! 35 pt down, later headers are 17.67 and footers 17.33; inset grouped rows
//! are 16 pt in from each side, the first and last of a section rounded
//! (26 pt). Cells lay out their parts as recorded for each style, mirrored
//! in a right-to-left layout (fixtures/rtlcontrols).
//!
//! The cells are the app's own objects (its subclasses too): the table asks
//! for the rows on screen only, puts each at its row, and keeps cells that
//! leave the screen for `dequeueReusableCellWithIdentifier:`. A row's
//! height is the delegate's, else the table's `rowHeight`, else the cell's
//! own (measured once it's made). Not here: editing, animated updates,
//! self-sizing from Auto Layout, sticky plain headers.

use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class, send, send_fp};
use crate::uikit::ret_rect;
use crate::uiview::{view, view_mut};
use crate::{Emu, HostFn, Result};
use std::collections::{BTreeMap, HashMap};

// ---- measured (fixtures/tableview) ----

const ROW: f64 = 52.0;
const SUBTITLE_ROW: f64 = 62.0;
const PLAIN_HEADER: f64 = 28.0;
const PLAIN_HEADER_PAD: f64 = 22.0;
const PLAIN_FOOTER: f64 = 28.0;
const GROUPED_FIRST_HEADER: f64 = 166.0 / 3.0;
const GROUPED_HEADER: f64 = 136.0 / 3.0;
const GROUPED_FOOTER: f64 = 30.0;
const GROUPED_TOP: f64 = 35.0;
const GROUPED_BARE_HEADER: f64 = 53.0 / 3.0;
const GROUPED_BARE_FOOTER: f64 = 52.0 / 3.0;
/// Below a grouped table's last section (its content 20 pt past the last footer).
const GROUPED_BOTTOM: f64 = 20.0;
const INSET: f64 = 16.0;
const INSET_RADIUS: f64 = 26.0;
const MARGIN: f64 = 16.0;
const SEPARATOR: [f64; 4] = [60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 31.0 / 255.0];
const SECONDARY: [f64; 4] = [60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.6];
const TERTIARY: [f64; 4] = [60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.3];
const SELECTED: [f64; 4] = [217.0 / 255.0, 217.0 / 255.0, 221.0 / 255.0, 1.0];
const GROUPED_BACKGROUND: [f64; 4] = [242.0 / 255.0, 242.0 / 255.0, 247.0 / 255.0, 1.0];
const VALUE2_TITLE: [f64; 4] = [0.0, 136.0 / 255.0, 1.0, 1.0];
/// The disclosure chevron's size, and the gap from it to the edge.
const DISCLOSURE: [f64; 2] = [31.0 / 3.0, 14.0];

/// UITableViewStyle.
const PLAIN: u64 = 0;
const INSET_GROUPED: u64 = 2;

#[derive(Clone, Debug, Default)]
struct Section {
    top: f64,
    header: (f64, f64),
    header_title: Option<String>,
    rows: Vec<(f64, f64)>,
    footer: (f64, f64),
    footer_title: Option<String>,
    bottom: f64,
}

#[derive(Clone, Debug, Default)]
struct Table {
    sections: Vec<Section>,
    dirty: bool,
    content_height: f64,
    /// Rows on screen → their cells.
    visible: BTreeMap<(usize, usize), u64>,
    /// Cells that left the screen, by reuse identifier.
    pool: HashMap<String, Vec<u64>>,
    /// Reuse identifier → class (`registerClass:forCellReuseIdentifier:`).
    registered: HashMap<String, u64>,
    /// (section, footer?) → its header or footer view.
    headers: HashMap<(usize, bool), u64>,
    /// Heights cells have of themselves, once made.
    natural: HashMap<(usize, usize), f64>,
    selected: Option<(usize, usize)>,
}

#[derive(Clone, Default)]
pub struct State {
    tables: HashMap<u64, Table>,
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_f64(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn get_f64(emu: &Emu, v: u64, key: &str, default: f64) -> f64 {
    objc::bag_get_f64(emu, v, key).unwrap_or(default)
}

fn set_f64(emu: &mut Emu, v: u64, key: &str, x: f64) {
    emu.objc.bag.insert((v, key.to_string()), (0, x.to_bits() as u128));
}

fn table(emu: &mut Emu, t: u64) -> &mut Table {
    emu.tables.tables.entry(t).or_insert_with(|| Table { dirty: true, ..Default::default() })
}

fn style(emu: &Emu, t: u64) -> u64 {
    objc::bag_get(emu, t, "pomme.tableStyle").unwrap_or(PLAIN)
}

fn new_of(emu: &mut Emu, class: &str, frame: [f64; 4]) -> Result<u64> {
    let cls = host_class(emu, class)?;
    let v = send(emu, cls, "alloc", &[])?;
    objc::send_fp_id(emu, v, "initWithFrame:", &[], &frame)
}

fn color(emu: &mut Emu, rgba: [f64; 4]) -> Result<u64> {
    crate::uiview::new_color(emu, rgba)
}

// ---- NSIndexPath ----

pub(crate) fn index_path(emu: &mut Emu, section: usize, row: usize) -> Result<u64> {
    let cls = host_class(emu, "NSIndexPath")?;
    let p = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, p, "section", section as u64);
    objc::bag_set(emu, p, "row", row as u64);
    Ok(p)
}

fn path_of(emu: &Emu, p: u64) -> Option<(usize, usize)> {
    if p == 0 {
        return None;
    }
    Some((objc::bag_get(emu, p, "section").unwrap_or(0) as usize, objc::bag_get(emu, p, "row").unwrap_or(0) as usize))
}

// ---- asking the app ----

fn data_source(emu: &Emu, t: u64) -> u64 {
    objc::bag_get(emu, t, "dataSource").unwrap_or(0)
}

fn delegate(emu: &Emu, t: u64) -> u64 {
    objc::bag_get(emu, t, "delegate").unwrap_or(0)
}

fn responds(emu: &mut Emu, obj: u64, sel: &str) -> Result<bool> {
    if obj == 0 {
        return Ok(false);
    }
    let s = objc::intern(emu, sel)?;
    objc::responds(emu, obj, s)
}

fn sections_count(emu: &mut Emu, t: u64) -> Result<usize> {
    let ds = data_source(emu, t);
    if responds(emu, ds, "numberOfSectionsInTableView:")? {
        return Ok(send(emu, ds, "numberOfSectionsInTableView:", &[t])? as i64 as usize);
    }
    Ok(if ds == 0 { 0 } else { 1 })
}

fn rows_count(emu: &mut Emu, t: u64, s: usize) -> Result<usize> {
    let ds = data_source(emu, t);
    if !responds(emu, ds, "tableView:numberOfRowsInSection:")? {
        return Ok(0);
    }
    Ok((send(emu, ds, "tableView:numberOfRowsInSection:", &[t, s as u64])? as i64).max(0) as usize)
}

fn title(emu: &mut Emu, t: u64, s: usize, footer: bool) -> Result<Option<String>> {
    let ds = data_source(emu, t);
    let sel = if footer { "tableView:titleForFooterInSection:" } else { "tableView:titleForHeaderInSection:" };
    if !responds(emu, ds, sel)? {
        return Ok(None);
    }
    let x = send(emu, ds, sel, &[t, s as u64])?;
    Ok(if x == 0 { None } else { Some(nsstring(emu, x)?).filter(|s| !s.is_empty()) })
}

/// A delegate's height for a section's header or footer, or a row (None: it gives none).
fn delegate_height(emu: &mut Emu, t: u64, sel: &str, arg: u64) -> Result<Option<f64>> {
    let dl = delegate(emu, t);
    if !responds(emu, dl, sel)? {
        return Ok(None);
    }
    let h = send_fp(emu, dl, sel, &[t, arg], &[])?[0];
    Ok((h >= 0.0).then_some(h))
}

// ---- geometry ----

fn compute(emu: &mut Emu, t: u64) -> Result<()> {
    let st = style(emu, t);
    let n = sections_count(emu, t)?;
    let header_view = objc::bag_get(emu, t, "tableHeaderView").unwrap_or(0);
    let mut y = if header_view != 0 { view(emu, header_view).bounds[3] } else { 0.0 };
    let row_height = get_f64(emu, t, "rowHeight", -1.0);
    let mut sections = Vec::with_capacity(n);
    for s in 0..n {
        let mut sec = Section { top: y, ..Default::default() };
        sec.header_title = title(emu, t, s, false)?;
        sec.footer_title = title(emu, t, s, true)?;
        let titled = sec.header_title.is_some();
        let given = delegate_height(emu, t, "tableView:heightForHeaderInSection:", s as u64)?;
        if st == PLAIN {
            let h = given.unwrap_or(if titled { PLAIN_HEADER } else { 0.0 });
            let pad = if h > 0.0 { PLAIN_HEADER_PAD } else { 0.0 };
            sec.header = (y + pad, h);
            y += pad + h;
        } else {
            if s == 0 && !titled && given.is_none() {
                // (Untitled, the first section starts lower: its header is empty.)
                y += GROUPED_TOP;
                sec.top = y;
            }
            let h = given.unwrap_or(match (titled, s) {
                (true, 0) => GROUPED_FIRST_HEADER,
                (true, _) => GROUPED_HEADER,
                (false, 0) => 0.0,
                (false, _) => GROUPED_BARE_HEADER,
            });
            sec.header = (y, h);
            y += h;
        }
        let rows = rows_count(emu, t, s)?;
        for r in 0..rows {
            let h = match delegate_height(emu, t, "tableView:heightForRowAtIndexPath:", 0)? {
                Some(_) => {
                    let p = index_path(emu, s, r)?;
                    delegate_height(emu, t, "tableView:heightForRowAtIndexPath:", p)?.unwrap_or(ROW)
                }
                None if row_height >= 0.0 => row_height,
                None => table(emu, t).natural.get(&(s, r)).copied().unwrap_or(ROW),
            };
            sec.rows.push((y, h));
            y += h;
        }
        let fgiven = delegate_height(emu, t, "tableView:heightForFooterInSection:", s as u64)?;
        let fh = fgiven.unwrap_or(match (st == PLAIN, sec.footer_title.is_some()) {
            (true, true) => PLAIN_FOOTER,
            (true, false) => 0.0,
            (false, true) => GROUPED_FOOTER,
            (false, false) => GROUPED_BARE_FOOTER,
        });
        sec.footer = (y, fh);
        y += fh;
        sec.bottom = y;
        sections.push(sec);
    }
    if st != PLAIN && n > 0 {
        y += GROUPED_BOTTOM;
    }
    let footer_view = objc::bag_get(emu, t, "tableFooterView").unwrap_or(0);
    if footer_view != 0 {
        let h = view(emu, footer_view).bounds[3];
        let w = view(emu, t).bounds[2];
        crate::uiview::set_frame_of(emu, footer_view, [0.0, y, w, h]);
        y += h;
    }
    if header_view != 0 {
        let (w, h) = (view(emu, t).bounds[2], view(emu, header_view).bounds[3]);
        crate::uiview::set_frame_of(emu, header_view, [0.0, 0.0, w, h]);
    }
    let tb = table(emu, t);
    tb.sections = sections;
    tb.content_height = y;
    tb.dirty = false;
    let w = view(emu, t).bounds[2];
    objc::send_fp(emu, t, "setContentSize:", &[], &[w, y])?;
    Ok(())
}

fn ensure(emu: &mut Emu, t: u64) -> Result<()> {
    if table(emu, t).dirty {
        compute(emu, t)?;
    }
    Ok(())
}

/// A row's rect in the table's content.
fn row_rect(emu: &mut Emu, t: u64, s: usize, r: usize) -> [f64; 4] {
    let w = view(emu, t).bounds[2];
    let inset = if style(emu, t) == INSET_GROUPED { INSET } else { 0.0 };
    match table(emu, t).sections.get(s).and_then(|sec| sec.rows.get(r)) {
        Some(&(y, h)) => [inset, y, w - 2.0 * inset, h],
        None => [0.0; 4],
    }
}

// ---- cells ----

/// UITableViewCellStyle: default 0, value 1 (1), value 2 (2), subtitle (3).
fn cell_style(emu: &Emu, c: u64) -> u64 {
    objc::bag_get(emu, c, "pomme.cellStyle").unwrap_or(0)
}

/// A cell's part, made on first use (`pomme.part.<key>`).
fn cell_part(emu: &mut Emu, c: u64, key: &str, class: &str, parent: Option<u64>) -> Result<u64> {
    let k = format!("pomme.cellPart.{key}");
    if let Some(p) = objc::bag_get(emu, c, &k).filter(|&p| p != 0) {
        return Ok(p);
    }
    let p = new_of(emu, class, [0.0; 4])?;
    objc::bag_set(emu, c, &k, p);
    send(emu, parent.unwrap_or(c), "addSubview:", &[p])?;
    Ok(p)
}

fn content_view(emu: &mut Emu, c: u64) -> Result<u64> {
    // (Behind it, the cell's background.)
    let bg = cell_part(emu, c, "background", "UIView", None)?;
    view_mut(emu, bg).user_interaction = false;
    let cv = cell_part(emu, c, "content", "UIView", None)?;
    Ok(cv)
}

fn label_font(emu: &mut Emu, l: u64, size: f64, weight: f64, rgba: [f64; 4]) -> Result<()> {
    let font = crate::uiview::new_font(emu, if weight > 0.0 { ".SFUI-Semibold" } else { ".SFUI-Regular" }, size, weight)?;
    objc::bag_set(emu, l, "font", font);
    let c = color(emu, rgba)?;
    objc::bag_set(emu, l, "textColor", c);
    Ok(())
}

fn text_label(emu: &mut Emu, c: u64) -> Result<u64> {
    let cv = content_view(emu, c)?;
    let k = "pomme.cellPart.text";
    if let Some(l) = objc::bag_get(emu, c, k).filter(|&p| p != 0) {
        return Ok(l);
    }
    let l = cell_part(emu, c, "text", "UILabel", Some(cv))?;
    let (size, rgba) = if cell_style(emu, c) == 2 { (13.0, VALUE2_TITLE) } else { (17.0, [0.0, 0.0, 0.0, 1.0]) };
    label_font(emu, l, size, 0.0, rgba)?;
    if cell_style(emu, c) == 2 {
        objc::bag_set(emu, l, "textAlignment", 2);
    }
    Ok(l)
}

fn detail_label(emu: &mut Emu, c: u64) -> Result<u64> {
    let st = cell_style(emu, c);
    if st == 0 {
        return Ok(0);
    }
    if let Some(l) = objc::bag_get(emu, c, "pomme.cellPart.detail").filter(|&p| p != 0) {
        return Ok(l);
    }
    let cv = content_view(emu, c)?;
    let l = cell_part(emu, c, "detail", "UILabel", Some(cv))?;
    let (size, rgba) = match st {
        1 => (17.0, SECONDARY),
        2 => (13.0, [0.0, 0.0, 0.0, 1.0]),
        _ => (15.0, SECONDARY),
    };
    label_font(emu, l, size, 0.0, rgba)?;
    Ok(l)
}

fn image_view(emu: &mut Emu, c: u64) -> Result<u64> {
    let cv = content_view(emu, c)?;
    cell_part(emu, c, "image", "UIImageView", Some(cv))
}

fn text_of(emu: &mut Emu, l: u64) -> Result<String> {
    if l == 0 {
        return Ok(String::new());
    }
    let t = send(emu, l, "text", &[])?;
    if t == 0 { Ok(String::new()) } else { nsstring(emu, t) }
}

/// A view's own size; a label's rounded up to the pixel, as UIKit rounds
/// the labels in cells (17-pt text 20.33 tall on a 3× screen), not to the point.
fn intrinsic(emu: &mut Emu, v: u64) -> Result<[f64; 2]> {
    let cls = host_class(emu, "UILabel")?;
    if send(emu, v, "isKindOfClass:", &[cls])? & 0xff != 0 {
        let t = crate::label::lay_out(emu, v, f64::INFINITY)?;
        let w = t.lines.iter().map(|l| l.width).fold(0.0, f64::max);
        let s = emu.device.scale.max(1.0);
        let up = |x: f64| (x * s - 1e-6).ceil() / s;
        return Ok([up(w), up(t.height())]);
    }
    let r = send_fp(emu, v, "intrinsicContentSize", &[], &[])?;
    Ok([r[0].max(0.0), r[1].max(0.0)])
}

/// Round to the pixel (as UIKit places a cell's parts).
fn pixel(emu: &Emu, x: f64) -> f64 {
    let s = emu.device.scale.max(1.0);
    (x * s).round() / s
}

/// The height a cell has of itself (its style and what it shows).
fn natural_height(emu: &mut Emu, c: u64) -> Result<f64> {
    if cell_style(emu, c) == 3 {
        let detail = objc::bag_get(emu, c, "pomme.cellPart.detail").unwrap_or(0);
        if !text_of(emu, detail)?.is_empty() {
            return Ok(SUBTITLE_ROW);
        }
    }
    Ok(ROW)
}

fn place(emu: &mut Emu, v: u64, frame: [f64; 4], width: f64, rtl: bool) -> Result<()> {
    let f = if rtl { [width - frame[0] - frame[2], frame[1], frame[2], frame[3]] } else { frame };
    objc::send_fp(emu, v, "setFrame:", &[], &f)?;
    Ok(())
}

/// `-[UITableViewCell layoutSubviews]`: its parts where iOS puts them for its style.
fn cell_layout(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    let b = view(emu, c).bounds;
    let (w, h) = (b[2], b[3]);
    let rtl = crate::direction::view_rtl(emu, c)?;
    let st = cell_style(emu, c);
    // The background: the app's colour if it gave the cell one, else white;
    // the selection's grey when selected.
    let bg = cell_part(emu, c, "background", "UIView", None)?;
    send(emu, c, "sendSubviewToBack:", &[bg])?;
    let selected = objc::bag_get(emu, c, "selected").unwrap_or(0) & 0xff != 0 || objc::bag_get(emu, c, "highlighted").unwrap_or(0) & 0xff != 0;
    let none_style = objc::bag_get(emu, c, "selectionStyle").unwrap_or(3) == 0;
    let own = view(emu, c).background != 0;
    let fill = if selected && !none_style { color(emu, SELECTED)? } else if own { color(emu, [0.0; 4])? } else { color(emu, [1.0; 4])? };
    send(emu, bg, "setBackgroundColor:", &[fill])?;
    objc::send_fp(emu, bg, "setFrame:", &[], &[0.0, 0.0, w, h])?;
    // Inset grouped: the section's first and last rows rounded.
    let corners = objc::bag_get(emu, c, "pomme.corners").unwrap_or(0);
    let layer = send(emu, bg, "layer", &[])?;
    objc::send_fp(emu, layer, "setCornerRadius:", &[], &[if corners != 0 { INSET_RADIUS } else { 0.0 }])?;
    send(emu, layer, "setMaskedCorners:", &[corners])?;
    // The accessory, at the trailing edge.
    let accessory_view = objc::bag_get(emu, c, "accessoryView").unwrap_or(0);
    let kind = objc::bag_get(emu, c, "accessoryType").unwrap_or(0);
    let acc = cell_part(emu, c, "accessory", "UIImageView", None)?;
    let mut aw = 0.0;
    if accessory_view != 0 {
        let s = view(emu, accessory_view).bounds;
        aw = s[2];
        if view(emu, accessory_view).superview != c {
            send(emu, c, "addSubview:", &[accessory_view])?;
        }
        place(emu, accessory_view, [w - MARGIN - s[2], (h - s[3]) / 2.0, s[2], s[3]], w, rtl)?;
        view_mut(emu, acc).hidden = true;
    } else if kind != 0 {
        // Disclosure (1), detail disclosure (2), checkmark (3), detail button (4).
        let (symbol, tint) = match kind {
            3 => ("checkmark", crate::appearance::system(emu, "systemBlueColor")),
            _ => ("chevron.forward", TERTIARY),
        };
        let img = crate::symbols::image(emu, symbol, Default::default())?;
        send(emu, acc, "setImage:", &[img])?;
        let tc = color(emu, tint)?;
        send(emu, acc, "setTintColor:", &[tc])?;
        objc::bag_set(emu, acc, "contentMode", 1);
        view_mut(emu, acc).content_mode = 1;
        aw = DISCLOSURE[0];
        place(emu, acc, [w - MARGIN - DISCLOSURE[0], (h - DISCLOSURE[1]) / 2.0, DISCLOSURE[0], DISCLOSURE[1]], w, rtl)?;
        view_mut(emu, acc).hidden = false;
    } else {
        view_mut(emu, acc).hidden = true;
    }
    let content_w = if aw > 0.0 { w - MARGIN - aw } else { w };
    let trailing = if aw > 0.0 { 8.0 } else { MARGIN };
    let cv = content_view(emu, c)?;
    place(emu, cv, [0.0, 0.0, content_w, h], w, rtl)?;
    // The image (not in value 2 cells).
    let iv = image_view(emu, c)?;
    let image = send(emu, iv, "image", &[])?;
    let mut text_x = MARGIN;
    if image != 0 && st != 2 {
        let s = intrinsic(emu, iv)?;
        let x = MARGIN + ((24.0 - s[0]) / 2.0).max(0.0);
        // (A symbol sits 2/3 pt above the middle: recorded at 52 and 62 pt.)
        let symbol = emu.render.images.get(&image).is_some_and(|i| i.look.symbol.is_some());
        let y = (h - s[1]) / 2.0 - if symbol { 2.0 / 3.0 } else { 0.0 };
        place(emu, iv, [x, y, s[0], s[1]], content_w, rtl)?;
        view_mut(emu, iv).hidden = false;
        text_x = x + s[0] + MARGIN;
    } else {
        view_mut(emu, iv).hidden = true;
    }
    let tl = text_label(emu, c)?;
    let dl = detail_label(emu, c)?;
    let ts = intrinsic(emu, tl)?;
    let room = content_w - text_x - trailing;
    match st {
        // Value 1: the title, then the detail against the trailing edge.
        1 => {
            let ds = intrinsic(emu, dl)?;
            let (ty, dy) = (pixel(emu, (h - ts[1]) / 2.0), pixel(emu, (h - ds[1]) / 2.0));
            place(emu, tl, [text_x, ty, ts[0].min(room), ts[1]], content_w, rtl)?;
            place(emu, dl, [content_w - trailing - ds[0], dy, ds[0], ds[1]], content_w, rtl)?;
        }
        // Value 2: a 91-pt title column (right-aligned), the detail 6 pt after it.
        2 => {
            let ds = intrinsic(emu, dl)?;
            let y = 19.0 + (h - ROW) / 2.0;
            place(emu, tl, [MARGIN, y, 91.0, ts[1]], content_w, rtl)?;
            place(emu, dl, [MARGIN + 97.0, y, ds[0].min(content_w - MARGIN - 97.0 - trailing), ds[1]], content_w, rtl)?;
        }
        // Subtitle: the title above the detail.
        3 => {
            let ds = intrinsic(emu, dl)?;
            if ds[1] > 0.0 && !text_of(emu, dl)?.is_empty() {
                let top = 9.0 + (h - SUBTITLE_ROW) / 2.0;
                place(emu, tl, [text_x, top, ts[0].min(room), ts[1]], content_w, rtl)?;
                place(emu, dl, [text_x, top + 23.33, ds[0].min(room), ds[1]], content_w, rtl)?;
            } else {
                place(emu, tl, [text_x, 0.0, room, h], content_w, rtl)?;
            }
        }
        _ => place(emu, tl, [text_x, 0.0, room, h], content_w, rtl)?,
    }
    // Separators: under the row from the text's leading edge (or across it),
    // and across its top for a grouped section's first row.
    let sep = objc::bag_get(emu, c, "pomme.separator").unwrap_or(1);
    let line = cell_part(emu, c, "separator", "UIView", None)?;
    let top_line = cell_part(emu, c, "topSeparator", "UIView", None)?;
    let sc = color(emu, SEPARATOR)?;
    send(emu, line, "setBackgroundColor:", &[sc])?;
    send(emu, top_line, "setBackgroundColor:", &[sc])?;
    let left = if st == 2 { MARGIN } else { text_x };
    match sep {
        0 => view_mut(emu, line).hidden = true,
        2 => {
            place(emu, line, [0.0, h - 1.0, w, 1.0], w, rtl)?;
            view_mut(emu, line).hidden = false;
        }
        _ => {
            place(emu, line, [left, h - 1.0, (w - left - MARGIN).max(0.0), 1.0], w, rtl)?;
            view_mut(emu, line).hidden = false;
        }
    }
    let top = objc::bag_get(emu, c, "pomme.topSeparator").unwrap_or(0) != 0;
    if top {
        objc::send_fp(emu, top_line, "setFrame:", &[], &[0.0, 0.0, w, 1.0])?;
    }
    view_mut(emu, top_line).hidden = !top;
    Ok(())
}

fn cell_init(emu: &mut Emu) -> Result<()> {
    let (c, st, reuse) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let c = objc::send_fp_id(emu, c, "initWithFrame:", &[], &[0.0, 0.0, 320.0, ROW])?;
    objc::bag_set(emu, c, "pomme.cellStyle", st);
    if reuse != 0 {
        let r = send(emu, reuse, "copy", &[])?;
        objc::bag_set(emu, c, "reuseIdentifier", r);
    }
    content_view(emu, c)?;
    ret(emu, c)
}

// ---- headers and footers ----

fn header_view(emu: &mut Emu, t: u64, s: usize, footer: bool) -> Result<u64> {
    if let Some(&v) = table(emu, t).headers.get(&(s, footer)) {
        return Ok(v);
    }
    let v = new_of(emu, "UITableViewHeaderFooterView", [0.0; 4])?;
    let l = new_of(emu, "UILabel", [0.0; 4])?;
    send(emu, v, "addSubview:", &[l])?;
    objc::bag_set(emu, v, "textLabel", l);
    let (size, weight) = if footer { (13.0, 0.0) } else { (17.0, 0.3) };
    label_font(emu, l, size, weight, SECONDARY)?;
    send(emu, t, "addSubview:", &[v])?;
    table(emu, t).headers.insert((s, footer), v);
    Ok(v)
}

/// Place a section's titled header or footer: its label where iOS puts it.
fn place_header(emu: &mut Emu, t: u64, s: usize, footer: bool, (y, h): (f64, f64), text: &str) -> Result<()> {
    let v = header_view(emu, t, s, footer)?;
    let w = view(emu, t).bounds[2];
    objc::send_fp(emu, v, "setFrame:", &[], &[0.0, y, w, h])?;
    let l = objc::bag_get(emu, v, "textLabel").unwrap_or(0);
    let txt = foundation::new_nsstring(emu, text)?;
    send(emu, l, "setText:", &[txt])?;
    let size = intrinsic(emu, l)?;
    let inset = if style(emu, t) == INSET_GROUPED { INSET } else { 0.0 };
    let plain = style(emu, t) == PLAIN;
    // (Measured: the label's top in the view, by its height above the rows' text.)
    let top = match (plain, footer) {
        (true, false) => 4.0,
        (true, true) => 6.0,
        (false, false) => h - 80.0 / 3.0,
        (false, true) => 23.0 / 3.0,
    };
    let rtl = crate::direction::view_rtl(emu, t)?;
    // (A plain table's header and footer labels are whole points tall: 21, 16.)
    let height = if plain { size[1].ceil() } else { size[1] };
    place(emu, l, [inset + MARGIN, top, size[0], height], w, rtl)?;
    view_mut(emu, v).hidden = false;
    Ok(())
}

// ---- laying out the rows on screen ----

/// Ask for the cell of a row.
fn make_cell(emu: &mut Emu, t: u64, s: usize, r: usize) -> Result<u64> {
    let ds = data_source(emu, t);
    if !responds(emu, ds, "tableView:cellForRowAtIndexPath:")? {
        return Ok(0);
    }
    let p = index_path(emu, s, r)?;
    send(emu, ds, "tableView:cellForRowAtIndexPath:", &[t, p])
}

fn reuse_id(emu: &mut Emu, c: u64) -> Result<String> {
    let r = objc::bag_get(emu, c, "reuseIdentifier").unwrap_or(0);
    if r == 0 { Ok(String::new()) } else { nsstring(emu, r) }
}

/// Put a cell that left the screen in the pool.
fn retire(emu: &mut Emu, t: u64, c: u64) -> Result<()> {
    send(emu, c, "retain", &[])?;
    send(emu, c, "removeFromSuperview", &[])?;
    let id = reuse_id(emu, c)?;
    table(emu, t).pool.entry(id).or_default().push(c);
    Ok(())
}

/// `-[UITableView layoutSubviews]`: the cells, headers and footers on screen.
fn layout(emu: &mut Emu, t: u64) -> Result<()> {
    default_background(emu, t)?;
    for _ in 0..3 {
        ensure(emu, t)?;
        let b = view(emu, t).bounds;
        let (top, bottom) = (b[1], b[1] + b[3]);
        let st = style(emu, t);
        let sections = table(emu, t).sections.clone();
        let mut wanted = Vec::new();
        for (s, sec) in sections.iter().enumerate() {
            for (r, &(y, h)) in sec.rows.iter().enumerate() {
                if y + h > top && y < bottom {
                    wanted.push((s, r));
                }
            }
        }
        // Rows that left: their cells to the pool.
        let gone: Vec<((usize, usize), u64)> = table(emu, t).visible.iter().filter(|(k, _)| !wanted.contains(k)).map(|(k, v)| (*k, *v)).collect();
        for (k, c) in gone {
            table(emu, t).visible.remove(&k);
            retire(emu, t, c)?;
        }
        let mut resized = false;
        for &(s, r) in &wanted {
            let c = match table(emu, t).visible.get(&(s, r)).copied() {
                Some(c) => c,
                None => {
                    let c = make_cell(emu, t, s, r)?;
                    if c == 0 {
                        continue;
                    }
                    send(emu, c, "retain", &[])?;
                    if view(emu, c).superview != t {
                        send(emu, t, "addSubview:", &[c])?;
                    }
                    table(emu, t).visible.insert((s, r), c);
                    let dl = delegate(emu, t);
                    if responds(emu, dl, "tableView:willDisplayCell:forRowAtIndexPath:")? {
                        let p = index_path(emu, s, r)?;
                        send(emu, dl, "tableView:willDisplayCell:forRowAtIndexPath:", &[t, c, p])?;
                    }
                    // A cell's own height, where the row takes it.
                    let nh = natural_height(emu, c)?;
                    let own = get_f64(emu, t, "rowHeight", -1.0) < 0.0 && !responds(emu, dl, "tableView:heightForRowAtIndexPath:")?;
                    if own && table(emu, t).natural.get(&(s, r)).copied().unwrap_or(ROW) != nh {
                        table(emu, t).natural.insert((s, r), nh);
                        resized = true;
                    }
                    c
                }
            };
            let last = r + 1 == sections[s].rows.len();
            let (sep, top_sep, corners) = match st {
                // (A plain section's last row loses its line to a footer.)
                PLAIN => (if last && sections[s].footer.1 > 0.0 { 0 } else { 1 }, 0, 0),
                INSET_GROUPED => (if last { 0 } else { 1 }, 0, (if r == 0 { 3 } else { 0 }) | (if last { 12 } else { 0 })),
                _ => (if last { 2 } else { 1 }, (r == 0) as u64, 0),
            };
            objc::bag_set(emu, c, "pomme.separator", sep);
            objc::bag_set(emu, c, "pomme.topSeparator", top_sep);
            objc::bag_set(emu, c, "pomme.corners", corners);
            let selected = table(emu, t).selected == Some((s, r));
            objc::bag_set(emu, c, "selected", selected as u64);
            let rect = row_rect(emu, t, s, r);
            objc::send_fp(emu, c, "setFrame:", &[], &rect)?;
            send(emu, c, "setNeedsLayout", &[])?;
        }
        if resized {
            table(emu, t).dirty = true;
            continue;
        }
        // Headers and footers with titles.
        let mut shown = Vec::new();
        for (s, sec) in sections.iter().enumerate() {
            for (footer, rect, text) in [(false, sec.header, &sec.header_title), (true, sec.footer, &sec.footer_title)] {
                if let Some(text) = text
                    && rect.0 + rect.1 > top
                    && rect.0 < bottom
                {
                    place_header(emu, t, s, footer, rect, text)?;
                    shown.push((s, footer));
                }
            }
        }
        let hidden: Vec<u64> = table(emu, t).headers.iter().filter(|(k, _)| !shown.contains(k)).map(|(_, v)| *v).collect();
        for v in hidden {
            view_mut(emu, v).hidden = true;
        }
        break;
    }
    Ok(())
}

/// The content scrolled: lay out the rows now on screen.
pub(crate) fn scrolled(emu: &mut Emu, v: u64) {
    if emu.tables.tables.contains_key(&v) {
        crate::uiview::view_mut(emu, v).needs_layout = true;
    }
}

fn reload(emu: &mut Emu, t: u64) -> Result<()> {
    let cells: Vec<u64> = table(emu, t).visible.values().copied().collect();
    table(emu, t).visible.clear();
    for c in cells {
        retire(emu, t, c)?;
    }
    let tb = table(emu, t);
    tb.dirty = true;
    tb.natural.clear();
    view_mut(emu, t).needs_layout = true;
    crate::host::frame_changed(emu);
    Ok(())
}

// ---- selection ----

fn select(emu: &mut Emu, t: u64, at: Option<(usize, usize)>) -> Result<()> {
    let old = table(emu, t).selected;
    table(emu, t).selected = at;
    for k in [old, at].into_iter().flatten() {
        if let Some(&c) = table(emu, t).visible.get(&k) {
            objc::bag_set(emu, c, "selected", (Some(k) == at) as u64);
            send(emu, c, "setNeedsLayout", &[])?;
        }
    }
    crate::host::frame_changed(emu);
    Ok(())
}

/// A tap ended on a cell: select its row, telling the delegate.
fn cell_touches_ended(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    let t = view(emu, c).superview;
    let Some(at) = table(emu, t).visible.iter().find(|(_, v)| **v == c).map(|(k, _)| *k) else { return Ok(()) };
    if objc::bag_get(emu, t, "allowsSelection").is_some_and(|a| a & 0xff == 0) {
        return Ok(());
    }
    let dl = delegate(emu, t);
    let mut p = index_path(emu, at.0, at.1)?;
    if responds(emu, dl, "tableView:willSelectRowAtIndexPath:")? {
        p = send(emu, dl, "tableView:willSelectRowAtIndexPath:", &[t, p])?;
    }
    let Some(at) = path_of(emu, p) else { return Ok(()) };
    if let Some(old) = table(emu, t).selected.filter(|&o| o != at) {
        select(emu, t, None)?;
        if responds(emu, dl, "tableView:didDeselectRowAtIndexPath:")? {
            let op = index_path(emu, old.0, old.1)?;
            send(emu, dl, "tableView:didDeselectRowAtIndexPath:", &[t, op])?;
        }
    }
    select(emu, t, Some(at))?;
    if responds(emu, dl, "tableView:didSelectRowAtIndexPath:")? {
        send(emu, dl, "tableView:didSelectRowAtIndexPath:", &[t, p])?;
    }
    Ok(())
}

// ---- methods ----

fn table_init(emu: &mut Emu, st: u64) -> Result<()> {
    let r = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    let t = objc::send_fp_id(emu, emu.arg(0), "initWithFrame:", &[], &r)?;
    objc::bag_set(emu, t, "pomme.tableStyle", st);
    table(emu, t);
    default_background(emu, t)?;
    ret(emu, t)
}

/// A table's background unless the app gave it one: white, or grouped grey.
fn default_background(emu: &mut Emu, t: u64) -> Result<()> {
    if objc::bag_get(emu, t, "pomme.tableBackground").is_some() {
        return Ok(());
    }
    objc::bag_set(emu, t, "pomme.tableBackground", 1);
    if view(emu, t).background == 0 {
        let bg = color(emu, if style(emu, t) == PLAIN { [1.0; 4] } else { GROUPED_BACKGROUND })?;
        send(emu, t, "setBackgroundColor:", &[bg])?;
    }
    Ok(())
}

fn dequeue(emu: &mut Emu, with_path: bool) -> Result<()> {
    let (t, id) = (emu.arg(0), emu.arg(2));
    let key = nsstring(emu, id)?;
    if let Some(c) = table(emu, t).pool.get_mut(&key).and_then(|v| v.pop()) {
        send(emu, c, "autorelease", &[])?;
        objc::bag_set(emu, c, "selected", 0);
        send(emu, c, "prepareForReuse", &[])?;
        return ret(emu, c);
    }
    let cls = table(emu, t).registered.get(&key).copied().unwrap_or(0);
    if cls == 0 {
        // (Unregistered: nil without an index path; with one, UIKit throws: a plain cell.)
        if !with_path {
            return ret(emu, 0);
        }
    }
    let cls = if cls == 0 { host_class(emu, "UITableViewCell")? } else { cls };
    let c = send(emu, cls, "alloc", &[])?;
    let c = send(emu, c, "initWithStyle:reuseIdentifier:", &[0, id])?;
    send(emu, c, "autorelease", &[])?;
    ret(emu, c)
}

const TV: &str = "UITableView";
const CELL: &str = "UITableViewCell";
const IP: &str = "NSIndexPath";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // NSIndexPath
    (IP, true, "indexPathForRow:inSection:", |e| {
        let p = index_path(e, e.arg(3) as usize, e.arg(2) as usize)?;
        ret(e, p)
    }),
    (IP, true, "indexPathForItem:inSection:", |e| {
        let p = index_path(e, e.arg(3) as usize, e.arg(2) as usize)?;
        ret(e, p)
    }),
    (IP, false, "row", |e| {
        let r = path_of(e, e.arg(0)).map_or(0, |p| p.1);
        ret(e, r as u64)
    }),
    (IP, false, "item", |e| {
        let r = path_of(e, e.arg(0)).map_or(0, |p| p.1);
        ret(e, r as u64)
    }),
    (IP, false, "section", |e| {
        let s = path_of(e, e.arg(0)).map_or(0, |p| p.0);
        ret(e, s as u64)
    }),
    (IP, false, "length", |e| ret(e, 2)),
    (IP, false, "indexAtPosition:", |e| {
        let p = path_of(e, e.arg(0)).unwrap_or((0, 0));
        let v = if e.arg(2) == 0 { p.0 } else { p.1 };
        ret(e, v as u64)
    }),
    (IP, false, "isEqual:", |e| {
        let (a, b) = (path_of(e, e.arg(0)), path_of(e, e.arg(2)));
        ret(e, (a.is_some() && a == b) as u64)
    }),
    (IP, false, "hash", |e| {
        let p = path_of(e, e.arg(0)).unwrap_or((0, 0));
        ret(e, (p.0 as u64) << 32 | p.1 as u64)
    }),
    (IP, false, "compare:", |e| {
        let (a, b) = (path_of(e, e.arg(0)), path_of(e, e.arg(2)));
        let o = match a.cmp(&b) {
            std::cmp::Ordering::Less => -1i64,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };
        ret(e, o as u64)
    }),
    (IP, false, "copyWithZone:", |e| {
        let p = e.arg(0);
        send(e, p, "retain", &[])?;
        ret(e, p)
    }),
    (IP, false, "description", |e| {
        let p = path_of(e, e.arg(0)).unwrap_or((0, 0));
        let s = foundation::new_nsstring(e, &format!("<NSIndexPath> {{length = 2, path = {} - {}}}", p.0, p.1))?;
        ret(e, s)
    }),
    // UITableView
    (TV, false, "initWithFrame:style:", |e| {
        let st = e.arg(2);
        table_init(e, st)
    }),
    (TV, false, "style", |e| {
        let s = style(e, e.arg(0));
        ret(e, s)
    }),
    (TV, false, "dataSource", |e| {
        let d = data_source(e, e.arg(0));
        ret(e, d)
    }),
    (TV, false, "setDataSource:", |e| {
        let (t, d) = (e.arg(0), e.arg(2));
        objc::bag_set(e, t, "dataSource", d);
        table(e, t).dirty = true;
        view_mut(e, t).needs_layout = true;
        Ok(())
    }),
    (TV, false, "setDelegate:", |e| {
        let (t, d) = (e.arg(0), e.arg(2));
        objc::bag_set(e, t, "delegate", d);
        table(e, t).dirty = true;
        view_mut(e, t).needs_layout = true;
        Ok(())
    }),
    (TV, false, "registerClass:forCellReuseIdentifier:", |e| {
        let (t, cls, id) = (e.arg(0), e.arg(2), e.arg(3));
        let key = nsstring(e, id)?;
        table(e, t).registered.insert(key, cls);
        Ok(())
    }),
    (TV, false, "registerClass:forHeaderFooterViewReuseIdentifier:", |_| Ok(())),
    (TV, false, "dequeueReusableCellWithIdentifier:", |e| dequeue(e, false)),
    (TV, false, "dequeueReusableCellWithIdentifier:forIndexPath:", |e| dequeue(e, true)),
    (TV, false, "reloadData", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "reloadSections:withRowAnimation:", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "reloadRowsAtIndexPaths:withRowAnimation:", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "insertRowsAtIndexPaths:withRowAnimation:", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "deleteRowsAtIndexPaths:withRowAnimation:", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "insertSections:withRowAnimation:", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "deleteSections:withRowAnimation:", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "beginUpdates", |_| Ok(())),
    (TV, false, "endUpdates", |e| {
        let t = e.arg(0);
        reload(e, t)
    }),
    (TV, false, "layoutSubviews", |e| {
        let t = e.arg(0);
        layout(e, t)
    }),
    (TV, false, "numberOfSections", |e| {
        let t = e.arg(0);
        ensure(e, t)?;
        let n = table(e, t).sections.len();
        ret(e, n as u64)
    }),
    (TV, false, "numberOfRowsInSection:", |e| {
        let (t, s) = (e.arg(0), e.arg(2) as usize);
        ensure(e, t)?;
        let n = table(e, t).sections.get(s).map_or(0, |sec| sec.rows.len());
        ret(e, n as u64)
    }),
    (TV, false, "rectForRowAtIndexPath:", |e| {
        let t = e.arg(0);
        ensure(e, t)?;
        let (s, r) = path_of(e, e.arg(2)).unwrap_or((usize::MAX, 0));
        let rect = row_rect(e, t, s, r);
        ret_rect(e, rect);
        Ok(())
    }),
    (TV, false, "rectForSection:", |e| {
        let (t, s) = (e.arg(0), e.arg(2) as usize);
        ensure(e, t)?;
        let w = view(e, t).bounds[2];
        let r = table(e, t).sections.get(s).map_or([0.0; 4], |sec| [0.0, sec.top, w, sec.bottom - sec.top]);
        ret_rect(e, r);
        Ok(())
    }),
    (TV, false, "rectForHeaderInSection:", |e| {
        let (t, s) = (e.arg(0), e.arg(2) as usize);
        ensure(e, t)?;
        let w = view(e, t).bounds[2];
        let r = table(e, t).sections.get(s).map_or([0.0; 4], |sec| [0.0, sec.header.0, w, sec.header.1]);
        ret_rect(e, r);
        Ok(())
    }),
    (TV, false, "rectForFooterInSection:", |e| {
        let (t, s) = (e.arg(0), e.arg(2) as usize);
        ensure(e, t)?;
        let w = view(e, t).bounds[2];
        let r = table(e, t).sections.get(s).map_or([0.0; 4], |sec| [0.0, sec.footer.0, w, sec.footer.1]);
        ret_rect(e, r);
        Ok(())
    }),
    (TV, false, "headerViewForSection:", |e| {
        let (t, s) = (e.arg(0), e.arg(2) as usize);
        let v = table(e, t).headers.get(&(s, false)).copied().unwrap_or(0);
        ret(e, v)
    }),
    (TV, false, "footerViewForSection:", |e| {
        let (t, s) = (e.arg(0), e.arg(2) as usize);
        let v = table(e, t).headers.get(&(s, true)).copied().unwrap_or(0);
        ret(e, v)
    }),
    (TV, false, "visibleCells", |e| {
        let t = e.arg(0);
        let cells: Vec<u64> = table(e, t).visible.values().copied().collect();
        let a = foundation::new_array_of(e, &cells)?;
        ret(e, a)
    }),
    (TV, false, "indexPathsForVisibleRows", |e| {
        let t = e.arg(0);
        let keys: Vec<(usize, usize)> = table(e, t).visible.keys().copied().collect();
        let mut ps = Vec::new();
        for (s, r) in keys {
            ps.push(index_path(e, s, r)?);
        }
        let a = foundation::new_array_of(e, &ps)?;
        ret(e, a)
    }),
    (TV, false, "cellForRowAtIndexPath:", |e| {
        let t = e.arg(0);
        let c = path_of(e, e.arg(2)).and_then(|k| table(e, t).visible.get(&k).copied()).unwrap_or(0);
        ret(e, c)
    }),
    (TV, false, "indexPathForCell:", |e| {
        let (t, c) = (e.arg(0), e.arg(2));
        match table(e, t).visible.iter().find(|(_, v)| **v == c).map(|(k, _)| *k) {
            Some((s, r)) => {
                let p = index_path(e, s, r)?;
                ret(e, p)
            }
            None => ret(e, 0),
        }
    }),
    (TV, false, "indexPathForRowAtPoint:", |e| {
        let t = e.arg(0);
        let (x, y) = (d(e, 0), d(e, 1));
        ensure(e, t)?;
        let w = view(e, t).bounds[2];
        let hit = table(e, t).sections.iter().enumerate().find_map(|(s, sec)| sec.rows.iter().position(|&(ry, rh)| y >= ry && y < ry + rh).map(|r| (s, r)));
        match hit.filter(|_| (0.0..w).contains(&x)) {
            Some((s, r)) => {
                let p = index_path(e, s, r)?;
                ret(e, p)
            }
            None => ret(e, 0),
        }
    }),
    (TV, false, "scrollToRowAtIndexPath:atScrollPosition:animated:", |e| {
        let (t, p, pos) = (e.arg(0), e.arg(2), e.arg(3));
        ensure(e, t)?;
        let Some((s, r)) = path_of(e, p) else { return Ok(()) };
        let rect = row_rect(e, t, s, r);
        let b = view(e, t).bounds;
        let content = table(e, t).content_height;
        // UITableViewScrollPosition: none 0, top 1, middle 2, bottom 3.
        let y = match pos {
            2 => rect[1] + rect[3] / 2.0 - b[3] / 2.0,
            3 => rect[1] + rect[3] - b[3],
            0 if rect[1] >= b[1] && rect[1] + rect[3] <= b[1] + b[3] => b[1],
            0 if rect[1] + rect[3] > b[1] + b[3] => rect[1] + rect[3] - b[3],
            _ => rect[1],
        };
        let y = y.clamp(0.0, (content - b[3]).max(0.0));
        crate::scrollview::set_offset(e, t, [b[0], y])?;
        view_mut(e, t).needs_layout = true;
        Ok(())
    }),
    (TV, false, "selectRowAtIndexPath:animated:scrollPosition:", |e| {
        let t = e.arg(0);
        let at = path_of(e, e.arg(2));
        select(e, t, at)
    }),
    (TV, false, "deselectRowAtIndexPath:animated:", |e| {
        let t = e.arg(0);
        let at = path_of(e, e.arg(2));
        if table(e, t).selected == at {
            select(e, t, None)?;
        }
        Ok(())
    }),
    (TV, false, "indexPathForSelectedRow", |e| {
        let t = e.arg(0);
        match table(e, t).selected {
            Some((s, r)) => {
                let p = index_path(e, s, r)?;
                ret(e, p)
            }
            None => ret(e, 0),
        }
    }),
    (TV, false, "rowHeight", |e| {
        let h = get_f64(e, e.arg(0), "rowHeight", -1.0);
        ret_f64(e, h)
    }),
    (TV, false, "setRowHeight:", |e| {
        let (t, h) = (e.arg(0), d(e, 0));
        set_f64(e, t, "rowHeight", h);
        table(e, t).dirty = true;
        view_mut(e, t).needs_layout = true;
        Ok(())
    }),
    (TV, false, "estimatedRowHeight", |e| {
        let h = get_f64(e, e.arg(0), "estimatedRowHeight", -1.0);
        ret_f64(e, h)
    }),
    (TV, false, "setEstimatedRowHeight:", |e| {
        let (t, h) = (e.arg(0), d(e, 0));
        set_f64(e, t, "estimatedRowHeight", h);
        Ok(())
    }),
    (TV, false, "sectionHeaderHeight", |e| {
        let h = get_f64(e, e.arg(0), "sectionHeaderHeight", -1.0);
        ret_f64(e, h)
    }),
    (TV, false, "sectionFooterHeight", |e| {
        let h = get_f64(e, e.arg(0), "sectionFooterHeight", -1.0);
        ret_f64(e, h)
    }),
    (TV, false, "separatorStyle", |e| {
        let s = objc::bag_get(e, e.arg(0), "separatorStyle").unwrap_or(1);
        ret(e, s)
    }),
    (TV, false, "separatorInset", |e| {
        ret_rect(e, [0.0, MARGIN, 0.0, 0.0]);
        Ok(())
    }),
    (TV, false, "separatorColor", |e| {
        let c = color(e, SEPARATOR)?;
        ret(e, c)
    }),
    (TV, false, "setTableHeaderView:", |e| {
        let (t, v) = (e.arg(0), e.arg(2));
        let old = objc::bag_get(e, t, "tableHeaderView").unwrap_or(0);
        if old != 0 {
            send(e, old, "removeFromSuperview", &[])?;
        }
        objc::bag_set(e, t, "tableHeaderView", v);
        if v != 0 {
            send(e, t, "addSubview:", &[v])?;
        }
        table(e, t).dirty = true;
        view_mut(e, t).needs_layout = true;
        Ok(())
    }),
    (TV, false, "setTableFooterView:", |e| {
        let (t, v) = (e.arg(0), e.arg(2));
        let old = objc::bag_get(e, t, "tableFooterView").unwrap_or(0);
        if old != 0 {
            send(e, old, "removeFromSuperview", &[])?;
        }
        objc::bag_set(e, t, "tableFooterView", v);
        if v != 0 {
            send(e, t, "addSubview:", &[v])?;
        }
        table(e, t).dirty = true;
        view_mut(e, t).needs_layout = true;
        Ok(())
    }),
    // UITableViewCell
    (CELL, false, "initWithStyle:reuseIdentifier:", cell_init),
    (CELL, false, "contentView", |e| {
        let c = e.arg(0);
        let v = content_view(e, c)?;
        ret(e, v)
    }),
    (CELL, false, "textLabel", |e| {
        let c = e.arg(0);
        let v = text_label(e, c)?;
        ret(e, v)
    }),
    (CELL, false, "detailTextLabel", |e| {
        let c = e.arg(0);
        let v = detail_label(e, c)?;
        ret(e, v)
    }),
    (CELL, false, "imageView", |e| {
        let c = e.arg(0);
        let v = image_view(e, c)?;
        ret(e, v)
    }),
    (CELL, false, "reuseIdentifier", |e| {
        let r = objc::bag_get(e, e.arg(0), "reuseIdentifier").unwrap_or(0);
        ret(e, r)
    }),
    (CELL, false, "accessoryType", |e| {
        let a = objc::bag_get(e, e.arg(0), "accessoryType").unwrap_or(0);
        ret(e, a)
    }),
    (CELL, false, "setAccessoryType:", |e| {
        let (c, a) = (e.arg(0), e.arg(2));
        objc::bag_set(e, c, "accessoryType", a);
        view_mut(e, c).needs_layout = true;
        Ok(())
    }),
    (CELL, false, "setAccessoryView:", |e| {
        let (c, v) = (e.arg(0), e.arg(2));
        let old = objc::bag_get(e, c, "accessoryView").unwrap_or(0);
        if old != 0 && old != v {
            send(e, old, "removeFromSuperview", &[])?;
        }
        if v != 0 {
            send(e, v, "retain", &[])?;
        }
        objc::bag_set(e, c, "accessoryView", v);
        view_mut(e, c).needs_layout = true;
        Ok(())
    }),
    (CELL, false, "isSelected", |e| {
        let s = objc::bag_get(e, e.arg(0), "selected").unwrap_or(0) & 0xff;
        ret(e, s)
    }),
    (CELL, false, "setSelected:", |e| {
        let (c, s) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, c, "selected", s);
        view_mut(e, c).needs_layout = true;
        crate::host::frame_changed(e);
        Ok(())
    }),
    (CELL, false, "setSelected:animated:", |e| {
        let (c, s) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, c, "selected", s);
        view_mut(e, c).needs_layout = true;
        crate::host::frame_changed(e);
        Ok(())
    }),
    (CELL, false, "isHighlighted", |e| {
        let s = objc::bag_get(e, e.arg(0), "highlighted").unwrap_or(0) & 0xff;
        ret(e, s)
    }),
    (CELL, false, "setHighlighted:animated:", |e| {
        let (c, s) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, c, "highlighted", s);
        view_mut(e, c).needs_layout = true;
        Ok(())
    }),
    (CELL, false, "prepareForReuse", |_| Ok(())),
    (CELL, false, "layoutSubviews", cell_layout),
    (CELL, false, "touchesEnded:withEvent:", cell_touches_ended),
    // UITableViewHeaderFooterView
    ("UITableViewHeaderFooterView", false, "textLabel", |e| {
        let l = objc::bag_get(e, e.arg(0), "textLabel").unwrap_or(0);
        ret(e, l)
    }),
    // UITableViewController: a table as its view, itself its data source and delegate.
    ("UITableViewController", false, "initWithStyle:", |e| {
        let (vc, st) = (e.arg(0), e.arg(2));
        objc::bag_set(e, vc, "pomme.tableStyle", st);
        let vc = send(e, vc, "initWithNibName:bundle:", &[0, 0])?;
        ret(e, vc)
    }),
    ("UITableViewController", false, "loadView", |e| {
        let vc = e.arg(0);
        let st = objc::bag_get(e, vc, "pomme.tableStyle").unwrap_or(PLAIN);
        let cls = host_class(e, TV)?;
        let t = send(e, cls, "alloc", &[])?;
        let screen = crate::orientation::screen(e);
        for (i, v) in [0.0f64, 0.0, screen.0, screen.1].iter().enumerate() {
            e.cpu.q[i] = v.to_bits() as u128;
        }
        let t = send(e, t, "initWithFrame:style:", &[st])?;
        send(e, t, "setDataSource:", &[vc])?;
        send(e, t, "setDelegate:", &[vc])?;
        send(e, vc, "setView:", &[t])?;
        Ok(())
    }),
    ("UITableViewController", false, "tableView", |e| {
        let v = send(e, e.arg(0), "view", &[])?;
        ret(e, v)
    }),
];
