//! The Hall of Records (MORDOR.EXE's HALL module): twelve records shared by every
//! character (saves/hall.json), started with the original's record holders. It also
//! keeps the guilds' Guildmasters, who start as the original's.

use std::{collections::BTreeMap, io, path::Path};

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
/// What each record is for, as the game announces it (text ids).
const FOR: [u32; 12] = [
    0x7c15_6b09, 0xedd4_2534, 0x97d6_6ba9, 0x093a_4ee1, 0x24ef_6fb8, 0x2d96_b8b3, 0x5f5c_cc1f, 0xcff6_869e, 0xf3d2_0bbe, 0x5c2e_ddd9, 0x81c5_4cd0, 0xa905_3d9b,
];
/// The original's record holders (sub0595), by text id.
const HOLDERS: [u32; 12] = [
    0x0f17_479d, 0xc242_0a43, 0xb9f0_0312, 0x3eed_d4ac, 0xce37_a0ad, 0x0cd1_7f52, 0x0f17_479d, 0x0f17_479d, 0x0cd1_7f52, 0xa654_181f, 0xfa69_7791, 0x0cd1_7f52,
];

/// The original's Guildmasters (sub0565, "Creating GM File..."): name ids and levels.
const GM: [(u32, i64); 12] = [
    (0x0f17_479d, 64),
    (0x76ea_15a3, 56),
    (0xc88e_ee17, 62),
    (0x7d7b_b92f, 48),
    (0x3d4b_88d3, 67),
    (0xb85c_eaa6, 77),
    (0x0cd1_7f52, 56),
    (0xd59e_e0bd, 54),
    (0x46ab_c002, 52),
    (0xb1a0_a9cd, 61),
    (0x2ac3_2cba, 48),
    (0x3e9d_9cbc, 65),
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
    /// Guildmasters who took over from the original's, by guild (value: their level).
    #[serde(default)]
    pub guildmasters: BTreeMap<usize, Record>,
    /// Today (m/d/yyyy), for new records.
    #[serde(skip)]
    pub today: String,
}

impl Default for Hall {
    /// The original's records (sub0595, "Creating Hall File..."); their holders' names
    /// come from MORDOR.EXE (`holder`).
    fn default() -> Self {
        let values = [14, 15, 18, 14, 15, 17, 1, 1_322_451, 5_466_453, 17, 6, 15];
        Self {
            records: values.map(|value| Record { name: String::new(), value, date: None }).into(),
            guildmasters: BTreeMap::new(),
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
    pub fn check(&mut self, d: &GameData, which: usize, name: &str, value: i64) -> Option<u32> {
        if !self.better(d, which, value) {
            return None;
        }
        let r = &mut self.records[which];
        let same = r.name == name;
        *r = Record { name: name.into(), value, date: Some(self.today.clone()) };
        (!same && which != CREATURE).then_some(FOR[which])
    }

    fn better(&self, d: &GameData, which: usize, value: i64) -> bool {
        let r = &self.records[which];
        if which == CREATURE {
            let m = |id| d.monsters.iter().find(|m| i64::from(m.id) == id).map_or(-1, might);
            m(value) > m(r.value)
        } else {
            value > r.value
        }
    }

    /// Takes each of `other`'s records that beats this one's (cloud saves from another device).
    pub fn merge(&mut self, d: &GameData, other: Hall) {
        for (i, r) in other.records.into_iter().enumerate().take(self.records.len()) {
            if self.better(d, i, r.value) {
                self.records[i] = r;
            }
        }
        for (g, r) in other.guildmasters {
            if self.guildmasters.get(&g).is_none_or(|m| r.value > m.value) {
                self.guildmasters.insert(g, r);
            }
        }
    }

    /// Who holds record j: the original's holder until someone beats it.
    pub fn holder<'a>(&'a self, d: &'a GameData, j: usize) -> &'a str {
        match &self.records[j] {
            r if r.name.is_empty() && r.date.is_none() => d.t(HOLDERS[j]),
            r => &r.name,
        }
    }

    /// Guild g's Guildmaster and level ("" and 0 for none).
    pub fn guildmaster(&self, d: &GameData, g: usize) -> (String, i64) {
        match self.guildmasters.get(&g) {
            Some(r) => (r.name.clone(), r.value),
            None => GM.get(g).map_or((String::new(), 0), |&(n, l)| (d.t(n).into(), l)),
        }
    }

    /// Every record but the creature's, for a character.
    pub fn check_all(&mut self, d: &GameData, c: &Character) -> Vec<u32> {
        let mut v: Vec<(usize, i64)> = (0..6).map(|i| (i, i64::from(c.stats[i]))).collect();
        v.push((EXPERIENCE, c.xp.iter().sum::<u64>() as i64));
        v.push((WEALTH, c.bank as i64));
        v.extend([FIGHTER, MAGIC, THIEF].map(|i| (i, index(c, d, i))));
        v.into_iter().filter_map(|(i, x)| self.check(d, i, &c.name, x)).collect()
    }
}
