//! The Hall of Records (MORDOR.EXE's HALL module): twelve records shared by every
//! character (saves/hall.json), started with the original's record holders.

use std::{io, path::Path};

use ddj_extract::vfs as fs;
use serde::{Deserialize, Serialize};

use crate::{
    game::{Character, DEX, FIGHTING, THIEVING},
    mdr::{GameData, Monster},
};

const FILE: &str = "hall.json";
pub const CREATURE: usize = 6;
pub const EXPERIENCE: usize = 7;
pub const WEALTH: usize = 8;
pub const FIGHTER: usize = 9;
pub const MAGIC: usize = 10;
pub const THIEF: usize = 11;
/// What the record is for, as the game announces it.
const FOR: [&str; 12] = [
    "Strongest Character",
    "most Intelligent Character",
    "Wisest Character",
    "Healthiest Character",
    "most Attractive Character",
    "Quickest Character",
    "most powerful creature defeated",
    "most experience",
    "wealthiest character",
    "highest fighting ability",
    "highest spell knowledge & power level",
    "highest thieving ability",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    /// The stat, the monster id (record 6), or the amount.
    pub value: i64,
    /// When it was set (m/d/yyyy); none for the original's holders.
    pub date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hall {
    pub records: Vec<Record>,
    /// Today (m/d/yyyy), for new records.
    #[serde(skip)]
    pub today: String,
}

impl Default for Hall {
    /// The original's holders (sub0595, "Creating Hall File...").
    fn default() -> Self {
        let r = |name: &str, value| Record { name: name.into(), value, date: None };
        Self {
            records: vec![
                r("Crashland", 14),
                r("Mager", 15),
                r("Theshal", 18),
                r("Orgal", 14),
                r("Alaya", 15),
                r("Nimblefingers", 17),
                r("Crashland", 1),
                r("Crashland", 1_322_451),
                r("Nimblefingers", 5_466_453),
                r("Morgul", 17),
                r("Ge'nal", 6),
                r("Nimblefingers", 15),
            ],
            today: String::new(),
        }
    }
}

/// A creature's might (mdrhalGetIndexValue): attack, STR, hits, DEX, power level, and
/// 75 for each special attack and 100 for each spell class it casts.
pub fn might(m: &Monster) -> i64 {
    let base = (f64::from(m.attack) * 2.0 + f64::from(m.stats[0]) * 10.0 + f64::from(m.hp) / 10.0).round_ties_even() as i64;
    base + i64::from(m.stats[5]) * 2 + i64::from(m.power_level) + 75 * i64::from((m.specials & 0x3fff).count_ones()) + 100 * i64::from(m.spells.count_ones())
}

/// Fighting, spell knowledge and thieving out of 100 (mdrhalGetIndexValue).
pub fn index(c: &Character, d: &GameData, which: usize) -> i64 {
    let ab = |i| f64::from(c.ability(d, i));
    let pct = |v: f64, of: f64| (v / of * 100.0).round_ties_even() as i64;
    match which {
        FIGHTER => pct((ab(FIGHTING) + ab(2) * 0.2 + ab(1) * 0.2 + ab(3) * 0.2).round_ties_even(), 160.0),
        MAGIC => {
            let of: f64 = d.spells.iter().map(|s| f64::from(s.level) * 10.0 + 255.0).sum();
            let mut v = 0.0;
            for s in &d.spells {
                let known = (0..c.levels.len()).find(|&g| {
                    let rate = f64::from(d.guilds[g].spell_rates[s.class as usize]);
                    c.levels[g] > 0 && rate > 0.0 && f64::from(s.level) * rate <= f64::from(c.levels[g]) / 2.0 + 0.5
                });
                if let Some(g) = known {
                    v = (v + f64::from(s.level) * 10.0 + f64::from(c.levels[g]) / 2.0 + 0.5).round_ties_even();
                }
            }
            pct(v, of.max(1.0))
        }
        _ => pct((ab(THIEVING) * f64::from(c.stats[DEX])).max(1.0), 3500.0),
    }
}

impl Hall {
    pub fn load(dir: &Path, today: String) -> Self {
        let h: Option<Self> = fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok());
        Self { today, ..h.filter(|h| h.records.len() == FOR.len()).unwrap_or_default() }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(FILE), serde_json::to_vec(self)?)
    }

    /// mdrhalCheckRecord: a higher value takes the record. The announcement, unless the
    /// holder only bettered their own record or it was a creature.
    pub fn check(&mut self, d: &GameData, which: usize, name: &str, value: i64) -> Option<&'static str> {
        let r = &mut self.records[which];
        let better = if which == CREATURE {
            let m = |id| d.monsters.iter().find(|m| i64::from(m.id) == id).map_or(-1, might);
            m(value) > m(r.value)
        } else {
            value > r.value
        };
        if !better {
            return None;
        }
        let same = r.name == name;
        *r = Record { name: name.into(), value, date: Some(self.today.clone()) };
        (!same && which != CREATURE).then_some(FOR[which])
    }

    /// Every record but the creature's, for a character.
    pub fn check_all(&mut self, d: &GameData, c: &Character) -> Vec<&'static str> {
        let mut v: Vec<(usize, i64)> = (0..6).map(|i| (i, i64::from(c.stats[i]))).collect();
        v.push((EXPERIENCE, c.xp.iter().sum::<u64>() as i64));
        v.push((WEALTH, c.bank as i64));
        v.extend([FIGHTER, MAGIC, THIEF].map(|i| (i, index(c, d, i))));
        v.into_iter().filter_map(|(i, x)| self.check(d, i, &c.name, x)).collect()
    }
}
