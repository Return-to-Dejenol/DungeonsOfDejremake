//! The original game's character file, MDATA4.MDR: VB3 random-access records of 2900
//! bytes. Record 0 holds the version, record 1 the character count, then one character
//! per record. The layout comes from MORDOR.EXE's own type descriptors (docs/formats.md);
//! fields without a known meaning are named by offset (`w40` is the Integer at +40).

use std::{fmt::Write, fs, io, path::Path};

const REC: usize = 2900;

#[derive(Clone, Copy)]
enum Kind {
    Text(usize),
    I16,
    I32,
    F32,
    /// VB Currency: i64 / 10,000.
    Cur,
}
use Kind::*;

/// Name, offset, kind, then for repeated fields the count, stride and named sub-fields.
type Field = (&'static str, usize, Kind, usize, usize, &'static [(&'static str, usize, Kind)]);

const ITEM: &[(&str, usize, Kind)] =
    &[("w0", 0, I16), ("w2", 2, I16), ("w4", 4, I16), ("w6", 6, I16), ("w8", 8, I16), ("w10", 10, I16), ("w12", 12, I16), ("w14", 14, I16), ("w16", 16, I16)];
const GUILD: &[(&str, usize, Kind)] = &[("level", 0, I16), ("xp", 2, Cur), ("w10", 10, I16), ("w12", 12, I16), ("w14", 14, I16), ("f16", 16, F32), ("f20", 20, F32)];
const COMPANION: &[(&str, usize, Kind)] = &[
    ("name", 0, Text(15)),
    ("w15", 15, I16),
    ("w17", 17, I16),
    ("w19", 19, I16),
    ("w21", 21, I16),
    ("w23", 23, I16),
    ("w25", 25, I16),
    ("w27", 27, I16),
    ("w29", 29, I16),
    ("w31", 31, I16),
];
const ONE: &[(&str, usize, Kind)] = &[];

const FIELDS: &[Field] = &[
    ("name", 0, Text(30), 1, 0, ONE),
    ("race", 30, I16, 1, 0, ONE),
    ("alignment", 32, I16, 1, 0, ONE),
    ("sex", 34, I16, 1, 0, ONE),
    ("age_days", 36, F32, 1, 0, ONE),
    ("w40", 40, I16, 1, 0, ONE),
    ("w42", 42, I16, 1, 0, ONE),
    ("w44", 44, I16, 1, 0, ONE),
    ("w46", 46, I16, 1, 0, ONE),
    ("attack", 48, F32, 1, 0, ONE),
    ("defense", 52, F32, 1, 0, ONE),
    ("str", 56, I16, 1, 0, ONE),
    ("int", 58, I16, 1, 0, ONE),
    ("wis", 60, I16, 1, 0, ONE),
    ("con", 62, I16, 1, 0, ONE),
    ("cha", 64, I16, 1, 0, ONE),
    ("dex", 66, I16, 1, 0, ONE),
    ("w68", 68, I16, 1, 0, ONE),
    ("stat_mod", 70, I16, 7, 2, ONE),
    ("item", 84, I16, 41, 18, ITEM),
    ("item2", 822, I16, 41, 18, ITEM),
    ("w1560", 1560, I16, 36, 2, ONE),
    ("w1632", 1632, I16, 1, 0, ONE),
    ("f1634", 1634, F32, 1, 0, ONE),
    ("f1638", 1638, F32, 1, 0, ONE),
    ("w1642", 1642, I16, 1, 0, ONE),
    ("w1644", 1644, I16, 1, 0, ONE),
    ("xp", 1646, Cur, 1, 0, ONE),
    ("gold", 1654, Cur, 1, 0, ONE),
    ("hp", 1662, I16, 1, 0, ONE),
    ("max_hp", 1664, I16, 1, 0, ONE),
    ("c1666", 1666, Cur, 1, 0, ONE),
    ("swings", 1674, I16, 1, 0, ONE),
    ("w1676", 1676, I16, 1, 0, ONE),
    ("guild", 1678, I16, 16, 24, GUILD),
    ("companion", 2062, I16, 5, 33, COMPANION),
    ("w2227", 2227, I16, 1, 0, ONE),
    ("w2229", 2229, I16, 1, 0, ONE),
    ("w2231", 2231, I16, 1, 0, ONE),
    ("s2233", 2233, Text(30), 1, 0, ONE),
    ("c2263", 2263, Cur, 9, 8, ONE),
    ("w2335", 2335, I16, 6, 2, ONE),
    ("status", 2347, I16, 8, 2, ONE),
    ("w2363", 2363, I16, 12, 2, ONE),
    ("l2387", 2387, I32, 1, 0, ONE),
    ("w2391", 2391, I16, 12, 2, ONE),
    ("w2415", 2415, I16, 1, 0, ONE),
    ("w2417", 2417, I16, 1, 0, ONE),
    ("s2419", 2419, Text(10), 1, 0, ONE),
    ("w2807", 2807, I16, 1, 0, ONE),
    ("l2809", 2809, I32, 1, 0, ONE),
    ("l2813", 2813, I32, 1, 0, ONE),
    ("w2817", 2817, I16, 12, 2, ONE),
    ("weapon_slot", 2841, I16, 2, 2, ONE),
    ("w2845", 2845, I16, 1, 0, ONE),
    ("w2847", 2847, I16, 11, 2, ONE),
    ("w2869", 2869, I16, 1, 0, ONE),
    ("w2871", 2871, I16, 1, 0, ONE),
    ("w2873", 2873, I16, 1, 0, ONE),
    ("w2875", 2875, I16, 3, 2, ONE),
    ("w2881", 2881, I16, 1, 0, ONE),
];

/// Every leaf field as (full name, offset, kind).
fn leaves() -> impl Iterator<Item = (String, usize, Kind)> {
    FIELDS.iter().flat_map(|&(name, off, kind, n, stride, subs)| {
        (0..n).flat_map(move |i| {
            let base = off + i * stride;
            let whole = if n == 1 { name.to_string() } else { format!("{name}.{i}") };
            if subs.is_empty() { vec![(whole, base, kind)] } else { subs.iter().map(|&(s, o, k)| (format!("{whole}.{s}"), base + o, k)).collect() }
        })
    })
}

pub struct File {
    bytes: Vec<u8>,
}

impl File {
    pub fn read(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        if bytes.len() < 2 * REC + 30 {
            return Err(io::Error::other("not an MDATA4 character file (too short)"));
        }
        Ok(Self { bytes })
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        fs::write(path, &self.bytes)
    }

    fn count(&self) -> usize {
        (self.bytes.len() - 2 * REC).div_ceil(REC)
    }

    fn name(&self, c: usize) -> String {
        latin1(&self.bytes[(2 + c) * REC..][..30])
    }

    /// The record start of the character called `who` (or `#n`).
    fn find(&self, who: &str) -> Result<usize, String> {
        let c = match who.strip_prefix('#') {
            Some(n) => n.parse().ok().filter(|&n| n < self.count()),
            None => (0..self.count()).find(|&c| self.name(c).eq_ignore_ascii_case(who)),
        };
        c.map(|c| (2 + c) * REC).ok_or_else(|| format!("no character {who}"))
    }

    fn locate(&self, path: &str) -> Result<(usize, Kind), String> {
        let (who, field) = path.split_once('.').ok_or("fields are written NAME.field")?;
        let rec = self.find(who)?;
        let (_, off, kind) = leaves().find(|(n, ..)| n == field).ok_or_else(|| format!("no field {field}"))?;
        Ok((rec + off, kind))
    }

    fn value(&self, at: usize, kind: Kind) -> String {
        let b = &self.bytes;
        let get = |n: usize| b.get(at..at + n).unwrap_or(&[]);
        match kind {
            Text(n) => latin1(get(n)),
            I16 => get(2).try_into().map_or(String::new(), |a| i16::from_le_bytes(a).to_string()),
            I32 => get(4).try_into().map_or(String::new(), |a| i32::from_le_bytes(a).to_string()),
            F32 => get(4).try_into().map_or(String::new(), |a| f32::from_le_bytes(a).to_string()),
            Cur => get(8).try_into().map_or(String::new(), |a| (i64::from_le_bytes(a) as f64 / 10_000.0).to_string()),
        }
    }

    pub fn get(&self, path: &str) -> Result<String, String> {
        let (at, kind) = self.locate(path)?;
        Ok(self.value(at, kind))
    }

    pub fn set(&mut self, path: &str, v: &str) -> Result<(), String> {
        let (at, kind) = self.locate(path)?;
        let bad = || format!("{v} is not a valid value for {path}");
        let new: Vec<u8> = match kind {
            Text(n) => {
                let mut t: Vec<u8> = v.chars().map(|c| u8::try_from(c).unwrap_or(b'?')).take(n).collect();
                t.resize(n, b' ');
                t
            }
            I16 => v.parse::<i16>().map_err(|_| bad())?.to_le_bytes().to_vec(),
            I32 => v.parse::<i32>().map_err(|_| bad())?.to_le_bytes().to_vec(),
            F32 => v.parse::<f32>().map_err(|_| bad())?.to_le_bytes().to_vec(),
            Cur => ((v.parse::<f64>().map_err(|_| bad())? * 10_000.0).round() as i64).to_le_bytes().to_vec(),
        };
        let slot = self.bytes.get_mut(at..at + new.len()).ok_or("the record is cut short there")?;
        slot.copy_from_slice(&new);
        Ok(())
    }

    /// Every character's fields that are not zero or blank.
    pub fn show(&self) -> String {
        let mut s = String::new();
        for c in 0..self.count() {
            let rec = (2 + c) * REC;
            let who = self.name(c);
            let _ = writeln!(s, "#{c} {who}");
            for (name, off, kind) in leaves() {
                let v = self.value(rec + off, kind);
                if !matches!(v.as_str(), "" | "0") {
                    let _ = writeln!(s, "  {who}.{name} = {v}");
                }
            }
        }
        s
    }
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect::<String>().trim_end_matches([' ', '\0']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-character file round-trips name, gold and a guild level through set/get.
    #[test]
    fn edits_fields() {
        let mut bytes = vec![0; 3 * REC];
        bytes[2 * REC..2 * REC + 30].copy_from_slice(&[b' '; 30]);
        bytes[2 * REC..2 * REC + 3].copy_from_slice(b"Ann");
        let mut f = File { bytes };
        f.set("Ann.gold", "1500").unwrap();
        f.set("ann.guild.0.level", "200").unwrap();
        f.set("Ann.name", "Bo").unwrap();
        assert_eq!((f.get("Bo.gold").unwrap().as_str(), f.get("#0.guild.0.level").unwrap().as_str()), ("1500", "200"));
        assert_eq!(i16::from_le_bytes([f.bytes[2 * REC + 1678], f.bytes[2 * REC + 1679]]), 200);
        assert!(f.show().contains("Bo.gold = 1500"));
        assert!(f.get("Bo.nothing").is_err());
    }
}
