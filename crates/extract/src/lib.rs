//! Imports data from the player's own copy of Mordor: The Depths of Dejenol.
//!
//! Accepts the distribution ZIP (e.g. the shareware `MORDOR11.ZIP`), a loose
//! `.WIP` installer archive, or an installed game folder. Files packed with
//! MS-DOS `COMPRESS.EXE` (`*.MD_`, as the registered dungeon ships) are expanded.
//! Only data, music and
//! sound files are copied out; executables are never extracted. Nothing produced
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

/// Decodes an uncompressed 8-bit, bottom-up BMP file.
fn bmp8(b: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let i32_at = |o: usize| Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
    let (off, w, h) = (i32_at(10)? as usize, i32_at(18)? as usize, i32_at(22)?);
    let pal = 14 + i32_at(14)? as usize;
    let (rows, stride) = (h.unsigned_abs() as usize, (w + 3) & !3);
    let mut out = Vec::with_capacity(w * rows * 4);
    for y in 0..rows {
        let row = if h > 0 { rows - 1 - y } else { y };
        for x in 0..w {
            let c = pal + 4 * usize::from(*b.get(off + row * stride + x)?);
            out.extend_from_slice(&[*b.get(c + 2)?, *b.get(c + 1)?, *b.get(c)?, 255]);
        }
    }
    Some((w, rows, out))
}

/// Where imported data lives: `gamedata/` in the working directory (gitignored).
pub fn data_dir() -> PathBuf {
    PathBuf::from("gamedata")
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
        for f in ["MDATA1.MDR", "MDATA11.MDR", "DUNGEON.MID", "MG11.DLL"] {
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
    }
}
