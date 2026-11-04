//! Imports data from the player's own copy of Mordor: The Depths of Dejenol.
//!
//! Accepts the distribution ZIP (e.g. the shareware `MORDOR11.ZIP`), a loose
//! `.WIP` installer archive, or an installed game folder. Files packed with
//! MS-DOS `COMPRESS.EXE` (`*.MD_`, as the registered dungeon ships) are expanded.
//! Only data, music, sound and picture files are copied out; executables are never
//! extracted (MORDOR.EXE is only scanned for its embedded pictures). Nothing produced
//! here may be committed or redistributed.

use std::fs;
use std::io::{self, Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

/// Files the remake reads. `MG11.DLL` holds the original monster bitmaps.
fn wanted(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    (n.starts_with("MDATA") && (n.ends_with(".MDR") || n.ends_with(".MD_")))
        || n.ends_with(".MID")
        || n.ends_with(".WAV")
        || n == "MG11.DLL"
        || n == "MORDOR.EXE"
}

/// The city and menu pictures VB3 embedded in MORDOR.EXE, by (width, height) in file
/// order. Only these bitmaps are written out; the program itself never is.
const EXE_PICTURES: [(&str, i32, i32); 11] = [
    ("CITY_EXIT", 120, 42),
    ("CITY_DUNGEON", 120, 87),
    ("CITY_GUILDS", 120, 87),
    ("CITY_SEER", 120, 87),
    ("CITY_MORGUE", 120, 87),
    ("CITY_BANK", 120, 87),
    ("CITY_STORE", 120, 87),
    ("CITY_CONFINEMENT", 120, 87),
    ("SPLASH", 300, 300),
    ("MENU", 600, 400),
    ("PORTRAIT", 150, 150),
];

fn exe_pictures(exe: &[u8], out: &Path, got: &mut Vec<String>) -> io::Result<()> {
    let at = |o: usize| exe.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let mut want = EXE_PICTURES.iter().peekable();
    for o in (0..exe.len().saturating_sub(54)).filter(|&o| &exe[o..o + 2] == b"BM") {
        let (Some(size), Some(hdr), Some(w), Some(h)) = (at(o + 2), at(o + 14), at(o + 18), at(o + 22)) else { continue };
        let Some(&&(name, ww, wh)) = want.peek() else { break };
        if hdr == 40 && (w, h) == (ww, wh) && exe.get(o..o + size as usize).is_some() {
            let file = format!("{name}.BMP");
            fs::write(out.join(&file), &exe[o..o + size as usize])?;
            got.push(file);
            want.next();
        }
    }
    Ok(())
}

/// Decodes a picture saved by the importer (e.g. `CITY_STORE.BMP`) as (width, height, RGBA).
pub fn picture(dir: &Path, name: &str) -> Option<(usize, usize, Vec<u8>)> {
    bmp8(&fs::read(dir.join(format!("{name}.BMP"))).ok()?)
}

/// Copies every wanted file from `src` into `out`, returning the file names.
pub fn import(src: &Path, out: &Path) -> io::Result<Vec<String>> {
    fs::create_dir_all(out)?;
    let mut got = Vec::new();
    if src.is_dir() {
        for e in fs::read_dir(src)? {
            let p = e?.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
            let upper = name.to_ascii_uppercase();
            if upper.ends_with(".WIP") || upper.ends_with(".ZIP") {
                got.extend(import(&p, out)?);
            } else if wanted(&name) {
                store(upper, fs::read(&p)?, out, &mut got)?;
            }
        }
    } else {
        unzip(fs::File::open(src)?, out, &mut got)?;
    }
    got.sort();
    got.dedup();
    Ok(got)
}

/// Extracts wanted entries, descending into nested `.WIP`/`.ZIP` archives.
fn unzip<R: Read + Seek>(r: R, out: &Path, got: &mut Vec<String>) -> io::Result<()> {
    let mut zip = ZipArchive::new(r).map_err(io::Error::other)?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(io::Error::other)?;
        // enclosed_name rejects absolute and `..` paths; keep only the base name.
        let Some(name) = f.enclosed_name().and_then(|p| p.file_name()?.to_str().map(str::to_ascii_uppercase)) else { continue };
        if name.ends_with(".WIP") || name.ends_with(".ZIP") {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            unzip(Cursor::new(buf), out, got)?;
        } else if wanted(&name) {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            store(name, buf, out, got)?;
        }
    }
    Ok(())
}

/// Writes one file, expanding `COMPRESS.EXE` output (name ending in `_`).
fn store(name: String, data: Vec<u8>, out: &Path, got: &mut Vec<String>) -> io::Result<()> {
    if name == "MORDOR.EXE" {
        return exe_pictures(&data, out, got);
    }
    let (name, data) = match name.strip_suffix('_') {
        Some(stem) => {
            let (last, data) = expand(&data)?;
            (format!("{stem}{}", char::from(last.to_ascii_uppercase())), data)
        }
        None => (name, data),
    };
    fs::write(out.join(&name), data)?;
    got.push(name);
    Ok(())
}

/// Expands an SZDD file (LZSS, 4 KB window). Returns the file name's missing
/// last character and the data.
fn expand(b: &[u8]) -> io::Result<(u8, Vec<u8>)> {
    if b.len() < 14 || &b[..8] != b"SZDD\x88\xf0\x27\x33" {
        return Err(io::Error::other("not a COMPRESS.EXE file"));
    }
    let len = u32::from_le_bytes([b[10], b[11], b[12], b[13]]) as usize;
    let (mut win, mut pos, mut out) = ([b' '; 4096], 4096 - 16, Vec::with_capacity(len));
    let mut src = b[14..].iter().copied();
    'outer: while out.len() < len {
        let Some(flags) = src.next() else { break };
        for bit in 0..8 {
            // Set bit: one literal byte. Clear: 12-bit window offset, 4-bit length - 3.
            let (off, n) = if flags >> bit & 1 == 1 {
                let Some(c) = src.next() else { break 'outer };
                win[pos] = c;
                (pos, 1)
            } else {
                let (Some(lo), Some(hi)) = (src.next(), src.next()) else { break 'outer };
                (usize::from(lo) | usize::from(hi & 0xf0) << 4, usize::from(hi & 15) + 3)
            };
            for k in 0..n {
                let c = win[(off + k) & 4095];
                win[pos] = c;
                pos = (pos + 1) & 4095;
                out.push(c);
            }
        }
    }
    if out.len() < len {
        return Err(io::Error::other("truncated COMPRESS.EXE file"));
    }
    out.truncate(len);
    Ok((b[9], out))
}

/// A named bitmap from MG11.DLL (`MONS`, `MAZE`, `DOOR`...) as (width, height, RGBA).
/// The DLL keeps them as resources of type "MORDOR" holding whole 8-bit BMP files.
pub fn mg_bitmap(dll: &[u8], name: &str) -> Option<(usize, usize, Vec<u8>)> {
    let u16_at = |o: usize| Some(u16::from_le_bytes(dll.get(o..o + 2)?.try_into().ok()?) as usize);
    let u32_at = |o: usize| Some(u32::from_le_bytes(dll.get(o..o + 4)?.try_into().ok()?) as usize);
    let ne = u32_at(0x3c)?;
    let rt = ne + u16_at(ne + 0x24)?;
    let (shift, mut p) = (u16_at(rt)?, rt + 2);
    let pstr = |o: usize| dll.get(rt + o + 1..rt + o + 1 + usize::from(*dll.get(rt + o)?));
    while u16_at(p)? != 0 {
        let count = u16_at(p + 2)?;
        for e in (0..count).map(|j| p + 8 + 12 * j) {
            let id = u16_at(e + 6)?;
            if id & 0x8000 == 0 && pstr(id)? == name.as_bytes() {
                return bmp8(dll.get(u16_at(e)? << shift..)?);
            }
        }
        p += 8 + 12 * count;
    }
    None
}

/// Decodes an 8-bit BMP file, plain or RLE8-compressed (the monster sheet is).
fn bmp8(b: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let i32_at = |o: usize| Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
    let (off, w, h, rle) = (i32_at(10)? as usize, i32_at(18)? as usize, i32_at(22)?, i32_at(30)? == 1);
    let pal = 14 + i32_at(14)? as usize;
    let rows = h.unsigned_abs() as usize;
    // Palette indexes, stored rows first (bottom-up unless the height is negative).
    let mut idx = vec![0u8; w * rows];
    if rle {
        let (mut x, mut y, mut p) = (0, 0, off);
        while p + 1 < b.len() && y < rows {
            let (n, v) = (usize::from(b[p]), b[p + 1]);
            p += 2;
            match (n, v) {
                (0, 0) => (x, y) = (0, y + 1),
                (0, 1) => break,
                (0, 2) => {
                    x += usize::from(*b.get(p)?);
                    y += usize::from(*b.get(p + 1)?);
                    p += 2;
                }
                (0, k) => {
                    let k = usize::from(k);
                    for i in 0..k {
                        if x < w {
                            idx[y * w + x] = *b.get(p + i)?;
                        }
                        x += 1;
                    }
                    p += k + (k & 1);
                }
                (n, v) => {
                    for _ in 0..n {
                        if x < w {
                            idx[y * w + x] = v;
                        }
                        x += 1;
                    }
                }
            }
        }
    } else {
        let stride = (w + 3) & !3;
        for y in 0..rows {
            idx[y * w..(y + 1) * w].copy_from_slice(b.get(off + y * stride..off + y * stride + w)?);
        }
    }
    let mut out = Vec::with_capacity(w * rows * 4);
    for y in 0..rows {
        let row = if h > 0 { rows - 1 - y } else { y };
        for &i in &idx[row * w..(row + 1) * w] {
            let c = pal + 4 * usize::from(i);
            out.extend_from_slice(&[*b.get(c + 2)?, *b.get(c + 1)?, *b.get(c)?, 255]);
        }
    }
    Some((w, rows, out))
}

/// Where imported data, saves and patches live: the working directory when it already
/// has `gamedata/` (running from source), else the user's own app data folder, since an
/// installed game's folder isn't writable.
pub fn home() -> PathBuf {
    if Path::new("gamedata").is_dir() {
        return PathBuf::from(".");
    }
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("XDG_DATA_HOME")).map(PathBuf::from);
    let base = base.or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    base.map_or_else(|| PathBuf::from("."), |b| b.join("DejenolRemake"))
}

/// Where imported data lives (gitignored when it's the source tree's `gamedata/`).
pub fn data_dir() -> PathBuf {
    home().join("gamedata")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_shareware_fixture() {
        // Under target/: the system temp dir can refuse writes from low-integrity folders.
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-extract-test");
        let _ = fs::remove_dir_all(&out);
        let zip = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/MORDOR11.ZIP");
        let got = import(&zip, &out).unwrap();
        for f in ["MDATA1.MDR", "MDATA11.MDR", "DUNGEON.MID", "MG11.DLL", "CITY_STORE.BMP", "MENU.BMP"] {
            assert!(got.iter().any(|g| g == f), "missing {f}");
        }
        assert_eq!(got.iter().filter(|g| g.ends_with(".MID")).count(), 16);
        assert!(got.iter().any(|g| g.ends_with(".WAV")));
        assert!(!got.iter().any(|g| g.ends_with(".EXE")));
    }

    #[test]
    fn expands_szdd() {
        // Literals "ab", then a 4-byte match at the window start: "ababab".
        let f = b"SZDD\x88\xf0\x27\x33Ar\x06\0\0\0\x03ab\xf0\xf1";
        assert_eq!(expand(f).unwrap(), (b'r', b"ababab".to_vec()));
    }

    #[test]
    fn reads_monster_sheet() {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-extract-mg");
        import(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/MORDOR11.ZIP"), &out).unwrap();
        let (w, h, px) = mg_bitmap(&fs::read(out.join("MG11.DLL")).unwrap(), "MONS").unwrap();
        assert_eq!((w, h, px.len()), (600, 540, 600 * 540 * 4));
        // The sheet is RLE8-compressed; these match an independent decoder.
        let at = |x: usize, y: usize| &px[(y * w + x) * 4..(y * w + x) * 4 + 3];
        assert_eq!((at(5, 87), at(5, 415), at(42, 169), at(90, 30)), (&[108, 60, 12][..], &[12, 156, 252][..], &[108, 156, 204][..], &[120, 120, 120][..]));
    }
}
