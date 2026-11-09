//! The original's own words. MORDOR.EXE's p-code keeps each string constant as
//! <link:2><length:2><text>; the importer writes every printable run after a 16-bit
//! length to STRINGS.TXT, one a line (more than the constants, which does no harm). The
//! remake names each by `id`, the FNV-1a hash of its text, so its source holds none of
//! the original's text (docs/formats.md "The original's text").

use std::{collections::HashMap, io, path::Path};

use crate::vfs;

pub const FILE: &str = "STRINGS.TXT";

/// A string's id: 32-bit FNV-1a of its UTF-8 text.
pub fn id(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

pub(crate) fn extract(exe: &[u8], out: &Path, got: &mut Vec<String>) -> io::Result<()> {
    let mut all = std::collections::BTreeSet::new();
    for o in 0..exe.len().saturating_sub(2) {
        let n = usize::from(u16::from_le_bytes([exe[o], exe[o + 1]]));
        if let Some(s) = exe.get(o + 2..o + 2 + n).filter(|s| (1..=1500).contains(&n) && s.iter().all(|&c| c >= 0x20 && c != 0x7F)) {
            all.insert(crate::hlp::ansi(s));
        }
    }
    vfs::write(out.join(FILE), all.into_iter().collect::<Vec<_>>().join("\n"))?;
    got.push(FILE.into());
    Ok(())
}

/// The imported text by id (empty before an import that read MORDOR.EXE).
pub fn load(dir: &Path) -> HashMap<u32, String> {
    let b = vfs::read(dir.join(FILE)).unwrap_or_default();
    String::from_utf8_lossy(&b).lines().map(|s| (id(s), s.to_string())).collect()
}
