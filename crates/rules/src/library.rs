//! The Library (MDATA13.MDR, frmLibrary): what explorers have recorded of each monster
//! and item, shared by every character (saves/library.json), and its pages (fn1BBE).

use std::{collections::BTreeMap, io, path::Path};

use ddj_extract::vfs as fs;
use serde::{Deserialize, Serialize};

use crate::{
    game::{ALIGNMENTS, Character, Game, RESISTANCES, STAT_NAMES, count_word},
    mdr::{GameData, Words},
};

const FILE: &str = "library.json";
/// Text ids of the original's size names (gv05BC), monster abilities (gv08DC), special
/// attacks (gv090E), spell classes (gv07B0) and item abilities (gv0684).
const SIZES: [u32; 6] = [0xa0be_dc12, 0x4880_662c, 0x58de_2772, 0xb8d1_6159, 0x5a72_6c9b, 0xe7ba_f382];
const ABILITIES: [u32; 6] = [0x0ded_9363, 0xbab3_1830, 0x0b4a_af81, 0x5556_12c3, 0x841c_2672, 0x8c97_6c93];
const ATTACKS: [u32; 14] = [
    0xbc3e_60d1, 0xb31a_5d05, 0xedff_205d, 0x31ea_4166, 0x37a1_a174, 0xd532_cfc8, 0xef8c_9490, 0x8610_3bb9, 0x6dd4_ca9a, 0x8ecb_e13c, 0xf31c_697d, 0xfde2_0b3a, 0x31f4_faf8, 0xb540_7a00,
];
const CLASSES: [u32; 18] = [
    0x0ed7_62f9, 0x6a72_bc2f, 0x2586_66d3, 0xe669_8669, 0x2df7_d460, 0x7d37_6697, 0x2bee_dcb9, 0x07df_43f0, 0x30ad_5f4e, 0xba97_377b, 0xe27f_8f78, 0x18e4_97a6, 0x8da8_55c8, 0xbdce_0130, 0x3be5_0b99, 0x8540_0341,
    0x5bc0_8dc6, 0xf888_8b1a,
];
const ITEM_ABILITIES: [u32; 6] = [0xcb4b_34f3, 0xbab3_1830, 0xd1dc_2b2c, 0x0ded_9363, 0xf31c_697d, 0xfde2_0b3a];

/// One monster's or item's record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// How much is known: 0 nothing (not listed), 1 very little .. 4 all.
    pub known: u8,
    /// How many were seen (monsters) or found (items).
    pub count: u32,
    /// When last seen or found (m/d/yyyy); empty when not known.
    pub date: String,
    /// Who first recorded it; empty when not known.
    pub first: String,
    /// Where it was last seen: "x,y,level", "?" or "the City"; an item's level.
    pub last: String,
    /// The explorers a monster killed.
    pub deaths: u32,
    /// The monster (index) the last item found came from; none for an unknown one.
    pub from: Option<usize>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library {
    /// By index into `GameData::monsters` and `GameData::items`.
    pub monsters: BTreeMap<usize, Entry>,
    pub items: BTreeMap<usize, Entry>,
    /// Today (m/d/yyyy), for new records and "N days ago".
    #[serde(skip)]
    pub today: String,
}

/// The day number of a short date (m/d/yyyy): Hinnant's days-from-civil, from 1 March
/// of year 0.
pub fn day(date: &str) -> Option<i64> {
    let mut p = date.split('/').map(|x| x.parse::<i64>().ok());
    let (m, d, y) = (p.next()??, p.next()??, p.next()??);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    Some(era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy)
}

/// The short date (m/d/yyyy) of a day number (`day`): Hinnant's days-to-civil.
pub fn date(z: i64) -> String {
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{m}/{}/{}", doy - (153 * mp + 2) / 5 + 1, yoe + z.div_euclid(146_097) * 400 + i64::from(m <= 2))
}

/// Takes the last ", " (or other two characters) off, as the original's Left$(s, Len - 2).
fn cut(mut s: String) -> String {
    s.pop();
    s.pop();
    s
}

impl Library {
    /// The saved library, or a new one (sub05AA) when there is none.
    pub fn load(dir: &Path, today: String, d: &GameData, rng: &mut Character) -> Self {
        let l: Option<Self> = fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let l = l.unwrap_or_else(|| Self::create(d, day(&today).unwrap_or(0), rng));
        Self { today, ..l }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(FILE), serde_json::to_vec(self)?)
    }

    /// "Creating Library File..." (sub05AA): ten monsters known in full, 1-5 seen 2-6
    /// days ago somewhere unknown (the second and sixth of type 0, the first two of types
    /// 7, 8 and 14, the first of types 1 and 9); each uncursed level 1 item up to 200,000
    /// gold known in full, found once on level 1 2-6 days ago.
    fn create(d: &GameData, today: i64, rng: &mut Character) -> Self {
        let first = |k: i16| (1..d.monsters.len()).find(|&i| d.monsters[i].kind == k).map_or(-1, |i| i as i64);
        let picked = [first(0) + 5, first(0) + 1, first(7), first(7) + 1, first(8), first(8) + 1, first(14), first(14) + 1, first(1), first(9)];
        let mut l = Self::default();
        let ago = |rng: &mut Character| date(today - ((rng.rnd() * 5.0) as i64 + 2));
        for m in (0..d.monsters.len()).filter(|&m| picked.contains(&(m as i64))) {
            let count = (rng.rnd() * 5.0) as u32 + 1;
            l.monsters.insert(m, Entry { known: 4, count, date: ago(rng), last: "?".into(), ..Default::default() });
        }
        for i in (0..d.items.len()).filter(|&i| d.items[i].level == 1 && d.items[i].price <= 200_000 && d.items[i].w(40) == 0) {
            l.items.insert(i, Entry { known: 4, count: 1, date: ago(rng), last: "1".into(), ..Default::default() });
        }
        l
    }

    /// Another device's library (cloud saves): of each record, the better known or more
    /// seen copy.
    pub fn merge(&mut self, other: Library) {
        for (mine, theirs) in [(&mut self.monsters, other.monsters), (&mut self.items, other.items)] {
            for (k, e) in theirs {
                let m = mine.entry(k).or_default();
                if (e.known, e.count + e.deaths) > (m.known, m.count + m.deaths) {
                    *m = e;
                }
            }
        }
    }

    /// The listed entries (known at all) of the monsters or the items: (index, record).
    pub fn listed(&self, monsters: bool) -> impl Iterator<Item = (usize, &Entry)> {
        let book = if monsters { &self.monsters } else { &self.items };
        book.iter().filter(|(_, e)| e.known != 0).map(|(&k, e)| (k, e))
    }

    /// How many days ago a record was dated; none when it is not known.
    fn ago(&self, e: &Entry) -> Option<i64> {
        Some((day(&e.date)? - day(&self.today)?).abs())
    }

    /// Monster m's page (fn1BBE): what is known of it by its known level, the deaths,
    /// how many were seen, who recorded it first and where and when it was last seen.
    pub fn monster_page(&self, d: &GameData, m: usize) -> String {
        let e = self.monsters.get(&m).cloned().unwrap_or_default();
        let (mon, t) = (&d.monsters[m], |id| d.t(id));
        let them = crate::monsters::plural(d, m, 2, true);
        let size = t(SIZES[mon.w(64).clamp(0, 5) as usize]);
        let kind = d.monster_types.get(mon.kind as usize).map_or("", |k| k.as_str());
        let mut s = format!("{}{}\n\n", t(0x9443_3923), mon.name);
        match e.known {
            1 => s += &format!("{}{them}{}{size}{}", t(0xc09b_4db7), t(0x1836_6094), t(0x62ca_49b1)),
            2 => s += &format!("{}{them}{}{size}{}{kind}{}", t(0xc09b_4db7), t(0x1836_6094), t(0xf7ae_10a2), t(0x0151_adc4)),
            3 | 4 => {
                let al = t(ALIGNMENTS[mon.w(31).clamp(0, 2) as usize]);
                s += &format!("{them}{}{size}{}{kind}{}{al}{}\n", t(0x1490_fa65), t(0xf7ae_10a2), t(0x5efc_6f55), t(0x883c_7093));
                s += &format!("{}{}/{}{}{}{}", t(0xa079_e608), mon.attack, mon.defense, t(0x8561_769a), mon.hp, t(0x0802_84a1));
                for (k, v) in mon.stats.iter().take(6).enumerate().filter(|(_, v)| **v > 0) {
                    s += &format!("{v} {}, ", t(STAT_NAMES[k]));
                }
            }
            _ => {}
        }
        if e.known == 3 {
            s += t(0x6fd8_ec1c);
        }
        if e.known == 4 {
            s += t(0x796a_86b4);
            let bit = |at: usize, k: usize| mon.long(at) >> k & 1 != 0;
            let mut have: String = (0..6).filter(|&k| bit(20, k)).map(|k| format!("{}, ", t(ABILITIES[k]))).collect();
            let attacks = (0..14).filter(|&k| bit(22, k)).count();
            have += &(0..14).filter(|&k| bit(22, k)).map(|k| format!("{}, ", t(ATTACKS[k]))).collect::<String>();
            have += &(0..18).filter(|&k| bit(24, k)).map(|k| format!("\"{}\"{}", t(CLASSES[k]), t(0xd06b_2014))).collect::<String>();
            if have.is_empty() {
                s += t(0x1b6f_b1c1);
            } else {
                s += &format!("{}{}.", t(if attacks == 0 { 0x364a_de19 } else { 0x0bfa_0b89 }), cut(have));
            }
            s += &format!("\n{them}{}", t(0x1490_fa65));
            let res: String = (0..11).filter(|&k| mon.resist[k] > 0).map(|k| format!("{}{}{}, ", mon.resist[k], t(0xf3b9_4e80), t(RESISTANCES[k]))).collect();
            s += &if res.is_empty() { t(0x5bd1_5126).to_string() } else { format!("{}{}", cut(res), t(0x23d4_6218)) };
        }
        s += "\n\n";
        s += match e.known {
            4 => t(0xcc5e_89f3),
            3 => t(0xdf88_5b7f),
            2 => t(0xd54a_b9a3),
            1 => t(0x0c14_7494),
            _ => "",
        };
        s += "\n\n";
        s += &match e.deaths {
            0 => t(0xe5dc_f726).to_string(),
            n => format!("{}{n}{}", t(0x2ecc_a750), t(if n == 1 { 0xe71b_c8e0 } else { 0xa1a3_e64b })),
        };
        s += "\n";
        if e.count > 0 {
            s += &format!("{}{}{}\n", t(0x5665_62a9), e.count, t(0x0e67_dd24));
        }
        s += &if e.first.is_empty() { t(0xf0f9_9045).to_string() } else { format!("\"{}\"{}", e.first, t(0xb5af_2bb8)) };
        s += "\n";
        s += &if e.last.is_empty() { t(0x6627_461f).to_string() } else { format!("{}{}, ", t(0xec2f_4b9f), e.last) };
        s += &match self.ago(&e) {
            None => t(0x58b8_fe7f).to_string(),
            Some(0) => t(0xb5e5_5fc8).to_string(),
            Some(1) => t(0xde5d_8a93).to_string(),
            Some(n) => format!("{n}{}", t(0x08fc_258d)),
        };
        s
    }

    /// Item i's page (fn1BBE): its class, A/D, hands and swings, and when known in full
    /// its damage modifier, curse, spell, abilities and effects; its requirements and
    /// guilds, the finds and who recorded it first.
    pub fn item_page(&self, d: &GameData, i: usize) -> String {
        let e = self.items.get(&i).cloned().unwrap_or_default();
        let (it, t, all) = (&d.items[i], |id| d.t(id), e.known == 4);
        let kind = it.kind as usize;
        let mut s = format!("{}{}\n\n", t(0x4ac9_39dc), it.name);
        s += &format!("{}{}{}\n", t(0xce12_9e75), d.item_types.get(kind).map_or("", |k| k.as_str()), t(0x242e_3c1d));
        s += &format!("{}{}/{}", t(0xbe4c_a3f1), it.attack, it.defense);
        if all && it.damage_mult != 0.0 && d.item_type_class.get(kind) == Some(&0) {
            s += &format!("{}{}", t(0x9b68_3796), it.damage_mult);
        }
        s += t(0x0ed0_8534);
        if all && it.w(40) != 0 {
            s += t(0xcf31_cbc5);
        }
        match it.w(22) {
            hands if hands > 0 => {
                match it.power {
                    1 => s += t(0xcbb1_3ffa),
                    n if n > 1 => s += &format!("{}{n}{}", t(0x93cc_71fe), t(0x6521_295f)),
                    _ => {}
                }
                s += &format!("{}{}{}", t(0xaf61_d41e), count_word(d, hands as usize), t(0x8545_e6cf));
            }
            _ => s += t(0x84b9_d66a),
        }
        match crate::magic::item_spell(d, i).filter(|_| all) {
            Some(sp) => s += &format!("{}\"{}\"{}{}{}", t(0xfdd4_e2b4), d.spells[sp].name, t(0x5979_3779), it.w(13), t(0xa1a3_e64b)),
            None => s += ".",
        }
        s += "\n";
        if all {
            s += t(0xf24b_47dc);
            let mut have: String = (0..6).filter(|&k| it.long(7) >> k & 1 != 0).map(|k| format!("{}{}", t(ITEM_ABILITIES[k]), t(0xcac5_cc67))).collect();
            have += &(0..11).filter(|&k| it.long(24) >> k & 1 != 0).map(|k| format!("{}{}", t(RESISTANCES[k]), t(0x22a5_16c2))).collect::<String>();
            s += &if have.is_empty() { t(0xe1e6_e50f).to_string() } else { format!("{}{}.", t(0x7cca_9ef2), cut(have)) };
        }
        s += "\n";
        s += t(0x79d1_c575);
        let stats = |at: usize| -> String { (0..6).filter(|&k| it.w(at + k) != 0).map(|k| format!("{} {}, ", it.w(at + k), t(STAT_NAMES[k]))).collect() };
        let need = stats(26);
        s += &if need.is_empty() { t(0x4003_8c80).to_string() } else { format!("{}{}", cut(need), t(0x9c53_c1b0)) };
        if all {
            let adds = stats(33);
            let adds = if adds.is_empty() { adds } else { cut(adds) };
            match it.w(10) {
                1..=10 if !adds.is_empty() => s += &format!("{}{adds}{}", t(0x9020_33f7), t(0x3a18_8ab3)),
                1..=10 => {}
                11 => s += t(0xfd59_4aca),
                12 => s += t(0xdc17_7a23),
                13 => s += t(0xa9d5_1341),
                14 => s += t(0x3aa9_b21a),
                _ if !adds.is_empty() => s += &format!("{}{adds}{}", t(0xa88d_e25e), t(0x7082_60e8)),
                _ => {}
            }
            s += t(if it.w(42) == -1 { 0x9579_b4ae } else { 0xdd84_6028 });
            s += t(0xb9e1_7b19);
        } else {
            s += t(0xdd8b_2b07);
        }
        for g in (0..d.guilds.len()).filter(|&g| it.long(15) >> g & 1 != 0) {
            s += &d.guilds[g].name;
            s += &if all { format!("{}{}{}", t(0xa9c5_a5f5), crate::magic::guild_need(d, i, g), t(0x182f_4c88)) } else { ", ".into() };
        }
        s = cut(s) + ".\n\n";
        s += t(if all { 0x4617_6781 } else { 0xb6a8_1a45 });
        s += "\n\n";
        if e.count > 0 {
            s += &format!("{}{}{}", t(0x71da_dd84), e.count, t(0x3595_8281));
            s += &match e.from {
                Some(m) => format!("{}{}{}", d.monsters[m].name, t(0x984c_fb44), e.last),
                None => format!("{}{}", t(0x6f4c_87a6), e.last),
            };
            s += &match self.ago(&e) {
                None => t(0x58b8_fe7f).to_string(),
                Some(0) => t(0xc141_70ce).to_string(),
                Some(1) => t(0x5f9e_2697).to_string(),
                Some(n) => format!(", {n}{}", t(0x08fc_258d)),
            };
            s += "\n";
        }
        s += &if e.first.is_empty() { t(0xc338_be4f).to_string() } else { format!("\"{}\"{}", e.first, t(0x8f10_91fa)) };
        s
    }
}

impl Game {
    /// Group gr of the encounter as recorded (sub06FE): its monster and how many still
    /// stand, unless it passes for another monster.
    pub(crate) fn group_record(&self, gr: u8) -> Option<(usize, u32)> {
        let f = self.fight.as_ref()?;
        let m = f.foes.iter().find(|x| x.group == gr)?.monster;
        let n = f.foes.iter().filter(|x| x.group == gr && x.hp > 0).count() as u32;
        f.shown.get(usize::from(gr)).is_none_or(|s| s.as_m == m).then_some((m, n))
    }

    /// The groups of the room the party came into (sub0A91): each recorded by the
    /// leader, a new group's numbers counted as seen.
    pub(crate) fn record_groups(&mut self, d: &GameData, new: bool) {
        let groups = self.fight.as_ref().map(|f| f.groups_from(0)).unwrap_or_default();
        let seen: Vec<(usize, u32)> = groups.into_iter().filter_map(|gr| self.group_record(gr)).collect();
        for (m, n) in seen {
            self.record_monster(d, m, Some(0), None, new.then_some(n), false);
        }
    }

    /// sub06FE: monster m recorded by member j (none: new stock in Confinement), known
    /// at ID level `id` (fn121C, 0 known .. 3, as known 4 .. 1) or, when none is given,
    /// at j's own (fn121C with its sighting penalty) and dated today. An invisible one
    /// goes unrecorded by a member in the dungeon who can't see it. The first record
    /// names its recorder. Where it was is the leader's square, unless lost or in the
    /// city; `seen` adds that many seen, where then being "?" for stock or "the City"
    /// when nothing else is known; `death` counts an explorer it killed.
    pub(crate) fn record_monster(&mut self, d: &GameData, m: usize, j: Option<usize>, id: Option<u8>, seen: Option<u32>, death: bool) {
        let c = j.map(|j| &self.party[j]);
        if id.is_none() && d.monsters[m].flags & 2 != 0 && c.is_some_and(|c| !c.sees_invisible(d) && c.pos.floor > 0) {
            return;
        }
        let known = 4 - id.unwrap_or_else(|| c.map_or(3, |c| crate::monsters::id_level(c, d, m, true))).min(3);
        let who = c.map_or(String::new(), |c| c.name.clone());
        let (p, lost, today) = (self.pos(), self.lost.pos || self.lost.level, self.library.today.clone());
        let e = self.library.monsters.entry(m).or_default();
        if e.known == 0 {
            e.first = who;
        }
        e.known = e.known.max(known);
        if !lost && p.floor > 0 {
            e.last = format!("{},{},{}", p.x, p.y, p.floor);
        }
        if id.is_none() {
            e.date = today;
        }
        if let Some(n) = seen {
            if j.is_none() {
                e.last = "?".into();
            } else if p.floor == 0 && (e.last.is_empty() || e.date.is_empty()) {
                e.last = d.t(0xdd29_35bb).into();
            }
            e.count += n;
        }
        if death {
            e.deaths += 1;
        }
    }

    /// sub0730: item i recorded by member j (none: new store stock), at ID state `id`
    /// (`Held::id`: 0 known in full, 1 named; else recorded but not listed). The first
    /// record names its recorder. A find (`found`: the monster it came from, none for
    /// an unknown one) is dated today and counted, last on the leader's level ("?" when
    /// lost, and for stock).
    pub(crate) fn record_item(&mut self, i: usize, j: Option<usize>, id: u8, found: Option<Option<usize>>) {
        let known = match id {
            0 => 4,
            1 => 3,
            _ => 0,
        };
        let who = j.map_or(String::new(), |j| self.party[j].name.clone());
        let (p, lost, today) = (self.pos(), self.lost.pos || self.lost.level, self.library.today.clone());
        let e = self.library.items.entry(i).or_default();
        if e.known == 0 {
            e.first = who;
        }
        e.known = e.known.max(known);
        if let Some(from) = found {
            e.from = from;
            e.date = today;
            e.last = if j.is_some() && !lost { p.floor.to_string() } else { "?".into() };
            e.count += 1;
        }
    }
}
