//! The original's windows (VB3 forms) read from MORDOR.EXE: every control's type,
//! position, caption, colours, font and picture. The importer writes them to
//! FORMS.JSON, with the pictures beside it, and the game draws its windows from them.
//!
//! A form's stream is RC_DATA <id> (magic FF CC 2C): the form record, then tagged
//! control records <len:4 (bit 31 = control array)><id:1>[<index:2>]<name><type>
//! <properties>. Tags: 1 opens a level, 2 closes one, 3 is a sibling, 4 ends. The
//! property sizes per control class are in vb3props.txt (from VBRUN300 and the VBXs).
//! Format after vb3dec's pkg/frm (MIT, Jamie McPeek), as ported in ghidra-vb3.

use std::{collections::BTreeMap, io, path::Path};

use serde::{Deserialize, Serialize};

use crate::vfs;

pub const FILE: &str = "FORMS.JSON";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Val {
    N(i64),
    F(f64),
    S(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Control {
    pub name: String,
    pub index: Option<u16>,
    pub kind: String,
    /// The containing control's position in `Form::controls`; None for the form.
    pub parent: Option<usize>,
    /// Left, top, width, height in pixels, in the parent's client area.
    pub rect: [i32; 4],
    pub props: BTreeMap<String, Val>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Form {
    pub name: String,
    /// The window's outer rectangle in pixels.
    pub rect: [i32; 4],
    pub props: BTreeMap<String, Val>,
    pub controls: Vec<Control>,
}

/// The imported forms by name.
pub fn load(dir: &Path) -> Option<BTreeMap<String, Form>> {
    serde_json::from_slice(&vfs::read(dir.join(FILE)).ok()?).ok()
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// RC_DATA resource `id` of a Windows 3 (NE) program.
fn rcdata(exe: &[u8], id: u16) -> Option<&[u8]> {
    let ne = u32_at(exe, 0x3c)? as usize;
    let rt = ne + usize::from(u16_at(exe, ne + 0x24)?);
    let (shift, mut p) = (u16_at(exe, rt)?, rt + 2);
    while u16_at(exe, p)? != 0 {
        let (kind, count) = (u16_at(exe, p)?, usize::from(u16_at(exe, p + 2)?));
        for e in (0..count).map(|j| p + 8 + 12 * j) {
            if kind == 0x800A && u16_at(exe, e + 6)? == 0x8000 | id {
                let (o, n) = (usize::from(u16_at(exe, e)?) << shift, usize::from(u16_at(exe, e + 2)?) << shift);
                return exe.get(o..o + n);
            }
        }
        p += 8 + 12 * count;
    }
    None
}

fn latin1(b: &[u8]) -> String {
    b.iter().take_while(|&&c| c != 0).map(|&c| char::from(c)).collect()
}

/// Pascal strings until an empty one.
fn names(b: &[u8]) -> Vec<String> {
    let (mut out, mut p) = (Vec::new(), 0);
    while let Some(&n) = b.get(p).filter(|&&n| n != 0) {
        let Some(s) = b.get(p + 1..p + 1 + usize::from(n)) else { break };
        out.push(latin1(s));
        p += 1 + usize::from(n);
    }
    out
}

/// The forms' stream ids, from the project directory (RC_DATA 1).
fn project(exe: &[u8]) -> Vec<u16> {
    let Some(rc) = rcdata(exe, 1) else { return Vec::new() };
    let (mut ids, Some(t), Some(h)) = (Vec::new(), u16_at(rc, 15), u16_at(rc, 17)) else { return Vec::new() };
    let mut p = 19 + usize::from(t) + usize::from(h);
    while let (Some(&kind), Some(&n), Some(res)) = (rc.get(p), rc.get(p + 1), u16_at(rc, p + 2)) {
        if !matches!(kind, 0x43 | 0x46 | 0x58) {
            break;
        }
        p += 11;
        let n = if n == 0xFF { rc.get(p).map_or(0, |&n| { p += 1; n }) } else { n };
        p += usize::from(n);
        if kind != 0x43 {
            p += 2 + 4 * usize::try_from(u16_at(rc, p).unwrap_or(0) as i16).unwrap_or(0);
        }
        if kind == 0x46 {
            ids.push(res & 0x7FFF);
        }
    }
    ids
}

/// Each control class's properties in id order, as (name, size code).
fn classes() -> BTreeMap<&'static str, Vec<(&'static str, u8)>> {
    include_str!("vb3props.txt")
        .lines()
        .filter_map(|l| {
            let (cls, rest) = l.split_once(' ')?;
            Some((cls, rest.split(' ').map(|p| p.rsplit_once(':').map_or((p, b'?'), |(n, c)| (n, c.as_bytes()[0]))).collect()))
        })
        .collect()
}

/// Decodes a property block into `props` and `rect`; pictures go to `pics`.
fn props(b: &[u8], class: &[(&str, u8)], form: bool, rect: &mut [i32; 4], props: &mut BTreeMap<String, Val>, pics: &mut Vec<Vec<u8>>) {
    let (mut p, mut sep) = (0, false);
    while let Some(&id) = b.get(p) {
        p += 1;
        if id == 0xFF {
            if sep {
                return;
            }
            sep = true;
            continue;
        }
        let Some(&(name, code)) = class.get(usize::from(id)) else { return };
        let i16_at = |o: usize| u16_at(b, o).map(|v| v as i16);
        let n = match code {
            b'L' if form => {
                for (k, r) in rect.iter_mut().enumerate() {
                    *r = (u32_at(b, p + 4 * k).unwrap_or(0) as i32 + 7) / 15;
                }
                16
            }
            b'L' => {
                for (k, r) in rect.iter_mut().enumerate() {
                    *r = (i32::from(i16_at(p + 2 * k).unwrap_or(0)) + 7).div_euclid(15);
                }
                8
            }
            b'F' => {
                let Some(&k) = b.get(p) else { return };
                let k = usize::from(k);
                props.insert("FontName".into(), Val::S(latin1(b.get(p + 1..p + 1 + k).unwrap_or_default())));
                let size = u32_at(b, p + 1 + k).map_or(8.25, |v| f64::from(f32::from_bits(v)));
                props.insert("FontSize".into(), Val::F(size));
                props.insert("FontFlags".into(), Val::N(i64::from(*b.get(p + 5 + k).unwrap_or(&0))));
                k + 6
            }
            b'h' => {
                let Some(&k) = b.get(p) else { return };
                props.insert(name.into(), Val::S(latin1(b.get(p + 1..p + 1 + usize::from(k)).unwrap_or_default())));
                1 + usize::from(k)
            }
            b's' => {
                props.insert(name.into(), Val::N(i16_at(p).unwrap_or(0).into()));
                2
            }
            b'l' | b'S' => {
                props.insert(name.into(), Val::N(u32_at(b, p).unwrap_or(0) as i32 as i64));
                4
            }
            b'b' => {
                props.insert(name.into(), Val::N((*b.get(p).unwrap_or(&0)).into()));
                1
            }
            b'p' => {
                let Some(len) = u32_at(b, p) else { return };
                if len == u32::MAX || len == 0 {
                    4
                } else {
                    let data = b.get(p + 4..p + 4 + len as usize).unwrap_or_default();
                    if data.starts_with(b"BM") {
                        props.insert(name.into(), Val::N(pics.len() as i64));
                        pics.push(data.to_vec());
                    }
                    4 + len as usize
                }
            }
            _ => return,
        };
        p += n;
    }
}

const BUILTIN: [&str; 25] = [
    "PictureBox", "Label", "TextBox", "Frame", "CommandButton", "CheckBox", "OptionButton", "ComboBox", "ListBox", "HScrollBar", "VScrollBar", "Timer", "",
    "Form", "", "", "DriveListBox", "DirListBox", "", "Menu", "MDIForm", "", "Shape", "Line", "Image",
];

/// True when a control record (length, id, name, type) starts at `p`.
fn record_at(s: &[u8], p: usize, known_type: bool) -> bool {
    let Some(raw) = u32_at(s, p) else { return false };
    let len = (raw & 0x7FFF_FFFF) as usize;
    if len < 7 || p + len > s.len() {
        return false;
    }
    let at = p + 5 + if raw & 0x8000_0000 != 0 { 2 } else { 0 };
    let ty = at + 1 + usize::from(*s.get(at).unwrap_or(&0));
    ty < p + len && (!known_type || s[ty] == 0xFF || BUILTIN.get(usize::from(s[ty])).is_some_and(|n| !n.is_empty()))
}

fn form(exe: &[u8], id: u16, classes: &BTreeMap<&str, Vec<(&str, u8)>>, pics: &mut Vec<Vec<u8>>) -> Option<Form> {
    let s = rcdata(exe, id)?;
    let names = names(rcdata(exe, id + 1).unwrap_or_default());
    if !s.starts_with(b"\xff\xcc\x2c") {
        return None;
    }
    let end = 9 + (u32_at(s, 9)? & 0x7FFF_FFFF) as usize;
    let mut f = Form { name: names.first().cloned().unwrap_or_default(), ..Default::default() };
    let p = 14 + 1 + usize::from(*s.get(14)?) + 1;
    props(s.get(p..end)?, classes.get("Form")?, true, &mut f.rect, &mut f.props, pics);
    let (mut p, mut stack, mut last) = (end, Vec::new(), None);
    while let Some(&tag) = s.get(p) {
        p += 1;
        if !matches!(tag, 1 | 2 | 3 | 5) {
            break;
        }
        // A tag followed by another tag (or the end) is structural; else a record follows.
        let next = s.get(p).copied();
        let structural = next == Some(4) || (matches!(next, Some(1 | 2 | 3 | 5)) && (record_at(s, p + 1, false) || matches!(s.get(p + 1), Some(1..=5))));
        match tag {
            1 => stack.push(last),
            2 if structural => {
                stack.pop();
            }
            _ => {}
        }
        if structural {
            continue;
        }
        if !record_at(s, p, !matches!(tag, 1 | 3)) {
            break;
        }
        let raw = u32_at(s, p)?;
        let rec_end = p + (raw & 0x7FFF_FFFF) as usize;
        let cid = usize::from(s[p + 4]);
        let mut c = p + 5;
        let index = (raw & 0x8000_0000 != 0).then(|| {
            c += 2;
            u16_at(s, c - 2).unwrap_or(0)
        });
        let inline = latin1(s.get(c + 1..c + 1 + usize::from(s[c]))?);
        c += 1 + usize::from(s[c]);
        let kind = if s[c] == 0xFF {
            c += 1;
            let k = latin1(s.get(c + 1..c + 1 + usize::from(s[c]))?);
            c += usize::from(s[c]);
            k.to_ascii_lowercase()
        } else {
            BUILTIN.get(usize::from(s[c])).unwrap_or(&"").to_string()
        };
        c += 1;
        let mut ctl = Control { name: names.get(cid).filter(|n| !n.is_empty()).cloned().unwrap_or(inline), index, parent: stack.last().copied().flatten(), ..Default::default() };
        if let Some(class) = classes.get(kind.as_str()) {
            props(s.get(c..rec_end)?, class, false, &mut ctl.rect, &mut ctl.props, pics);
        }
        ctl.kind = kind;
        last = Some(f.controls.len());
        f.controls.push(ctl);
        p = rec_end;
    }
    Some(f)
}

/// Writes FORMS.JSON and each form's pictures (`<form>_<n>.BMP`, numbered by the
/// picture's `Picture` property value).
pub(crate) fn extract(exe: &[u8], out: &Path, got: &mut Vec<String>) -> io::Result<()> {
    let classes = classes();
    let mut all = BTreeMap::new();
    for id in project(exe) {
        let mut pics = Vec::new();
        let Some(f) = form(exe, id, &classes, &mut pics) else { continue };
        for (n, pic) in pics.iter().enumerate() {
            let file = format!("{}_{n}.BMP", f.name);
            vfs::write(out.join(&file), pic)?;
            got.push(file);
        }
        all.insert(f.name.clone(), f);
    }
    if all.is_empty() {
        return Ok(());
    }
    vfs::write(out.join(FILE), serde_json::to_vec(&all).map_err(io::Error::other)?)?;
    got.push(FILE.into());
    Ok(())
}
