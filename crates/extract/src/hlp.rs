//! The original's on-line HelpLesson, MORDOR.HLP: a Windows 3.1 help file. Read here are
//! its topics (paragraphs, fonts, tables, jumps and pictures), windows, keywords and
//! context numbers, for the remake's WinHelp window. The layout follows the format as
//! documented by the helpdeco project (M. Winterhoff, "helpfile.txt").

use std::collections::HashMap;

/// What a paragraph holds, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Text(String),
    Font(u16),
    /// A line break.
    Break,
    /// A paragraph's end.
    Par,
    Tab,
    /// A picture in the line (0), at the left (1) or the right (2) margin.
    Pic(Pic, u8),
    /// The start of a jump's hot text: to a topic, in a window (0, the main window).
    Link(usize, usize),
    EndLink,
}

/// A picture: an internal bitmap `|bmN`, or the BMP file an embedded window showed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Pic {
    Bm(u16),
    File(String),
}

/// A paragraph's format; lengths in help units (half points).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fmt {
    pub above: i32,
    pub below: i32,
    pub left: i32,
    pub right: i32,
    pub first: i32,
    /// Left (0), right (1) or centred (2).
    pub align: u8,
    pub tabs: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub fmt: Fmt,
    pub items: Vec<Item>,
    /// In a table: the row's number in the topic, the cell's left edge and width.
    pub cell: Option<(usize, i32, i32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Topic {
    pub title: String,
    /// The non-scrolling region, then the rest.
    pub head: Vec<Block>,
    pub body: Vec<Block>,
    /// The browse sequence (<< and >>).
    pub prev: Option<usize>,
    pub next: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Font {
    pub face: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub under: bool,
    pub color: Option<[u8; 3]>,
}

/// A window the help names: the main one first, then the secondary ones.
#[derive(Clone, Debug)]
pub struct Window {
    pub name: String,
    pub caption: String,
    /// Left, top, width and height in 1024ths of the screen.
    pub rect: [i32; 4],
    /// The non-scrolling region's colour.
    pub head: Option<[u8; 3]>,
    pub back: Option<[u8; 3]>,
}

#[derive(Clone, Debug, Default)]
pub struct Help {
    pub title: String,
    pub contents: usize,
    pub windows: Vec<Window>,
    pub fonts: Vec<Font>,
    pub topics: Vec<Topic>,
    /// The Search list: each keyword and its topics.
    pub keywords: Vec<(String, Vec<usize>)>,
    /// Context numbers (the windows' HelpContextID) to topics.
    pub map: HashMap<i32, usize>,
    /// The pictures' raw files, by internal name.
    files: HashMap<String, Vec<u8>>,
}

struct R<'a> {
    d: &'a [u8],
    i: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.d.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    /// A compressed unsigned short: one byte if its low bit is clear, else two.
    fn cu16(&mut self) -> Option<u32> {
        Some(if self.d.get(self.i)? & 1 == 0 { u32::from(self.u8()?) >> 1 } else { u32::from(self.u16()?) >> 1 })
    }
    fn cs16(&mut self) -> Option<i32> {
        Some(if self.d.get(self.i)? & 1 == 0 { i32::from(self.u8()?) / 2 - 0x40 } else { i32::from(self.u16()?) / 2 - 0x4000 })
    }
    /// A compressed long: two bytes if the low bit is clear, else four.
    fn cu32(&mut self) -> Option<u32> {
        Some(if self.d.get(self.i)? & 1 == 0 { u32::from(self.u16()?) >> 1 } else { self.u32()? >> 1 })
    }
    fn cs32(&mut self) -> Option<i64> {
        Some(if self.d.get(self.i)? & 1 == 0 { i64::from(self.u16()? >> 1) - 0x4000 } else { i64::from(self.u32()? >> 1) - 0x4000_0000 })
    }
    fn sz(&mut self) -> Option<String> {
        let n = self.d.get(self.i..)?.iter().position(|&c| c == 0)?;
        let s = ansi(self.take(n)?);
        self.i += 1;
        Some(s)
    }
}

/// Windows-1252's punctuation, where Latin-1 has control codes.
pub const CP1252: [(u8, char); 9] = [(0x85, '…'), (0x91, '‘'), (0x92, '’'), (0x93, '“'), (0x94, '”'), (0x95, '•'), (0x96, '–'), (0x97, '—'), (0x99, '™')];

/// Windows ANSI text.
pub(crate) fn ansi(b: &[u8]) -> String {
    b.iter().map(|&c| CP1252.iter().find(|p| p.0 == c).map_or(char::from(c), |p| p.1)).collect()
}

/// An internal file's data.
fn file(b: &[u8], at: usize) -> Option<&[u8]> {
    let used = u32::from_le_bytes(b.get(at + 4..at + 8)?.try_into().ok()?) as usize;
    b.get(at + 9..at + 9 + used)
}

/// Calls `f` on each leaf entry of a B+ tree, in key order.
fn leaves(d: &[u8], mut f: impl FnMut(&mut R) -> Option<()>) -> Option<()> {
    let mut r = R { d, i: 4 };
    let size = usize::from(r.u16()?);
    r.i = 26;
    let mut page = r.u16()? as i16;
    r.i = 32;
    let levels = r.u16()?;
    for _ in 1..levels {
        r.i = 38 + usize::try_from(page).ok()? * size + 4;
        page = r.u16()? as i16;
    }
    while page >= 0 {
        r.i = 38 + page as usize * size + 2;
        let n = r.u16()?;
        r.i += 2;
        page = r.u16()? as i16;
        for _ in 0..n {
            f(&mut r)?;
        }
    }
    Some(())
}

impl Help {
    pub fn parse(b: &[u8]) -> Option<Self> {
        if b.get(..4)? != [0x3f, 0x5f, 3, 0] {
            return None;
        }
        let mut files = HashMap::new();
        leaves(file(b, u32::from_le_bytes(b.get(4..8)?.try_into().ok()?) as usize)?, |r| {
            let name = r.sz()?;
            files.insert(name, file(b, r.u32()? as usize)?);
            Some(())
        })?;
        let mut h = Help::default();
        // |SYSTEM: the title, the contents topic and the windows.
        let sys = files.get("|SYSTEM")?;
        if sys.get(10..12)? != [0, 0] {
            return None; // A compressed |TOPIC: not what Mordor shipped.
        }
        let (mut r, mut contents) = (R { d: sys, i: 12 }, 0);
        while let (Some(t), Some(n)) = (r.u16(), r.u16()) {
            let d = r.take(usize::from(n))?;
            let s = |a: usize, z: usize| d.get(a..z).map(|s| ansi(s.split(|&c| c == 0).next().unwrap_or_default())).unwrap_or_default();
            let n16 = |a: usize| d.get(a..a + 2).map_or(0, |v| i32::from(i16::from_le_bytes([v[0], v[1]])));
            let flags = n16(0);
            let rgb = |a: usize, bit: i32| (flags & bit != 0).then(|| d.get(a..a + 3).map(|c| [c[0], c[1], c[2]])).flatten();
            match t {
                1 => h.title = s(0, d.len()),
                3 => contents = u32::from_le_bytes(d.get(..4)?.try_into().ok()?),
                6 => h.windows.push(Window { name: s(12, 21).to_ascii_lowercase(), caption: s(21, 72), rect: [n16(72), n16(74), n16(76), n16(78)], back: rgb(82, 0x100), head: rgb(86, 0x200) }),
                _ => {}
            }
        }
        if !h.windows.first().is_some_and(|w| w.name == "main") {
            h.windows.insert(0, Window { name: "main".into(), caption: h.title.clone(), rect: [0, 0, 1023, 1023], head: None, back: None });
        }
        // |FONT: face names, then each font.
        let f = files.get("|FONT")?;
        let n16 = |a: usize| f.get(a..a + 2).map_or(0, |v| usize::from(u16::from_le_bytes([v[0], v[1]])));
        let (nf, nd, fo, de) = (n16(0).max(1), n16(2), n16(4), n16(6));
        let len = (de.saturating_sub(fo) / nf).max(1);
        for i in 0..nd {
            let d = f.get(de + 11 * i..de + 11 * i + 11)?;
            let face = f.get(fo + len * n16(de + 11 * i + 3)..).map_or(String::new(), |s| ansi(&s[..len.min(s.len())]).trim_end_matches('\0').split('\0').next().unwrap_or_default().to_string());
            let color = (d[5..8] != [1, 1, 0]).then(|| [d[5], d[6], d[7]]);
            h.fonts.push(Font { face, size: f32::from(d[1]) / 2.0, bold: d[0] & 1 != 0, italic: d[0] & 2 != 0, under: d[0] & 4 != 0, color });
        }
        // |CONTEXT: jump hashes to topic offsets.
        let mut ctx = HashMap::new();
        leaves(files.get("|CONTEXT")?, |r| {
            ctx.insert(r.u32()?, r.u32()?);
            Some(())
        })?;
        // |TOPIC: 4 KB blocks, each a 12-byte header and a share of the links.
        let t = files.get("|TOPIC")?;
        let s: Vec<u8> = t.chunks(4096).flat_map(|c| c.get(12..).unwrap_or_default()).copied().collect();
        let lin = |pos: u32| (pos >> 14) as usize * 4084 + (pos & 0x3fff) as usize - 12;
        let (mut pos, mut block, mut chars, mut offsets, mut browse, mut scroll, mut row) = (12u32, 0, 0u32, Vec::new(), Vec::new(), 0u32, 0);
        while pos != u32::MAX && lin(pos) + 21 <= s.len() {
            let mut r = R { d: &s, i: lin(pos) };
            let (size, _, _, next, len1, kind) = (r.u32()? as usize, r.u32()?, r.u32()?, r.u32()?, r.u32()? as usize, r.u8()?);
            let o = lin(pos);
            let (d1, d2) = (s.get(o + 21..o + len1)?, s.get(o + len1..o + size)?);
            if (pos >> 14) != block {
                (block, chars) = (pos >> 14, 0);
            }
            let mut r = R { d: d1, i: 0 };
            if kind == 2 {
                r.u32()?;
                let (bck, fwd) = (r.u32()?, r.u32()?);
                r.u32()?;
                r.u32()?;
                scroll = r.u32()?;
                offsets.push((block << 15) + chars);
                browse.push((bck, fwd));
                h.topics.push(Topic { title: ansi(d2.split(|&c| c == 0).next().unwrap_or_default()), ..Default::default() });
            } else if kind == 0x20 || kind == 0x23 {
                r.cs32()?;
                chars += r.cu16()?;
                let mut text = d2.split(|&c| c == 0);
                let mut cols = Vec::new();
                if kind == 0x23 {
                    let (n, ty) = (r.u8()?, r.u8()?);
                    if ty == 0 || ty == 2 {
                        r.u16()?;
                    }
                    for _ in 0..n {
                        cols.push((r.u16()? as i16 as i32, r.u16()? as i16 as i32));
                    }
                    row += 1;
                }
                loop {
                    let cell = if kind == 0x23 {
                        let c = r.u16()? as i16;
                        if c < 0 {
                            break;
                        }
                        r.take(3)?;
                        // Each column: its width, then the gap before it (the first's is the indent).
                        let x = cols.iter().take(c as usize).map(|&(w, _)| w).sum::<i32>() + cols.iter().take(c as usize + 1).map(|&(_, g)| g).sum::<i32>();
                        Some((row, x, cols.get(c as usize)?.0))
                    } else {
                        None
                    };
                    r.take(4)?;
                    let bits = r.u16()?;
                    if bits & 1 != 0 {
                        r.cs32()?;
                    }
                    let mut v = [0; 6];
                    for (k, x) in v.iter_mut().enumerate() {
                        if bits & (2 << k) != 0 {
                            *x = r.cs16()?;
                        }
                    }
                    if bits & 0x100 != 0 {
                        r.take(3)?;
                    }
                    let mut tabs = Vec::new();
                    if bits & 0x200 != 0 {
                        for _ in 0..r.cs16()? {
                            let t = r.cu16()?;
                            if t & 0x4000 != 0 {
                                r.cu16()?;
                            }
                            tabs.push((t & 0x3fff) as i32);
                        }
                    }
                    let align = if bits & 0x800 != 0 { 2 } else { u8::from(bits & 0x400 != 0) };
                    let fmt = Fmt { above: v[0], below: v[1], left: v[3], right: v[4], first: v[5], align, tabs };
                    let mut items = Vec::new();
                    loop {
                        let s = text.next().unwrap_or_default();
                        if !s.is_empty() {
                            items.push(Item::Text(ansi(s)));
                        }
                        match r.u8()? {
                            0xff => break,
                            0x20 => {
                                r.take(4)?;
                            }
                            0x21 => {
                                r.take(2)?;
                            }
                            0x80 => items.push(Item::Font(r.u16()?)),
                            0x81 => items.push(Item::Break),
                            0x82 => items.push(Item::Par),
                            0x83 => items.push(Item::Tab),
                            c @ 0x86..=0x88 => {
                                let ty = r.u8()?;
                                let n = usize::try_from(r.cs32()?).ok()?;
                                if ty == 0x22 {
                                    r.cu16()?;
                                }
                                let d = r.take(n)?;
                                let pic = match ty {
                                    // An embedded window: "FH_BMP.DLL,FH_BMP,goblin.bmp" showed that file.
                                    5 => Pic::File(ansi(d.get(6..)?).trim_end_matches('\0').rsplit(',').next()?.to_ascii_uppercase()),
                                    _ => Pic::Bm(u16::from_le_bytes(d.get(2..4)?.try_into().ok()?)),
                                };
                                items.push(Item::Pic(pic, c - 0x86));
                            }
                            0x89 => items.push(Item::EndLink),
                            0x8b => items.push(Item::Text("\u{a0}".into())),
                            0x8c => items.push(Item::Text("-".into())),
                            0xc8 | 0xcc => {
                                let n = usize::from(r.u16()?);
                                r.take(n.saturating_sub(3))?;
                            }
                            c @ 0xe0..=0xe7 => {
                                let v = r.u32()?;
                                // Windows 3.0 jumps name the topic's offset; later ones a hash.
                                let o = if c < 0xe2 { v } else { *ctx.get(&v).unwrap_or(&u32::MAX) };
                                items.push(Item::Link(o as usize, 0));
                            }
                            0xea | 0xeb | 0xee | 0xef => {
                                let n = usize::from(r.u16()?);
                                let d = r.take(n)?;
                                let v = u32::from_le_bytes(d.get(1..5)?.try_into().ok()?);
                                let win = if d[0] == 1 { usize::from(*d.get(5)?) } else { 0 };
                                items.push(Item::Link(*ctx.get(&v).unwrap_or(&u32::MAX) as usize, win));
                            }
                            _ => return None,
                        }
                    }
                    let topic = h.topics.last_mut()?;
                    let head = scroll != u32::MAX && pos < scroll;
                    if head { &mut topic.head } else { &mut topic.body }.push(Block { fmt, items, cell });
                    if kind == 0x20 {
                        break;
                    }
                }
            }
            pos = if next > pos { next } else { u32::MAX };
        }
        // Offsets to topics: the last topic starting at or before.
        let at = |o: u32| (o != u32::MAX).then(|| offsets.partition_point(|&t| t <= o).checked_sub(1)).flatten();
        for (t, &(bck, fwd)) in h.topics.iter_mut().zip(&browse) {
            (t.prev, t.next) = (at(bck), at(fwd));
            for b in t.head.iter_mut().chain(t.body.iter_mut()) {
                for i in &mut b.items {
                    if let Item::Link(o, w) = i {
                        *i = at(*o as u32).map_or(Item::EndLink, |t| Item::Link(t, *w));
                    }
                }
            }
        }
        h.contents = at(contents).unwrap_or(0);
        // |CTXOMAP: context numbers; |KWBTREE and |KWDATA: the keywords.
        let m = files.get("|CTXOMAP")?;
        for e in m.get(2..)?.as_chunks::<8>().0.iter().take(usize::from(u16::from_le_bytes(m.get(..2)?.try_into().ok()?))) {
            if let Some(t) = at(u32::from_le_bytes(e[4..8].try_into().ok()?)) {
                h.map.insert(i32::from_le_bytes(e[..4].try_into().ok()?), t);
            }
        }
        let kd = files.get("|KWDATA")?;
        leaves(files.get("|KWBTREE")?, |r| {
            let (word, n, o) = (r.sz()?, usize::from(r.u16()?), r.u32()? as usize);
            let ts = kd.get(o..o + 4 * n)?.as_chunks::<4>().0.iter().filter_map(|c| at(u32::from_le_bytes(*c))).collect();
            h.keywords.push((word, ts));
            Some(())
        })?;
        h.files = files.into_iter().filter(|(k, _)| k.starts_with("|bm") || !k.starts_with('|')).map(|(k, v)| (k, v.to_vec())).collect();
        Some(h)
    }

    /// A picture as (width, height, RGBA).
    pub fn picture(&self, p: &Pic) -> Option<(usize, usize, Vec<u8>)> {
        let n = match p {
            Pic::File(f) => return crate::bmp(self.files.get(f)?),
            Pic::Bm(n) => n,
        };
        // An MRB/SHG file's first picture: a DIB, plain or run-length packed.
        let d = self.files.get(&format!("|bm{n}"))?;
        let at = u32::from_le_bytes(d.get(4..8)?.try_into().ok()?) as usize;
        let mut r = R { d, i: at };
        let (ty, pack) = (r.u8()?, r.u8()?);
        if ty != 6 || pack > 1 {
            return None;
        }
        r.cu32()?;
        r.cu32()?;
        r.cu16()?;
        let (bits, w, h, used) = (r.cu16()?, r.cu32()?, r.cu32()?, r.cu32()?);
        r.cu32()?;
        let (size, _) = (r.cu32()? as usize, r.cu32()?);
        let (off, _) = (r.u32()? as usize, r.u32()?);
        let colors = if used == 0 { 1 << bits } else { used as usize };
        let pal = r.take(4 * colors)?;
        let src = d.get(at + off..at + off + size)?;
        let mut px = Vec::new();
        if pack == 1 {
            let mut i = 0;
            while let Some(&n) = src.get(i) {
                let k = usize::from(n & 0x7f);
                if n & 0x80 != 0 {
                    px.extend_from_slice(src.get(i + 1..i + 1 + k)?);
                    i += 1 + k;
                } else {
                    px.extend(std::iter::repeat_n(*src.get(i + 1)?, k));
                    i += 2;
                }
            }
        } else {
            px = src.to_vec();
        }
        // As a BMP file, for the BMP decoder.
        let head = 54 + pal.len();
        let mut f = b"BM".to_vec();
        for v in [(head + px.len()) as u32, 0, head as u32, 40, w, h] {
            f.extend(v.to_le_bytes());
        }
        f.extend([1, 0, bits as u8, 0]);
        f.extend([0u8; 24]);
        f.extend(pal);
        f.extend(px);
        crate::bmp(&f)
    }
}
