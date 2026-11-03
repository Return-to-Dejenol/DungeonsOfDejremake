//! Imports data from the player's own copy of Mordor: The Depths of Dejenol.
//!
//! Accepts the distribution ZIP (e.g. the shareware `MORDOR11.ZIP`), a loose
//! `.WIP` installer archive, or an installed game folder. Only data, music and
//! sound files are copied out; executables are never extracted. Nothing produced
//! here may be committed or redistributed.

use std::fs;
use std::io::{self, Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

/// Files the remake reads. `MG11.DLL` holds the original monster bitmaps.
fn wanted(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    (n.starts_with("MDATA") && n.ends_with(".MDR"))
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
                fs::copy(&p, out.join(&upper))?;
                got.push(upper);
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
            io::copy(&mut f, &mut fs::File::create(out.join(&name))?)?;
            got.push(name);
        }
    }
    Ok(())
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
}
