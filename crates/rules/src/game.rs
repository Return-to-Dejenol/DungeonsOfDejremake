//! The game state: characters, the party, the city's services, dungeon movement and
//! saves. Combat is in `combat.rs`, spells in `magic.rs` and party AI in `ai.rs`; the
//! remake's rules are in docs/balance.md.

use std::{io, path::Path};

use ddj_extract::vfs as fs;

use serde::{Deserialize, Serialize};

use crate::{
    Tuning,
    ai::{self, Rule},
    combat::Fight,
    hall::Hall,
    mdr::{Floor, GameData, Words, square},
    monsters::Companion,
    store::{self, Store},
    treasure::Chest,
};

pub const SAVE_VERSION: u32 = 4;
/// Item 0, "Hands": every character's first slot, the weapon when no other is wielded
/// (two swings at half damage). It can't be sold, dropped, stolen or destroyed.
pub const HANDS: usize = 0;
/// The character you control plus up to three allies.
pub const MAX_PARTY: usize = 4;
/// Text ids (ddj_extract::text) of the alignments' names, the guild's "Make Level" title
/// and "Congratulations!".
pub const ALIGNMENTS: [u32; 3] = [0x7a8c_7df8, 0xe933_0778, 0x6fc6_dfe7];
pub const MAKE_LEVEL: u32 = 0x3457_7b25;
/// Text ids of the stats' full names, the directions, and the counts "One" to "Ten".
/// The resistances' names, in the race order.
pub const RESISTANCES: [u32; 11] = [0x0ed7_62f9, 0x6a72_bc2f, 0x2586_66d3, 0xe669_8669, 0xb31a_5d05, 0xbc3e_60d1, 0xb960_112c, 0x6dd4_ca9a, 0xcc56_5075, 0x8610_3bb9, 0xd532_cfc8];
pub const STAT_NAMES: [u32; 6] = [0xda4d_03d0, 0xae1e_75e2, 0x7421_f406, 0xdcbe_e1c6, 0x32eb_1663, 0x43a8_0f2f];
pub const DIRECTIONS: [u32; 4] = [0x675e_4fa4, 0x6733_095c, 0xb3e2_9f1a, 0x6b15_95ce];
const COUNTS: [u32; 10] = [0x1cb1_b18f, 0x20af_1fc9, 0x2e43_0863, 0xaf96_41c5, 0x2ee0_8fb5, 0x2cbc_324b, 0xdb3f_1e9e, 0xa6f5_0710, 0x9652_bb75, 0x1f8c_8bf4];

/// "One" to "Ten", then "A Lot" (the original's count words).
pub fn count_word(d: &GameData, n: usize) -> &str {
    d.t(COUNTS.get(n.wrapping_sub(1)).copied().unwrap_or(0x4215_c9b3))
}
pub const CONGRATULATIONS: u32 = 0xa577_628f;
/// Guild g's Crest is the item with id CREST + g.
const CREST: usize = 299;
pub const STATS: [&str; 6] = ["STR", "INT", "WIS", "CON", "CHA", "DEX"];
pub(crate) const STR: usize = 0;
pub(crate) const INT: usize = 1;
pub(crate) const WIS: usize = 2;
pub(crate) const CON: usize = 3;
pub(crate) const CHA: usize = 4;
pub(crate) const DEX: usize = 5;
pub(crate) const NOMAD: usize = 0;
pub(crate) const HEALER: usize = 11;
/// Slots of `Guild::abilities`.
/// The abilities' names (text ids): Thieving, Backstabbing, Critical Hit, Multiple
/// Swings, Fighting, Perception.
pub const ABILITIES: [u32; 6] = [0x624d_7d65, 0xa389_3100, 0xf31c_697d, 0xa6e6_0188, 0xe99e_db37, 0xfdfe_3e9a];
/// Male and Female (text ids).
pub const SEXES: [u32; 2] = [0x487d_36f0, 0xd8a2_6cb9];
pub(crate) const THIEVING: usize = 0;
pub(crate) const BACKSTAB: usize = 1;
pub(crate) const CRITICAL: usize = 2;
pub(crate) const FIGHTING: usize = 4;

/// Floor 0 is the town. Facing runs clockwise from 0 = north; x is east, y is north.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pos {
    pub x: usize,
    pub y: usize,
    pub floor: usize,
    pub facing: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Life {
    Alive,
    /// The corpse lies here (or is carried by the party) until it reaches the morgue.
    Dead(Pos),
    InMorgue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Held {
    /// Index into `GameData::items`.
    pub item: usize,
    pub equipped: bool,
    /// Uses left for potions, scrolls and spell-casting items; 0 for the rest.
    #[serde(default)]
    pub charges: i16,
    /// How unknown it is (MORDOR.EXE's identification level): 0 fully identified, 1-3
    /// needing that many steps at the store.
    #[serde(default)]
    pub id: u8,
    /// Its alignment (MORDOR.EXE): 0 none, 1 good, 2 neutral, 3 evil.
    #[serde(default)]
    pub align: u8,
    /// Saves before version 4 kept only "fully identified".
    #[serde(default, rename = "known", skip_serializing)]
    pub(crate) old_known: bool,
}

impl Held {
    pub fn known(&self) -> bool {
        self.id == 0
    }
}

/// Lasting conditions; the city cures them on arrival.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    /// Strengths: poison and disease the monster's level found, paralysis its power
    /// level (MORDOR.EXE); 0 when well.
    pub poison: u16,
    pub disease: u16,
    pub paralysis: u16,
    pub stoned: bool,
}

/// Spell effects, lasting until the city or death (the experience boost until the party
/// changes floor).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Buffs {
    /// Protection's strength P, worth CInt(3 ln(P)²) defense (fn13A8).
    pub protection: i32,
    /// Bit n: resistance slot n raised by a spell.
    pub resist: u16,
    /// The strongest experience boost (potions, Charm of Experience); 0 = none.
    pub xp_mult: f32,
    pub levitate: bool,
    pub see_invisible: bool,
    /// Sight Veil: unseen by monsters that don't see invisible.
    #[serde(default)]
    pub invisible: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Character {
    pub version: u32,
    pub name: String,
    pub race: usize,
    pub female: bool,
    /// 0 good, 1 neutral, 2 evil.
    pub alignment: u8,
    pub stats: [i16; 6],
    /// Level and XP in each guild, in `GameData::guilds` order; level 0 = not a member.
    pub levels: Vec<u32>,
    pub xp: Vec<u64>,
    pub guild: usize,
    pub gold: u64,
    #[serde(default)]
    pub bank: u64,
    pub hp: i32,
    pub max_hp: i32,
    #[serde(default)]
    pub sp: i32,
    pub items: Vec<Held>,
    pub pos: Pos,
    pub life: Life,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub buffs: Buffs,
    /// The automap: squares seen, per floor, row-major.
    pub seen: Vec<Vec<bool>>,
    pub kills: u32,
    /// Combat rules for when this character is AI-controlled, first match wins.
    #[serde(default)]
    pub macros: Vec<Rule>,
    /// Monsters fighting for this character (charmed, bound or bought).
    #[serde(default)]
    pub companions: Vec<Companion>,
    /// Allies who travel with this character, rejoined when it is loaded.
    #[serde(default)]
    pub allies: Vec<String>,
    #[serde(default)]
    pub sanctuary: Option<Pos>,
    /// Days lived; the city ages you as you rest there.
    #[serde(default)]
    pub age_days: u32,
    /// A guild quest: kill this monster before the next level.
    #[serde(default)]
    pub quest: Option<usize>,
    /// Or bring back this item (index into `GameData::items`).
    #[serde(default)]
    pub quest_item: Option<usize>,
    rng: u64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Open,
    Wall,
    Door,
}

/// The race's minimum plus 5 (cmbRace_Click). Creation then spends the race's stat points,
/// and taking a stat down toward its minimum gives points back.
pub fn start_stats(data: &GameData, race: usize) -> [i16; 6] {
    let r = &data.races[race];
    std::array::from_fn(|i| (r.stat_min[i] + 5).min(r.stat_max[i]))
}

/// What a square's side looks like from inside it; secret doors look like walls.
pub fn side(f: &Floor, x: usize, y: usize, dir: u8) -> Side {
    let (sq, right) = match dir % 4 {
        0 => (Some((x, y)), false),
        1 => (Some((x, y)), true),
        2 => (y.checked_sub(1).map(|y| (x, y)), false),
        _ => (x.checked_sub(1).map(|x| (x, y)), true),
    };
    let Some((x, y)) = sq else { return Side::Wall };
    let fl = f.square(x, y).flags;
    let (wall, door) = if right { (square::RIGHT_WALL | square::RIGHT_SECRET_DOOR, square::RIGHT_DOOR) } else { (square::UPPER_WALL | square::UPPER_SECRET_DOOR, square::UPPER_DOOR) };
    if fl & door != 0 { Side::Door } else if fl & wall != 0 { Side::Wall } else { Side::Open }
}

/// The square one step away, if it is on the map.
pub fn ahead(f: &Floor, x: usize, y: usize, dir: u8) -> Option<(usize, usize)> {
    let (x, y) = match dir % 4 {
        0 => (x, y + 1),
        1 => (x + 1, y),
        2 => (x, y.checked_sub(1)?),
        _ => (x.checked_sub(1)?, y),
    };
    (x < f.width && y < f.height).then_some((x, y))
}

impl Character {
    pub fn new(data: &GameData, name: &str, race: usize, female: bool, alignment: u8, stats: [i16; 6]) -> Result<Self, String> {
        let r = &data.races[race];
        let spent: i16 = stats.iter().zip(start_stats(data, race)).map(|(s, b)| s - b).sum();
        if name.trim().is_empty() || name.contains(['/', '\\', '.']) {
            return Err("Pick a name without slashes or dots.".into());
        }
        if name.trim().eq_ignore_ascii_case("store") {
            return Err("That name is taken by the store's file.".into());
        }
        if (0..6).any(|i| stats[i] < r.stat_min[i] || stats[i] > r.stat_max[i]) || spent > r.stat_points {
            return Err("Stats are outside the race's range.".into());
        }
        if r.alignments & (1 << alignment) == 0 {
            return Err(format!("{}s cannot be that alignment.", r.name));
        }
        let mut levels = vec![0; data.guilds.len()];
        levels[NOMAD] = 1;
        let hp = i32::from(stats[CON]) + 4;
        let seed = name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3));
        let mut c = Self {
            version: SAVE_VERSION,
            name: name.trim().into(),
            race,
            female,
            alignment,
            stats,
            xp: vec![0; levels.len()],
            levels,
            guild: NOMAD,
            gold: 1500,
            bank: 0,
            hp,
            max_hp: hp,
            sp: 0,
            items: vec![Held { equipped: true, ..crate::magic::new_held(data, HANDS, true) }],
            pos: Pos::default(),
            life: Life::Alive,
            status: Status::default(),
            buffs: Buffs::default(),
            seen: Vec::new(),
            kills: 0,
            macros: ai::preset(ai::Role::Fighter, |_| None),
            companions: Vec::new(),
            allies: Vec::new(),
            sanctuary: None,
            age_days: 16 * 365,
            quest: None,
            quest_item: None,
            rng: seed | 1,
        };
        c.sp = c.max_sp(data, &Tuning::load(&[]).expect("built-in tuning"));
        Ok(c)
    }

    /// 0..n, from a generator saved with the character.
    pub fn roll(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % u64::from(n.max(1))) as u32
    }

    pub(crate) fn chance(&mut self, p: f64) -> bool {
        f64::from(self.roll(10_000)) < p * 10_000.0
    }

    /// VB's `Rnd`: uniform on [0, 1).
    pub(crate) fn rnd(&mut self) -> f64 {
        f64::from(self.roll(1 << 24)) / f64::from(1 << 24)
    }

    /// `Int(Rnd * 100) + 1`, the original's percentile roll.
    pub(crate) fn d100(&mut self) -> i32 {
        self.roll(100) as i32 + 1
    }

    pub fn alive(&self) -> bool {
        self.life == Life::Alive
    }

    /// Able to act: alive, not paralysed or stoned.
    pub fn able(&self) -> bool {
        self.alive() && self.status.paralysis == 0 && !self.status.stoned
    }

    pub fn level(&self) -> u32 {
        self.levels[self.guild]
    }

    pub fn age(&self) -> u32 {
        self.age_days / 365
    }

    /// Total XP the current guild wants before its next level.
    pub fn xp_needed(&self, data: &GameData, t: &Tuning) -> u64 {
        t.xp_needed(self.level(), data.races[self.race].xp_factor, data.guilds[self.guild].xp_factor)
    }

    /// A guild's ability rate (the table's value, Warrior fighting 1.2).
    pub(crate) fn guild_ability(data: &GameData, g: usize, i: usize) -> f32 {
        // Healers are decent in melee (docs/balance.md): Villain's fighting.
        if g == HEALER && i == FIGHTING { 0.8 } else { data.guilds[g].abilities[i] }
    }

    /// The best guild's value of `f(guild, level)`, as the original takes the best
    /// guild. Nomad passes a 20% share to the other guilds, counting no more Nomad
    /// levels than the best other guild has; a pure Nomad gets 70% (docs/balance.md).
    pub(crate) fn guild_best(&self, f: impl Fn(usize, u32) -> f64) -> f64 {
        let other_level = (0..self.levels.len()).filter(|&g| g != NOMAD).map(|g| self.levels[g]).max().unwrap_or(0);
        let nomad = |l: u32| if l > 0 { f(NOMAD, l) } else { 0.0 };
        if other_level == 0 {
            return 0.7 * nomad(self.levels[NOMAD]);
        }
        let best_other = (0..self.levels.len()).filter(|&g| g != NOMAD && self.levels[g] > 0).map(|g| f(g, self.levels[g])).fold(0.0, f64::max);
        best_other.max(0.7 * nomad(self.levels[NOMAD])) + 0.2 * nomad(self.levels[NOMAD].min(other_level))
    }

    /// Ability rating (thieving, fighting...): the best guild's level - 1 x its rate.
    pub fn ability(&self, data: &GameData, i: usize) -> f32 {
        self.guild_best(|g, l| f64::from(l - 1) * f64::from(Self::guild_ability(data, g, i))) as f32
    }

    /// The best guild's (rate, level) for ability `i`, by rate x level (MORDOR.EXE's
    /// choice for critical hits and backstabs).
    pub(crate) fn best_rate(&self, data: &GameData, i: usize) -> (f64, f64) {
        (0..self.levels.len())
            .filter(|&g| self.levels[g] > 0)
            .map(|g| (f64::from(Self::guild_ability(data, g, i)), f64::from(self.levels[g])))
            .max_by(|a, b| (a.0 * a.1).total_cmp(&(b.0 * b.1)))
            .unwrap_or((0.0, 0.0))
    }

    /// The fighting multiplier and the level it comes from: 0.6 + (ln(100 + L) / 1.75 -
    /// 2.3) x rate² / 2 for the best guild (MORDOR.EXE).
    pub(crate) fn fighting(&self, data: &GameData) -> (f64, f64) {
        let f = |g: usize, l: u32| 0.6 + ((100.0 + f64::from(l)).ln() / 1.75 - 2.3) * f64::from(Self::guild_ability(data, g, FIGHTING)).powi(2) / 2.0;
        let level = (0..self.levels.len()).filter(|&g| self.levels[g] > 0).max_by(|&a, &b| f(a, self.levels[a]).total_cmp(&f(b, self.levels[b]))).map_or(1, |g| self.levels[g]);
        (self.guild_best(f), f64::from(level))
    }

    /// Melee rating (2 x STR + DEX) / 3, the decided stand-in for STR (docs/balance.md).
    pub(crate) fn melee_rating(&self, d: &GameData) -> i32 {
        (2 * i32::from(self.stat(d, STR)) + i32::from(self.stat(d, DEX))) / 3
    }

    /// The worn items' stat modifiers (w33-38; sub0F74).
    pub fn mods(&self, d: &GameData) -> [i16; 6] {
        let mut m = [0; 6];
        for h in self.items.iter().filter(|h| h.equipped) {
            let kin = crate::magic::kin(h, self.alignment);
            for (s, v) in m.iter_mut().enumerate() {
                *v += crate::magic::aligned(d.items[h.item].w(33 + s), kin);
            }
        }
        m
    }

    /// A stat with the worn modifiers, as MORDOR.EXE counts it for attack and defense,
    /// casting, thieving and identifying (natural stats alone for max SP, guilds and
    /// item requirements).
    pub fn stat(&self, d: &GameData, s: usize) -> i16 {
        self.stats[s] + self.mods(d)[s]
    }

    /// A worn item's flag (w7; sub11B9): 0 levitate, 1 invisible, 2 protection, 3 see
    /// invisible.
    fn worn(&self, d: &GameData, bit: u32) -> bool {
        self.items.iter().any(|h| h.equipped && crate::magic::kin(h, self.alignment) == 0 && d.items[h.item].long(7) >> bit & 1 != 0)
    }

    pub fn levitates(&self, d: &GameData) -> bool {
        self.buffs.levitate || self.worn(d, 0)
    }

    pub fn unseen(&self, d: &GameData) -> bool {
        self.buffs.invisible || self.worn(d, 1)
    }

    pub fn sees_invisible(&self, d: &GameData) -> bool {
        self.buffs.see_invisible || self.worn(d, 3)
    }

    /// Attack and defense as MORDOR.EXE keeps them: 10 each, the stat bonus, the best
    /// guild's A/D and the equipped items. The stat bonus is the original's (+4 a point
    /// of STR and DEX above 12, -4 below 8) on the melee rating, and STR above 15 adds
    /// defense (docs/balance.md).
    pub fn att_def(&self, data: &GameData, _t: &Tuning) -> (i32, i32) {
        let r = self.melee_rating(data);
        // Paralysis takes the stat bonus away (sub0D1D).
        let still = self.status.paralysis > 0;
        let bonus = if still { 0 } else { 4 * ((r - 12).max(0) - (8 - r).max(0)) };
        let strong = if still { 0 } else { (i32::from(self.stat(data, STR)) - 15).max(0) };
        let ad = |k: usize| self.guild_best(|g, l| guild_ad(data, g, l)[k]) as i32;
        // A worn protection item holds Int(current level / 2) + 1 (sub0894).
        let worn = if self.worn(data, 2) { self.level() as i32 / 2 + 1 } else { 0 };
        let p = f64::from(self.buffs.protection.max(worn).max(1));
        let (mut a, mut d) = (10 + bonus + ad(0), 10 + bonus + ad(1) + strong + (3.0 * p.ln().powi(2)).round_ties_even() as i32);
        // A neighbouring alignment halves an item's A/D, the opposite one negates it.
        for h in self.items.iter().filter(|h| h.equipped) {
            let (it, kin) = (&data.items[h.item], crate::magic::kin(h, self.alignment));
            let ad = |v: i16| if kin == 1 { (f64::from(v) / 2.0).round_ties_even() as i32 } else { i32::from(crate::magic::aligned(v, kin)) };
            a += ad(it.attack);
            d += ad(it.defense);
        }
        (a, d)
    }

    /// Swings a round before the weapon's: the best guild's (MORDOR.EXE: the first
    /// comes at level ln(11 - rate) x 50, each next at ln(11 - rate) x 2 x the last).
    pub(crate) fn swings(&self, data: &GameData) -> i32 {
        (0..self.levels.len())
            .map(|g| {
                let k = (11.0 - f64::from(Self::guild_ability(data, g, 3))).ln();
                let (mut t, mut n) = ((k * 50.0).round_ties_even(), 0);
                for i in 1..=self.levels[g] {
                    if f64::from(i) > t {
                        n += 1;
                        t = (k * 2.0 * t).round_ties_even();
                    }
                }
                n
            })
            .max()
            .unwrap_or(0)
    }

    /// The wielded weapon, else Hands.
    pub(crate) fn weapon(&self, data: &GameData) -> Option<usize> {
        let wielded = self.items.iter().find(|h| h.equipped && h.item != HANDS && data.items[h.item].power > 0).map(|h| h.item);
        wielded.or(self.items.iter().any(|h| h.item == HANDS).then_some(HANDS))
    }

    /// INT + WIS per level, as the original; casters' growth scales by `max_sp_mult`.
    pub fn max_sp(&self, data: &GameData, t: &Tuning) -> i32 {
        let best = self.levels.iter().max().copied().unwrap_or(1).max(1) as f64;
        let _ = data;
        let per = f64::from(self.stats[INT] + self.stats[WIS]);
        (per * (5.0 + t.spells.max_sp_mult * (best - 1.0))) as i32
    }

    pub fn add_xp(&mut self, data: &GameData, t: &Tuning, xp: u64) -> u64 {
        let xp = if self.buffs.xp_mult > 1.0 { (xp as f32 * self.buffs.xp_mult) as u64 } else { xp };
        // Pinned: XP stops one short of the level after next until the next level is made.
        let cap = t.xp_needed(self.level() + 1, data.races[self.race].xp_factor, data.guilds[self.guild].xp_factor) - 1;
        self.xp[self.guild] = (self.xp[self.guild] + xp).min(cap);
        xp
    }

    /// Making a level in the current guild (cmdMake_Click): the lines for Messages, or a
    /// message box (its title's text id, its text).
    pub fn make_level(&mut self, data: &GameData, t: &Tuning) -> Result<Vec<String>, (u32, String)> {
        let (need, gold) = (self.xp_needed(data, t), Tuning::level_gold(self.level(), data.guilds[self.guild].gold_factor));
        if self.xp[self.guild] < need {
            return Err((MAKE_LEVEL, data.t(0xffee_afe6).into()));
        }
        // An open quest (cmdMake_Click); a pinned character may file for forfeit.
        let owed = self.quest_item.filter(|&it| !self.items.iter().any(|h| h.item == it && !h.equipped));
        let quest = match (self.quest, owed) {
            (Some(m), _) => Some((0xa2c3_5d33, &data.monsters[m].name)),
            (_, Some(it)) => Some((0x4bc8_33ad, &data.items[it].name)),
            _ => None,
        };
        if let Some((what, name)) = quest {
            let pinned = if self.pinned(data, t) { format!("\n\n{}{}", data.t(0x5bee_3f22), data.t(0x0833_9ca1)) } else { String::new() };
            return Err((0xe241_4a85, format!("{}\n{}{name}.{pinned}", data.t(0xf445_728a), data.t(what))));
        }
        if self.gold + self.bank < gold {
            return Err((MAKE_LEVEL, data.t(0x778e_55d4).into()));
        }
        // The quest item is handed in.
        if let Some(it) = self.quest_item.take()
            && let Some(slot) = self.items.iter().position(|h| h.item == it && !h.equipped)
        {
            self.items.remove(slot);
        }
        self.pay(gold);
        let g = &data.guilds[self.guild];
        let highest = self.levels.iter().all(|&l| l <= self.level());
        // Unedited preset macros take up the spells the new level brings.
        let preset = |c: &Self| ai::preset(ai::Role::of(c.guild), |k| crate::magic::best_known(c, data, k));
        // Spell rules aside, as old saves (versions 1-2) keep them.
        let bare = |v: &[Rule]| v.iter().filter(|r| !matches!(r.then, ai::Act::Cast(..))).cloned().collect::<Vec<_>>();
        let unedited = bare(&self.macros) == bare(&preset(self));
        self.levels[self.guild] += 1;
        if unedited {
            self.macros = preset(self);
        }
        let level = self.level();
        let mut said = Vec::new();
        if highest {
            // Nomad: 3 a level to 20, then 1. Healer: 3 a level to its cap.
            let (per, cap) = match self.guild {
                NOMAD => (3, 20),
                HEALER => (3, g.hp_cap),
                _ => (g.hp_per_level, g.hp_cap),
            };
            let past_cap = level > cap as u32;
            let avg = if past_cap { g.hp_after } else { per };
            // MORDOR.EXE: the guild's hits, two times in three Int(Rnd x hits / 5) + 1 more,
            // and CON above 16 adds Int(Rnd x (CON - 16)) + 1 (half past the cap).
            let avg = i32::from(avg.max(1));
            let extra = if self.roll(3) < 2 { (self.rnd() * f64::from(avg) / 5.0) as i32 + 1 } else { 0 };
            let con = (i32::from(self.stats[CON]) - 16).max(0);
            let con = if con > 0 { (self.rnd() * f64::from(con)) as i32 + 1 } else { 0 };
            let mut gain = avg + extra + if past_cap { con / 2 } else { con };
            // Nomad's share: other guilds gain a hit per level Nomad has already reached.
            if self.guild != NOMAD && self.levels[NOMAD] >= level {
                gain += 1;
            }
            self.max_hp += gain;
            self.hp += gain;
        }
        // From level 5 a guild may set a quest at its quest chance (w32; cmdMake_Click):
        // 5% of the time the Questmaster lets you off; above level 15 a quarter are items.
        if level >= 5 && (self.rnd() * 100.0).round_ties_even() <= f64::from(g.w(32)) {
            said.push(data.t(0x9325_18b2).into());
            let q = (f64::from(level) / 25.0).round_ties_even().clamp(1.0, 10.0) as i16;
            if (self.rnd() * 100.0).round_ties_even() <= 5.0 {
                said.push(data.t(0xc74b_e239).into());
            } else if (self.rnd() * 100.0).round_ties_even() < 25.0 && level > 15 {
                if let Some(it) = self.quest_pick_item(data, (f64::from(q) / 1.5) as i16 + 1) {
                    self.quest_item = Some(it);
                    said.push(format!("{}{}.", data.t(0x9a6b_91c7), data.items[it].name));
                }
            } else if let Some(m) = self.quest_pick_monster(data, q) {
                self.quest = Some(m);
                said.push(format!("{}{}.", data.t(0x6119_fc50), data.monsters[m].name));
            }
        }
        Ok(said)
    }

    /// XP at its cap: one short of the level after next.
    fn pinned(&self, data: &GameData, t: &Tuning) -> bool {
        self.xp[self.guild] + 1 >= t.xp_needed(self.level() + 1, data.races[self.race].xp_factor, data.guilds[self.guild].xp_factor)
    }

    /// Whether a forfeit is on offer: a quest open while pinned.
    pub fn may_forfeit(&self, data: &GameData, t: &Tuning) -> bool {
        (self.quest.is_some() || self.quest_item.is_some()) && self.pinned(data, t)
    }

    /// Filing for forfeit (sub29BD): never with a cursed item worn; the guild loses
    /// Int(ln(L + 1)) levels and the hits they brought, 15 at least.
    pub fn forfeit(&mut self, data: &GameData, t: &Tuning) -> Result<(), String> {
        if self.items.iter().any(|h| h.equipped && crate::magic::cursed(data, h, self.alignment)) {
            return Err(data.t(0xf7a9_0469).into());
        }
        let (g, l) = (self.guild, self.level());
        let gd = &data.guilds[g];
        let (per, cap) = match g {
            NOMAD => (3, 20),
            HEALER => (3, gd.hp_cap),
            _ => (gd.hp_per_level, gd.hp_cap),
        };
        let n = (f64::from(l) + 1.0).ln() as u32;
        for k in (l.saturating_sub(n) + 1..=l).rev() {
            if (0..self.levels.len()).any(|o| o != g && self.levels[o] >= k) {
                continue;
            }
            self.max_hp -= if k > cap as u32 {
                i32::from(gd.hp_after)
            } else {
                let extra = if self.roll(3) < 2 { (self.rnd() * f64::from(per) / 5.0) as i32 - 1 } else { 0 };
                i32::from(per) + extra
            };
        }
        self.max_hp = self.max_hp.max(15);
        self.hp = self.max_hp;
        self.levels[g] = l.saturating_sub(n).max(1);
        (self.quest, self.quest_item) = (None, None);
        self.xp[g] = self.xp_needed(data, t);
        Ok(())
    }

    /// A quest monster (cmdMake_Click): of a random type, up to level found q, from the
    /// type's first 40% of the time and else from level q; rarely one of spawn weight
    /// below 10.
    fn quest_pick_monster(&mut self, data: &GameData, q: i16) -> Option<usize> {
        for _ in 0..1000 {
            let k = self.roll(data.monster_types.len() as u32) as i16;
            let mut of: Vec<usize> = (0..data.monsters.len()).filter(|&m| data.monsters[m].kind == k).collect();
            of.sort_by_key(|&m| data.monsters[m].level);
            let Some(end) = of.iter().rposition(|&m| data.monsters[m].level <= q) else { continue };
            let start = if self.rnd() * 100.0 < 40.0 { 0 } else { of.iter().position(|&m| data.monsters[m].level >= q).unwrap_or(0).min(end) };
            let m = of[start + (self.rnd() * (end - start) as f64).round_ties_even() as usize];
            if data.monsters[m].weight < 10 && self.rnd() * 100.0 > 25.0 {
                continue;
            }
            return Some(m);
        }
        None
    }

    /// A quest item: of a random type, level 1 to q, worth 100 or more, not cursed;
    /// rarely a rare one (w6 below 5).
    fn quest_pick_item(&mut self, data: &GameData, q: i16) -> Option<usize> {
        for _ in 0..1000 {
            let k = self.roll(data.item_types.len() as u32) as i16;
            let of: Vec<usize> = (0..data.items.len()).filter(|&i| data.items[i].kind == k && (1..=q).contains(&data.items[i].level) && data.items[i].price >= 100 && data.items[i].w(40) == 0).collect();
            let Some(&it) = of.get(self.roll(of.len().max(1) as u32) as usize) else { continue };
            if data.items[it].w(6) < 5 && self.rnd() * 100.0 > 25.0 {
                continue;
            }
            return Some(it);
        }
        None
    }

    /// Spends gold, taking from the bank when the purse runs short (as the original).
    pub fn pay(&mut self, gold: u64) {
        let from_purse = gold.min(self.gold);
        self.gold -= from_purse;
        self.bank = self.bank.saturating_sub(gold - from_purse);
    }

    /// Joining guild `g` (MORDOR.EXE): free for a guild you belong to, or while you
    /// belong to no guild but your current one; otherwise A² x 5356, where A sums
    /// (rate + 1) x 1.25 over the guild's abilities and max spell level / 3 over its
    /// spell classes (Nomad 1.7 million, Ninja 10.9 million).
    pub fn join_cost(&self, data: &GameData, g: usize) -> u64 {
        if self.levels[g] > 0 || (0..self.levels.len()).all(|o| o == self.guild || self.levels[o] == 0) {
            return 0;
        }
        let gd = &data.guilds[g];
        let a: f64 = gd.abilities.iter().filter(|&&v| v > 0.0).map(|&v| (f64::from(v) + 1.0) * 1.25).sum::<f64>()
            + (0..19).map(|c| f64::from(gd.single(71 + 2 * c))).filter(|&l| l > 0.0).map(|l| l / 3.0).sum::<f64>();
        (a * a * 5356.0).round_ties_even() as u64
    }

    /// What guild g says to this character, and whether it turns them away
    /// (frmCityGuild's sub2962).
    pub fn welcome(&self, data: &GameData, g: usize) -> (String, bool) {
        let gd = &data.guilds[g];
        if self.levels[g] > 0 {
            return (format!("\n{}", data.t(if self.female { 0xd1c1_812b } else { 0x8e5e_5007 })), false);
        }
        let mut s = String::new();
        if gd.races & (1 << self.race) == 0 {
            s += &format!("{}{}{}\n", data.t(0x01ff_f40f), data.races[self.race].name, data.t(0xf851_9f99));
        }
        if gd.alignments & (1 << self.alignment) == 0 {
            s += &format!("{}{}{}\n", data.t(0xfe65_e82c), data.t(ALIGNMENTS[usize::from(self.alignment)]), data.t(0x41bb_a887));
        }
        if (0..6).any(|i| self.stats[i] < gd.stat_req[i]) {
            s += data.t(0x39d4_1948);
        }
        if s.is_empty() { (data.t(0x8f8c_71eb).into(), false) } else { (s, true) }
    }

    /// Joining guild g, or acting as its member again (cmdJoin_Click): the Messages line
    /// (often none), or the message box's text.
    pub fn join(&mut self, data: &GameData, g: usize) -> Result<String, String> {
        let (refusal, refused) = self.welcome(data, g);
        if refused {
            return Err(refusal);
        }
        if self.items.iter().any(|h| h.equipped && crate::magic::cursed(data, h, self.alignment)) {
            return Err(data.t(0x52e3_ce99).into());
        }
        let cost = self.join_cost(data, g);
        if self.gold + self.bank < cost {
            return Err(data.t(0x22bd_be43).into());
        }
        self.pay(cost);
        if self.levels[g] == 0 {
            self.levels[g] = 1;
            self.macros = ai::preset(ai::Role::of(g), |k| crate::magic::best_known(self, data, k));
        }
        self.guild = g;
        let mut off = false;
        for h in self.items.iter_mut().filter(|h| h.equipped && data.items[h.item].guilds & (1 << g) == 0) {
            (h.equipped, off) = (false, true);
        }
        Ok(if off { data.t(0x7edb_3449).into() } else { String::new() })
    }

    /// Equipping or taking off an item (sub0F74, fn110D): two hands, the item class's
    /// count, the guild rule (w42: 0 any joined guild, -1 the current one) and its level
    /// CInt(3.97 x guild w112 x item w17), the natural stats (w26-31), and no stat pushed
    /// below the race minimum.
    pub fn toggle_equip(&mut self, data: &GameData, slot: usize) -> Result<(), String> {
        let it = &data.items[self.items[slot].item];
        if self.items[slot].equipped {
            if self.items[slot].item == HANDS {
                return Err("Your hands are always ready.".into());
            }
            if crate::magic::cursed(data, &self.items[slot], self.alignment) {
                return Err(data.t(0x8616_d862).into());
            }
            self.items[slot].equipped = false;
            // Putting a weapon away leaves the hands.
            if it.power > 0 {
                for h in self.items.iter_mut().filter(|h| h.item == HANDS) {
                    h.equipped = true;
                }
            }
            return Ok(());
        }
        let class = |i: &crate::mdr::Item| data.item_type_class.get(i.kind as usize).copied().unwrap_or(-1);
        let max = usize::try_from(class(it)).ok().and_then(|c| data.item_class_max.get(c)).copied().unwrap_or(0);
        if max <= 0 {
            return Err(format!("{} is used, not worn.", it.name));
        }
        let worn = || self.items.iter().filter(|h| h.equipped && h.item != HANDS).map(|h| &data.items[h.item]);
        let refuse = |id: u32| Err(data.t(id).into());
        if worn().map(|o| o.w(22)).sum::<i16>() + it.w(22) > 2 {
            return refuse(0xeda8_6a59);
        }
        if worn().filter(|o| class(o) == class(it)).count() >= max as usize {
            return refuse(0xb9a9_fca7);
        }
        let can = |g: usize| it.guilds & (1 << g) != 0;
        let need = |g: usize| ((f64::from(data.guilds[g].single(112)) * 3.97 * f64::from(it.w(17))).round_ties_even() as u32).max(1);
        let joined: Vec<usize> = (0..self.levels.len()).filter(|&g| self.levels[g] > 0 && can(g)).collect();
        if it.w(42) == 0 {
            if joined.is_empty() {
                return refuse(0x177d_4641);
            }
            if !joined.iter().any(|&g| self.levels[g] >= need(g)) {
                return refuse(0x91a0_377e);
            }
        } else if !can(self.guild) {
            return refuse(0x3ed9_504a);
        } else if self.level() < need(self.guild) {
            return refuse(0x4b92_dfcf);
        }
        if (0..6).any(|s| self.stats[s] < it.w(26 + s)) {
            return refuse(0x0f97_d4af);
        }
        let (mods, kin) = (self.mods(data), crate::magic::kin(&self.items[slot], self.alignment));
        if (0..6).any(|s| self.stats[s] + mods[s] + crate::magic::aligned(it.w(33 + s), kin) < data.races[self.race].stat_min[s]) {
            return refuse(0x1735_6505);
        }
        // A weapon takes the place of Hands.
        if class(it) == 0 {
            for h in self.items.iter_mut().filter(|h| h.item == HANDS) {
                h.equipped = false;
            }
        }
        self.items[slot].equipped = true;
        Ok(())
    }

    pub fn deposit(&mut self, gold: u64) {
        let n = gold.min(self.gold);
        self.gold -= n;
        self.bank += n;
    }

    pub fn withdraw(&mut self, gold: u64) {
        let n = gold.min(self.bank);
        self.bank -= n;
        self.gold += n;
    }

    pub(crate) fn see(&mut self, data: &GameData) {
        let Pos { x, y, floor, .. } = self.pos;
        if floor == 0 {
            return;
        }
        let f = &data.floors[floor - 1];
        self.seen.resize(data.floors.len(), Vec::new());
        self.seen[floor - 1].resize(f.width * f.height, false);
        self.seen[floor - 1][y * f.width + x] = true;
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(format!("{}.json", self.name)), serde_json::to_vec_pretty(self)?)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let mut c: Self = serde_json::from_slice(&fs::read(path)?)?;
        if c.version > SAVE_VERSION {
            return Err(io::Error::other(format!("save version {} is newer than this game", c.version)));
        }
        if c.version < 2 {
            c.macros = ai::preset(ai::Role::of(c.guild), |_| None);
            c.age_days = c.age_days.max(16 * 365);
        }
        // Version 4 keeps the original's identification levels.
        if c.version < 4 {
            for h in &mut c.items {
                h.id = if h.old_known { 0 } else { 2 };
            }
        }
        // Version 3 learns spells as MORDOR.EXE (about twice the guild level): macros
        // naming a spell no longer known are dropped.
        if c.version < 3 {
            c.macros.retain(|r| !matches!(r.then, ai::Act::Cast(..)));
        }
        // Saves from before Hands was carried get it back in the first slot.
        if !c.items.iter().any(|h| h.item == HANDS) {
            c.items.insert(0, Held { item: HANDS, equipped: false, charges: 0, id: 0, align: 0, old_known: true });
        }
        c.version = SAVE_VERSION;
        Ok(c)
    }
}

/// A guild's accumulated (attack, defense) at `level` (MORDOR.EXE): w114 and w116 are
/// added every Int(w119) levels up to the HP cap (w1), then at the gaps
/// CInt((w1 / 2 + L / 18) / (w119 + 3 for Nomad)) apart, until level w118.
pub(crate) fn guild_ad(data: &GameData, g: usize, level: u32) -> [f64; 2] {
    let gd = &data.guilds[g];
    let (cap, every, last) = (i32::from(gd.hp_cap), gd.single(119), u32::try_from(gd.w(118)).unwrap_or(0));
    let level = level.min(last) as i32;
    let mut steps = (level.min(cap) / (every as i32).max(1)).max(0);
    let mut l = cap;
    loop {
        let gap = ((f64::from(cap / 2) + f64::from(l) / 18.0) / (f64::from(every) + if g == NOMAD { 3.0 } else { 0.0 })).round_ties_even() as i32;
        l += gap.max(1);
        if l > level {
            break;
        }
        steps += 1;
    }
    [f64::from(steps) * f64::from(gd.single(114)), f64::from(steps) * f64::from(gd.single(116))]
}

/// The chance paralysis holds another tick (timAilment_Timer): ln(0.6 p) / ln 1.02 less
/// ((ln L + L / 10) / ln 1.05) / 10 + 1.5 CON + paralysis resistance, 5-90, for strength p
/// and current guild level L.
pub(crate) fn paralysis_stay(data: &GameData, c: &Character) -> f64 {
    let (p, l) = (f64::from(c.status.paralysis), f64::from(c.level().max(1)));
    let save = ((l.ln() + l / 10.0) / 1.05f64.ln()) / 10.0 + 1.5 * f64::from(c.stat(data, CON)) + f64::from(crate::monsters::resist(c, data, 8));
    ((0.6 * p).ln() / 1.02f64.ln() - save).clamp(5.0, 90.0)
}

/// A raise's odds (fn13D7): clamp(CInt(35 x CON / 4.5 + 10 k), 10, 98), k 1 for the
/// morgue window and Raise Dead, 2 Restore Flesh, 3 Resurrect.
pub(crate) fn raise_odds(c: &Character, k: i32) -> i32 {
    ((35.0 * f64::from(c.stats[CON]) / 4.5).round_ties_even() as i32 + 10 * k).clamp(10, 98)
}

/// After a failed raise (fn11A1): complications come at age / (lifespan / 2), and raise
/// the body anyway at -5 to every stat, 15 years older and max hits / 1.2 (the text id
/// of their words); otherwise the body ages 54 days and stays dead (None).
pub(crate) fn complications(data: &GameData, c: &mut Character) -> Option<u32> {
    let r = &data.races[c.race];
    if (c.rnd() * 100.0).floor() >= f64::from(c.age_days) / 365.0 / (f64::from(r.lifespan.max(1)) / 2.0) * 100.0 {
        c.age_days += 54;
        return None;
    }
    for s in 0..6 {
        c.stats[s] = (c.stats[s] - 5).max(r.stat_min[s]);
    }
    c.age_days += 5475;
    c.max_hp = (f64::from(c.max_hp) / 1.2) as i32;
    Some(0x0abc_27aa)
}

/// Names of the characters saved in `dir`.
pub fn saved(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|d| d.iter().filter_map(|p| p.file_name()?.to_str()?.strip_suffix(".json").filter(|n| !["store", "hall"].contains(n)).map(str::to_owned)).collect())
        .unwrap_or_default();
    v.sort();
    v
}


/// A running game: the party plus what isn't saved (the fight, the rooms, log).
/// `party[0]` is the character the player controls.
pub struct Game {
    pub party: Vec<Character>,
    pub fight: Option<Fight>,
    /// A chest left in the room after a fight, until the party moves on.
    pub chest: Option<Chest>,
    /// Each (floor, area) visited: its monsters and when it was stocked.
    pub rooms: std::collections::HashMap<(usize, i16), crate::combat::Room>,
    /// Ailment ticks so far (a combat round or a step each): the rooms' clock.
    pub ticks: u64,
    pub log: Vec<String>,
    /// The store's and Confinement's shared stock (saves/store.json).
    pub store: Store,
    /// The Hall of Records (saves/hall.json).
    pub hall: Hall,
    /// What the dungeon window's labels say about the controlled character: its own
    /// action (CHAR_HIT), what hit it (MONSTER_HIT) and its square (SQUARE).
    pub said: [String; 3],
    /// Message boxes for the window to show: title's text id, text.
    pub boxes: Vec<(u32, String)>,
}

/// `Game::said` slots, after the dungeon window's lblCharacterHit, lblMonsterHit and
/// lblSquareInfo.
pub const CHAR_HIT: usize = 0;
pub const MONSTER_HIT: usize = 1;
pub const SQUARE: usize = 2;

impl Game {
    pub fn new(lead: Character) -> Self {
        Self { party: vec![lead], fight: None, chest: None, rooms: Default::default(), ticks: 0, log: Vec::new(), store: Store::default(), hall: Hall::default(), said: Default::default(), boxes: Vec::new() }
    }

    pub fn lead(&self) -> &Character {
        &self.party[0]
    }

    pub fn lead_mut(&mut self) -> &mut Character {
        &mut self.party[0]
    }

    pub(crate) fn say(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
    }

    /// News of member i the original's way: its "You" words in the controlled
    /// character's label `at`, and in Messages by name: `named` when the original has
    /// such words, else the quoted name and the "You" words. Alone, the "You" words.
    pub(crate) fn tell(&mut self, i: usize, at: usize, you: String, named: Option<String>) {
        if i == 0 {
            self.said[at].clone_from(&you);
        }
        let line = if self.party.len() == 1 { you } else { named.unwrap_or_else(|| format!("\"{}\": {you}", self.party[i].name)) };
        self.say(line);
    }

    /// The quoted name, as the original names party members in Messages.
    pub(crate) fn quoted(&self, i: usize) -> String {
        format!("\"{}\"", self.party[i].name)
    }

    /// The Hall's records for the party, announced as the original does; true if any fell.
    pub fn records(&mut self, d: &GameData) -> bool {
        let before = self.hall.records.clone();
        for i in 0..self.party.len() {
            for what in self.hall.check_all(d, &self.party[i]) {
                let line = if self.party.len() == 1 { format!("{}{}!", d.t(0xfb4a_3a19), d.t(what)) } else { format!("{}{}{}!", self.quoted(i), d.t(0x2147_aff8), d.t(what)) };
                self.say(line);
            }
        }
        before != self.hall.records
    }

    /// After party member i made a level: past the Guildmaster's level, they are the
    /// guild's new Guildmaster, given its Crest (cmdMake_Click, modAwardCrest). The
    /// message boxes: title's text id, text.
    pub fn guildmaster(&mut self, d: &GameData, i: usize) -> Vec<(u32, String)> {
        let c = &mut self.party[i];
        let (g, level) = (c.guild, i64::from(c.levels[c.guild]));
        let (gm, gm_level) = self.hall.guildmaster(d, g);
        if gm_level >= level && !gm.is_empty() {
            return Vec::new();
        }
        let mut said = Vec::new();
        if gm != c.name {
            said.push((0x73d1_9d26, format!("{}\n{}", d.t(CONGRATULATIONS), d.t(0xc647_0b35))));
        }
        let crest = d.items.iter().position(|it| usize::try_from(it.id) == Ok(CREST + g));
        if let Some(k) = crest.filter(|&k| !c.items.iter().any(|h| h.item == k)) {
            let name = &d.items[k].name;
            if c.items.len() >= 40 {
                said.push((0x6255_8b51, format!("{}{name}{}", d.t(0x64e3_8593), d.t(0xe3c2_b5a9))));
            } else {
                c.items.push(crate::magic::new_held(d, k, true));
                said.push((CONGRATULATIONS, format!("{}{name}{}\n\n{}", d.t(0x90a6_d702), d.t(0x80be_a9a5), d.t(0xd523_aca6))));
            }
        }
        self.hall.guildmasters.insert(g, crate::hall::Record { name: c.name.clone(), value: level, date: None });
        said
    }

    pub fn pos(&self) -> Pos {
        self.party[0].pos
    }

    pub fn floor<'a>(&self, data: &'a GameData) -> Option<&'a Floor> {
        self.pos().floor.checked_sub(1).map(|f| &data.floors[f])
    }

    /// Party members who are alive.
    pub fn living(&self) -> Vec<usize> {
        (0..self.party.len()).filter(|&i| self.party[i].alive()).collect()
    }

    /// Adds a saved character as an ally: alive, in the city, and not Good with Evil.
    /// Adds a saved character as an ally (fn14CB's Join); the refusal is the original's.
    pub fn add_ally(&mut self, data: &GameData, ally: Character) -> Result<(), String> {
        let name = format!("\"{}\"", ally.name);
        if self.party.len() >= MAX_PARTY {
            return Err(data.t(0x37c2_52b8).into());
        }
        if !ally.alive() {
            return Err(format!("{name}{}", data.t(0xa32f_8c98)));
        }
        if self.party.iter().any(|m| m.alignment.abs_diff(ally.alignment) == 2) {
            return Err(format!("{name}{}", data.t(0x764b_5170)));
        }
        if self.party.iter().any(|m| m.name == ally.name) {
            return Err(format!("{name}{}", data.t(0x8a53_ab38)));
        }
        self.party.push(ally);
        self.party[0].allies = self.party[1..].iter().map(|m| m.name.clone()).collect();
        Ok(())
    }

    pub fn dismiss(&mut self, i: usize) -> Option<Character> {
        if i == 0 || i >= self.party.len() {
            return None;
        }
        let c = self.party.remove(i);
        self.party[0].allies = self.party[1..].iter().map(|m| m.name.clone()).collect();
        Some(c)
    }

    /// Moves member i to place j among the followers (the leader stays first).
    pub fn reorder(&mut self, i: usize, j: usize) {
        if i > 0 && j > 0 && i.max(j) < self.party.len() {
            let m = self.party.remove(i);
            self.party.insert(j, m);
            self.party[0].allies = self.party[1..].iter().map(|m| m.name.clone()).collect();
        }
    }

    /// Puts another living member in control (the original's Switch To).
    pub fn switch_to(&mut self, i: usize) {
        if i < self.party.len() && self.party[i].alive() {
            self.party.swap(0, i);
            let names: Vec<String> = self.party[1..].iter().map(|m| m.name.clone()).collect();
            self.party[0].allies = names;
            for m in &mut self.party[1..] {
                m.allies.clear();
            }
        }
    }

    /// Moves everyone, carried bodies included.
    pub(crate) fn set_pos(&mut self, data: &GameData, p: Pos) {
        let floor_changed = p.floor != self.pos().floor;
        self.store.deepest = self.store.deepest.max(p.floor);
        if (p.x, p.y, p.floor) != (self.pos().x, self.pos().y, self.pos().floor) {
            self.chest = None;
        }
        for m in &mut self.party {
            m.pos = p;
            if let Life::Dead(_) = m.life {
                m.life = Life::Dead(p);
            }
            if m.alive() {
                m.see(data);
                if floor_changed {
                    m.buffs.xp_mult = 0.0;
                }
            }
        }
    }

    /// Arriving in town: the living rest (healed, cured, aged a little), carried bodies
    /// go to the morgue, and the dungeon repopulates.
    pub(crate) fn to_town(&mut self, data: &GameData, t: &Tuning) {
        // Resting ages a day per 3.25% of hits missing (MORDOR.EXE), and the leader's
        // rest restocks the store.
        let lead = &self.party[0];
        let days = (100.0 - 100.0 * f64::from(lead.hp) / f64::from(lead.max_hp.max(1))) / 3.25;
        let days = if lead.alive() { days.round_ties_even() as u32 } else { 0 };
        for m in &mut self.party {
            m.pos = Pos::default();
            m.buffs = Buffs::default();
            match m.life {
                Life::Alive => {
                    let missing = 100.0 - 100.0 * f64::from(m.hp) / f64::from(m.max_hp.max(1));
                    m.age_days += (missing / 3.25).round_ties_even() as u32;
                    m.hp = m.max_hp;
                    m.sp = m.max_sp(data, t);
                    m.status = Status::default();
                }
                Life::Dead(_) => m.life = Life::InMorgue,
                Life::InMorgue => {}
            }
        }
        if days > 0 {
            let mut store = std::mem::take(self.stock(data));
            store.restock(data, &mut self.party[0], days);
            self.store = store;
        }
        self.fight = None;
    }

    pub fn enter_dungeon(&mut self, data: &GameData) -> Result<(), String> {
        if let Some(m) = self.party.iter().find(|m| !m.alive()) {
            return Err(format!("{} is dead; visit the morgue or dismiss them first.", m.name));
        }
        let f = &data.floors[0];
        let i = f.squares.iter().position(|s| s.flags & square::STAIRS_UP != 0).unwrap_or(0);
        self.set_pos(data, Pos { x: i % f.width, y: i / f.width, floor: 1, facing: 0 });
        Ok(())
    }

    pub fn turn(&mut self, by: u8) {
        if self.fight.is_none() {
            for m in &mut self.party {
                m.pos.facing = (m.pos.facing + by) % 4;
            }
        }
    }

    pub fn step(&mut self, data: &GameData, t: &Tuning) {
        if let Some(f) = &self.fight {
            if !f.peaceful {
                return self.say("You can't walk away; choose Run.");
            }
            // Peaceful monsters let you walk on through; they stay in their room.
            self.shelve(data);
        }
        let Some(f) = self.floor(data) else { return };
        let Pos { x, y, facing, .. } = self.pos();
        let secret = match facing {
            0 => f.square(x, y).flags & square::UPPER_SECRET_DOOR != 0,
            1 => f.square(x, y).flags & square::RIGHT_SECRET_DOOR != 0,
            _ => ahead(f, x, y, facing).is_some_and(|(nx, ny)| f.square(nx, ny).flags & if facing == 2 { square::UPPER_SECRET_DOOR } else { square::RIGHT_SECRET_DOOR } != 0),
        };
        let to = ahead(f, x, y, facing).filter(|&(nx, ny)| (side(f, x, y, facing) != Side::Wall || secret) && f.square(nx, ny).flags & square::ROCK == 0);
        let Some((nx, ny)) = to else { return };
        self.said = Default::default();
        if secret {
            self.say(data.t(0x3c78_91e7));
        }
        let back = self.pos();
        self.set_pos(data, Pos { x: nx, y: ny, ..back });
        self.walk_effects(data, t);
        self.arrive(data, t, back);
    }

    /// Each step: an ailment tick, SP trickles back, and a Healer in the party slowly
    /// mends everyone (docs/balance.md).
    fn walk_effects(&mut self, data: &GameData, t: &Tuning) {
        let healer = self.party.iter().any(|m| m.alive() && m.levels[HEALER] > 0);
        for m in self.party.iter_mut().filter(|m| m.alive()) {
            let max_sp = m.max_sp(data, t);
            m.sp = (m.sp + ((f64::from(max_sp) * t.spells.regen_per_step).ceil() as i32)).min(max_sp);
            if healer {
                m.hp = (m.hp + (m.max_hp / 100).max(1)).min(m.max_hp);
            }
        }
        self.ailments(data);
        self.companions_step(data);
    }

    /// One tick of the original's ailment timer (timAilment_Timer): a combat round in a
    /// fight, a step outside, never while idle (docs/balance.md). Poison and disease wear
    /// off when d200 + 2 (CON - 10) - 2 x strength beats 185 (195), else bite for 2 (1);
    /// paralysis wears off at `paralysis_stay`, and at once with no monsters left.
    pub(crate) fn ailments(&mut self, data: &GameData) {
        self.ticks += 1;
        let foes = self.fight.as_ref().is_some_and(|f| !f.over());
        let Pos { x, y, floor, .. } = self.pos();
        let sq = self.floor(data).map_or(0, |f| f.square(x, y).flags);
        let (sinking, quicksand) = (sq & (square::WATER | square::QUICKSAND) != 0, sq & square::QUICKSAND != 0);
        let (mut hurt, mut lost) = (Vec::new(), Vec::new());
        for (i, m) in self.party.iter_mut().enumerate().filter(|(_, m)| m.alive()) {
            let con = 2 * i32::from(m.stat(data, CON) - 10);
            for (k, cure, dmg) in [(0, 185, 2), (1, 195, 1)] {
                let strength = [m.status.poison, m.status.disease][k];
                if strength > 0 {
                    if m.roll(200) as i32 + con - 2 * i32::from(strength) > cure {
                        if k == 0 { m.status.poison = 0 } else { m.status.disease = 0 }
                    } else {
                        hurt.push((i, dmg));
                    }
                }
            }
            if m.status.paralysis > 0 && (!foes || f64::from(m.d100()) > paralysis_stay(data, m)) {
                m.status.paralysis = 0;
            }
            // Water and quicksand take the floor's number in hits from each member not
            // levitating, every tick; quicksand swallows an item from a random slot
            // F/25 of the time (timAilment_Timer).
            if sinking && !m.levitates(data) {
                hurt.push((i, floor as i32));
                let s = 1 + m.roll(39) as usize;
                if quicksand && m.rnd() * 25.0 < floor as f64 && s < m.items.len() && m.items[s].item != HANDS && !(500..512).contains(&data.items[m.items[s].item].w(10)) {
                    let h = m.items.remove(s);
                    lost.push((i, h.item));
                }
            }
            // Past 98% of the race's lifespan, death of natural causes at
            // Int(Rnd x 100) < 5 + ln(age% - 98).
            let pct = f64::from(m.age_days) / 365.0 / f64::from(data.races[m.race].lifespan.max(1)) * 100.0;
            if pct > 98.0 && (m.rnd() * 100.0).floor() < 5.0 + (pct - 98.0).ln() {
                hurt.push((i, -1));
            }
        }
        for (i, item) in lost {
            let (it, gone) = (&data.items[item].name, data.t(0xc7c7_7f7d));
            let (you, named) = (format!("{}{it}{gone}", data.t(0xd1a8_7aa4)), format!("{}{}{it}{gone}", self.quoted(i), data.t(0x07b6_f246)));
            self.tell(i, SQUARE, you, Some(named));
        }
        if sinking && self.party.iter().any(|m| m.alive() && !m.levitates(data)) {
            self.say(data.t(if quicksand { 0xd5ed_9dca } else { 0xba4a_abcb }));
        }
        for (i, dmg) in hurt {
            if dmg < 0 {
                let line = if self.party.len() == 1 { data.t(0x13a8_39ee).into() } else { format!("{}{}", self.quoted(i), data.t(0x92c4_4e25)) };
                self.say(line);
            }
            let dmg = if dmg < 0 { self.party[i].hp } else { dmg };
            self.hurt(data, i, dmg);
        }
    }

    /// Effects of the square just entered, then a possible encounter.
    pub(crate) fn arrive(&mut self, data: &GameData, t: &Tuning, back: Pos) {
        let Some(f) = self.floor(data) else { return };
        let Pos { x, y, floor, .. } = self.pos();
        let sq = f.square(x, y);
        let (flags, area) = (sq.flags, sq.area);
        for (bit, dir) in [(square::FACE_NORTH, 0), (square::FACE_EAST, 1), (square::FACE_SOUTH, 2), (square::FACE_WEST, 3)] {
            if flags & bit != 0 {
                self.turn_to(dir);
            }
        }
        if flags & square::ROTATOR != 0 {
            let dir = self.party[0].roll(4) as u8;
            self.turn_to(dir);
        }
        // The size term of a fall: ln(race size + 2).
        let size = |c: &Character| (f64::from(data.races[c.race].w(28)) + 2.0).ln();
        if flags & square::PIT != 0 {
            // Each member's own levitation holds, but fails (5 + F/4)% of the time
            // (sub0835); a fall costs Int(Rnd x (F + 1) / 2 x 15.5) x ln(size + 2) + 1.
            for i in self.living() {
                let m = &mut self.party[i];
                if m.levitates(data) && m.rnd() * 100.0 > 5.0 + floor as f64 / 4.0 {
                    continue;
                }
                let dmg = ((m.rnd() * (floor as f64 + 1.0) / 2.0 * 15.5).floor() * size(m) + 1.0).round_ties_even() as i32;
                let (you, named) = (format!("{} {dmg}{}", data.t(0xc950_49a3), data.t(0x3563_88c5)), format!("{}{} {dmg}{}", self.quoted(i), data.t(0x8a5e_51a8), data.t(0x3563_88c5)));
                self.tell(i, SQUARE, you, Some(named));
                self.hurt(data, i, dmg);
            }
        }
        if flags & square::EXTINGUISH != 0 {
            for m in &mut self.party {
                m.buffs = Buffs::default();
            }
        }
        if self.living().is_empty() {
            return;
        }
        // A chute drops the party by its table's floors when the leader falls (the remake
        // keeps the party together); a levitating member is unhurt, the others take
        // Int(Rnd x new floor x 10.5) x ln(size + 2) + 5 (sub08F5).
        let drop = f.chutes.iter().find(|c| c.at == (x, y)).map_or(0, |c| c.drop.max(0) as usize);
        if flags & square::CHUTE != 0 && drop > 0 && !self.party[0].levitates(data) {
            let to = (floor + drop).min(data.floors.len());
            self.set_pos(data, Pos { floor: to, ..self.pos() });
            let fall: Vec<usize> = self.living().into_iter().filter(|&i| !self.party[i].levitates(data)).collect();
            for i in fall {
                let m = &mut self.party[i];
                let dmg = ((m.rnd() * to as f64 * 10.5).floor() * size(m) + 5.0).round_ties_even() as i32;
                self.hurt(data, i, dmg);
            }
            return;
        }
        if flags & square::TELEPORTER != 0 {
            let tp = f.teleporters.iter().find(|tp| tp.at == (x, y)).and_then(|tp| tp.to);
            let (tx, ty, tf) = match tp {
                Some((tx, ty, tf)) => (tx, ty, (tf as usize).clamp(1, data.floors.len())),
                None => self.random_square(data, floor),
            };
            self.set_pos(data, Pos { x: tx, y: ty, floor: tf, facing: self.pos().facing });
            return self.say(data.t(if self.party.len() == 1 { 0x6e7f_7df7 } else { 0xd5ce_1bdb }));
        }
        // A room's monsters meet the party as it comes in (sub0980).
        let was = (back.floor == floor).then(|| f.square(back.x, back.y).area);
        if area != 0 && was != Some(area) {
            self.enter_room(data, t, area, flags & square::STUD != 0, back);
        }
    }

    pub(crate) fn random_square(&mut self, data: &GameData, floor: usize) -> (usize, usize, usize) {
        let f = &data.floors[floor - 1];
        loop {
            let (x, y) = (self.party[0].roll(f.width as u32) as usize, self.party[0].roll(f.height as u32) as usize);
            if f.square(x, y).flags & square::ROCK == 0 {
                return (x, y, floor);
            }
        }
    }

    fn turn_to(&mut self, dir: u8) {
        for m in &mut self.party {
            m.pos.facing = dir;
        }
    }

    /// Takes damage; true if it killed that member. When the controlled character dies,
    /// control passes to the next living member; when all are dead, the fight ends.
    pub(crate) fn hurt(&mut self, data: &GameData, i: usize, dmg: i32) -> bool {
        let m = &mut self.party[i];
        if !m.alive() {
            return false;
        }
        m.hp -= dmg;
        if m.hp > 0 {
            return false;
        }
        m.hp = 0;
        m.life = Life::Dead(m.pos);
        m.buffs = Buffs::default();
        let name = format!("\"{}\"", m.name);
        self.say(format!("{name}{}", data.t(0x7acb_4bd0)));
        if self.living().is_empty() {
            self.shelve(data);
            self.say("The whole party has fallen. Your bodies lie where you fell.");
            for m in &mut self.party {
                m.pos = Pos::default();
            }
        } else if i == 0 {
            let next = self.living()[0];
            self.switch_to(next);
            self.say(format!("{}{}", self.quoted(0), data.t(0x4307_1d1b)));
        }
        true
    }

    pub fn take_stairs(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.floor(data) else { return };
        let p = self.pos();
        let flags = f.square(p.x, p.y).flags;
        if self.fight.is_some() {
            self.say(data.t(0x17ef_8df8));
        } else if flags & square::STAIRS_UP != 0 {
            if p.floor == 1 {
                self.to_town(data, t);
            } else {
                self.set_pos(data, Pos { floor: p.floor - 1, ..p });
            }
        } else if flags & square::STAIRS_DOWN != 0 && p.floor < data.floors.len() {
            self.set_pos(data, Pos { floor: p.floor + 1, ..p });
            self.arrive(data, t, p);
        } else if flags & square::CHUTE != 0 && f.chutes.iter().any(|c| c.at == (p.x, p.y) && c.drop > 0) {
            // Taking a chute drops by its table's floors (fn14CB, key T).
            let drop = f.chutes.iter().find(|c| c.at == (p.x, p.y)).map_or(1, |c| c.drop.max(1) as usize);
            self.set_pos(data, Pos { floor: (p.floor + drop).min(data.floors.len()), ..p });
        } else {
            self.say("There are no stairs here.");
        }
    }

    /// The Seer (MORDOR.EXE's fees): where a party member lies, for
    /// 132,567.52 x ln(F + 1) x F on floor F; or where a monster keeps its lair, for
    /// half of that at the monster's level, give or take an eighth (up to level 12).
    pub fn seer_cost(&mut self, data: &GameData, name: &str) -> Option<u64> {
        let fee = |f: f64| 132_567.52 * (f + 1.0).ln() * f;
        let name = name.trim().to_lowercase();
        if let Some(m) = self.party.iter().find(|m| m.name.to_lowercase() == name) {
            return Some(match m.life {
                Life::Dead(p) => fee(p.floor as f64).round_ties_even() as u64,
                _ => 0,
            });
        }
        let m = data.monsters.iter().find(|m| m.name.to_lowercase() == name)?;
        (m.level <= 12).then(|| {
            let c = fee(f64::from(m.level.max(1)));
            ((c + self.party[0].rnd() * c / 4.0) / 2.0).round_ties_even() as u64
        })
    }

    /// The Seer's answer, a message box in the original's words (FRMSEEK): its title's
    /// text id and its text.
    pub fn seer(&mut self, data: &GameData, name: &str) -> (u32, String) {
        const SORRY: u32 = 0x6255_8b51;
        const SEER: u32 = 0xde72_1ed6;
        let at = |x: usize, y: usize, f: i64| format!("{},{},{f}.", x + 1, y + 1);
        let key = name.trim().to_lowercase();
        let member = self.party.iter().any(|m| m.name.to_lowercase() == key);
        let monster = data.monsters.iter().position(|m| m.name.to_lowercase() == key);
        let Some(cost) = self.seer_cost(data, name) else {
            return match monster {
                Some(_) => (SORRY, data.t(0x243e_9ebd).into()),
                None => (SORRY, format!("{}{}{}", data.t(0xf8d3_b5cc), name.trim(), data.t(0x81be_ab38))),
            };
        };
        let lead = &mut self.party[0];
        if lead.gold + lead.bank < cost {
            return (if member { 0xf803_76b8 } else { 0xeabe_3cb9 }, data.t(0x22bd_be43).into());
        }
        lead.pay(cost);
        if let Some(m) = self.party.iter().find(|m| m.name.to_lowercase() == key) {
            let found = |p: Pos| format!("{}\"{}\"{}{}", data.t(0x68c0_5808), m.name, data.t(0x349a_42d0), at(p.x, p.y, p.floor as i64));
            return match m.life {
                Life::Dead(p) => (SEER, format!("{}\n{}", found(p), data.t(0xf406_90d9))),
                _ if m.pos.floor == 0 => (SEER, format!("{}\n{}", data.t(0xb448_8b9d), data.t(0x67ae_99cd))),
                _ => (SEER, found(m.pos)),
            };
        }
        let m = monster.expect("a monster when not a member");
        let id = data.monsters[m].id;
        let lair = data.floors.iter().find_map(|f| {
            let a = f.areas.iter().position(|a| a.lair == id)?;
            let i = f.squares.iter().position(|sq| sq.area as usize == a)?;
            Some(at(i % f.width, i / f.width, i64::from(f.level)))
        });
        match lair {
            Some(at) => (0xeabe_3cb9, format!("{}{}{}{at}", data.t(0x0129_16e9), data.monsters[m].name, data.t(0x9b15_be84))),
            None => (0xeabe_3cb9, format!("{}{}{}\n{}", data.t(0x11bb_b661), data.monsters[m].level, data.t(0x9198_fbe6), data.t(0x2ab1_633e))),
        }
    }

    /// The store's stock, starting it fresh if none was loaded.
    pub fn stock(&mut self, data: &GameData) -> &mut Store {
        if self.store.items.len() != data.items.len() {
            self.store = Store::fresh(data);
        }
        &mut self.store
    }

    /// Member `i`'s price for an item of alignment `al` (CHA's passive included).
    pub fn buy_price(&mut self, data: &GameData, i: usize, item: usize, al: u8) -> u64 {
        let s = self.stock(data).count(item, al);
        store::cha_buy(&self.party[i], data, store::buy_price(data, item, al, s))
    }

    pub fn sell_price(&mut self, data: &GameData, i: usize, slot: usize) -> u64 {
        let h = self.party[i].items[slot].clone();
        let s = self.stock(data).sell_count(&h);
        store::cha_sell(&self.party[i], data, store::sell_price(data, &h, s))
    }

    pub fn identify_price(&mut self, data: &GameData, i: usize, slot: usize) -> u64 {
        let h = self.party[i].items[slot].clone();
        let s = self.stock(data).count(h.item, h.align);
        store::identify_price(data, &h, s)
    }

    /// Realigning a pack item to alignment `to` (1-3) costs 0.72 P, twice that across
    /// two steps; never a worn one (cmdRealign_Click).
    pub fn realign(&mut self, data: &GameData, i: usize, slot: usize, to: u8) -> Result<String, (u32, String)> {
        const TITLE: u32 = 0x4c34_6180;
        let c = &mut self.party[i];
        let h = &c.items[slot];
        if h.equipped {
            return Err((0x9fa0_af51, data.t(0x5dab_1ecb).into()));
        }
        let cost = (0.72 * f64::from(data.items[h.item].price) * if h.align.abs_diff(to) > 1 { 2.0 } else { 1.0 }).round_ties_even() as u64;
        if c.gold + c.bank < cost {
            return Err((TITLE, data.t(0x22bd_be43).into()));
        }
        c.pay(cost);
        c.items[slot].align = to;
        Ok(String::new())
    }

    /// The store's services (FRMCTYS) answer in the original's words: Ok is the
    /// Messages line (often none), Err a message box (title's text id, text).
    pub fn buy(&mut self, data: &GameData, i: usize, item: usize, al: u8) -> Result<String, (u32, String)> {
        const BUY: u32 = 0xf158_f0aa;
        let price = self.buy_price(data, i, item, al);
        if self.store.count(item, al) == 0 {
            return Err((BUY, "The store has none left.".into()));
        }
        let c = &mut self.party[i];
        if c.gold + c.bank < price {
            return Err((BUY, data.t(0x22bd_be43).into()));
        }
        if c.items.len() >= 40 {
            return Err((BUY, data.t(0x6ac7_6f90).into()));
        }
        c.pay(price);
        c.items.push(Held { align: al, ..crate::magic::new_held(data, item, true) });
        self.store.take(item, al);
        Ok(format!("{}{}{}{}.", data.t(0x4a29_b680), c.items.len(), data.t(0x7dba_5663), data.items[item].name))
    }

    pub fn sell(&mut self, data: &GameData, i: usize, slot: usize) -> String {
        let it = &data.items[self.party[i].items[slot].item];
        if self.party[i].items[slot].item == HANDS {
            return "The store has no use for your hands.".into();
        }
        if (500..512).contains(&it.w(10)) {
            return data.t(0xdb51_2bd8).into();
        }
        if self.party[i].items[slot].equipped && crate::magic::cursed(data, &self.party[i].items[slot], self.party[i].alignment) {
            return data.t(0x0997_b513).into();
        }
        let price = self.sell_price(data, i, slot);
        let h = self.party[i].items.remove(slot);
        self.party[i].gold += price;
        self.store.put(data, &h);
        String::new()
    }

    pub fn identify(&mut self, data: &GameData, i: usize, slot: usize) -> Result<String, (u32, String)> {
        let cost = self.identify_price(data, i, slot);
        let c = &mut self.party[i];
        if c.gold + c.bank < cost {
            return Err((0x4a17_88e0, data.t(0x22bd_be43).into()));
        }
        c.pay(cost);
        // One step of identification (cmdID_Click).
        c.items[slot].id = c.items[slot].id.saturating_sub(1);
        Ok(String::new())
    }

    /// Uncursing an equipped item costs 1.6 x its price, takes it off and identifies it;
    /// the item stays cursed (MORDOR.EXE).
    pub fn uncurse(&mut self, data: &GameData, i: usize, slot: usize) -> Result<String, (u32, String)> {
        let c = &mut self.party[i];
        let item = c.items[slot].item;
        let cost = store::uncurse_price(data, item);
        if c.gold + c.bank < cost {
            return Err((0xe52c_729b, data.t(0x22bd_be43).into()));
        }
        c.pay(cost);
        c.items[slot].equipped = false;
        c.items[slot].id = 0;
        Ok(data.t(0x4549_b450).into())
    }

    /// Rescuers fetch a corpse from floor F for 372,567.52 x ln(F + 1) x F (MORDOR.EXE);
    /// they are lost, and keep the fee, 4% of the time per floor.
    pub fn rescue_cost(&self, i: usize) -> u64 {
        match self.party[i].life {
            Life::Dead(p) => (372_567.52 * (p.floor as f64 + 1.0).ln() * p.floor as f64).round_ties_even() as u64,
            _ => 0,
        }
    }

    /// Raising costs 2,912.32 x max hits / 1.5 (MORDOR.EXE).
    pub fn raise_cost(&self, i: usize) -> u64 {
        (2_912.32 * f64::from(self.party[i].max_hp) / 1.5).round_ties_even() as u64
    }

    /// The party's gold, purses and banks.
    fn funds(&self) -> u64 {
        self.party.iter().map(|m| m.gold + m.bank).sum()
    }

    /// The fee comes from the dead character's purse and bank, then the party's; false,
    /// with the original's "Not enough gold!", when they can't cover it (fn114E).
    fn charge(&mut self, data: &GameData, i: usize, cost: u64) -> bool {
        if self.funds() < cost {
            self.boxes.push((0x9fa0_af51, data.t(0xab5e_eac3).into()));
            return false;
        }
        let mut left = cost;
        for j in std::iter::once(i).chain(0..self.party.len()) {
            let m = &mut self.party[j];
            let take = left.min(m.gold + m.bank);
            m.pay(take);
            left -= take;
        }
        true
    }

    /// FRMMORG's rescue, told in the original's words.
    pub fn rescue(&mut self, data: &GameData, i: usize) {
        let Life::Dead(p) = self.party[i].life else { return };
        if !self.charge(data, i, self.rescue_cost(i)) {
            return;
        }
        if self.party[i].rnd() * 100.0 <= 4.0 * p.floor as f64 {
            return self.say(data.t(0x6601_942a));
        }
        self.party[i].life = Life::InMorgue;
        self.party[i].pos = Pos::default();
        let found = format!("{}{} {},{},{}{}", self.party[i].name, data.t(0xf7c5_95c1), p.x + 1, p.y + 1, p.floor, data.t(0xf8d3_d331));
        self.say(found);
    }

    /// The morgue's raise (MAINMOD2.mdrcharIsDead): paid for, or free while the best guild
    /// level is 5 or less and the gold falls short. It works when Int(Rnd x 100) is no
    /// more than the odds (fn13D7, the window's +10). A failure may bring complications,
    /// which raise anyway (fn11A1); a raise ages 54 days and costs a point of CON
    /// (50 - 2 CON)% of the time.
    pub fn raise(&mut self, data: &GameData, i: usize) {
        if self.party[i].life != Life::InMorgue {
            return;
        }
        const INFO: u32 = 0xbe7b_2721;
        let cost = self.raise_cost(i);
        if self.party[i].levels.iter().max().copied().unwrap_or(0) <= 5 && self.funds() < cost {
            self.boxes.push((0xad54_2324, data.t(0x15ee_90dd).into()));
        } else if !self.charge(data, i, cost) {
            return;
        }
        let m = &mut self.party[i];
        let said = if (m.rnd() * 100.0).floor() as i32 > raise_odds(m, 1) {
            match complications(data, m) {
                Some(id) => vec![0x8327_8e1b, id],
                None => return self.boxes.push((INFO, data.t(0x8327_8e1b).into())),
            }
        } else {
            vec![0xf63e_9d43]
        };
        m.age_days += 54;
        m.life = Life::Alive;
        m.hp = m.max_hp;
        let min = data.races[m.race].stat_min[CON];
        let con = m.rnd() * 100.0 < 50.0 - 2.0 * f64::from(m.stats[CON]) && m.stats[CON] > min;
        if con {
            m.stats[CON] -= 1;
        }
        self.boxes.extend(said.into_iter().map(|id| (INFO, data.t(id).into())));
        if con {
            let line = if self.party.len() == 1 { data.t(0x346e_365c).into() } else { format!("{}{}{}", data.t(0x2cf6_254e), self.quoted(i), data.t(0x02e1_4e6f)) };
            self.say(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai::Act,
        magic::{self, Magic},
    };

    fn data() -> GameData {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = root.join("target/ddj-game-test");
        ddj_extract::import(&root.join("tests/fixtures/MORDOR11.ZIP"), &out).unwrap();
        GameData::load(&out).unwrap()
    }

    fn kit(d: &GameData, c: &mut Character) {
        for n in ["Bronze Sword", "Leather Armor", "Leather Gloves", "Leather Boots", "Cloth Cloak", "Wooden Shield", "Leather Cap"] {
            let item = d.items.iter().position(|i| i.name == n).unwrap();
            c.items.push(magic::new_held(d, item, true));
            let slot = c.items.len() - 1;
            let _ = c.toggle_equip(d, slot);
        }
    }

    /// A character of `guild` at `level`, with the standard kit.
    fn hero(d: &GameData, name: &str, race: usize, align: u8, guild: usize, level: u32) -> Character {
        let base = start_stats(d, race);
        let stats = std::array::from_fn(|i| base[i].max(d.guilds[guild].stat_req[i]).min(d.races[race].stat_max[i]));
        let mut c = Character::new(d, name, race, false, align, base).unwrap_or_else(|e| panic!("{name}: {e}"));
        c.stats = stats;
        c.gold = 100_000;
        if guild != NOMAD {
            c.join(d, guild).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        c.levels[guild] = level;
        c.macros = ai::preset(ai::Role::of(guild), |k| magic::best_known(&c, d, k));
        c.max_hp += 6 * level as i32;
        c.hp = c.max_hp;
        kit(d, &mut c);
        c
    }

    /// A fighter, a healer and a mage explore floor 1 with the AI running everyone.
    #[test]
    fn party_plays_floor_one() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut best = 0;
        let mut cast = 0;
        for seed in ["A", "B", "C"] {
            let mut g = Game::new(hero(&d, &format!("Fighter{seed}"), 0, 1, 1, 4));
            g.add_ally(&d, hero(&d, &format!("Healer{seed}"), 1, 1, HEALER, 4)).unwrap();
            g.add_ally(&d, hero(&d, &format!("Mage{seed}"), 1, 0, 8, 4)).unwrap();
            for m in &mut g.party {
                m.sp = m.max_sp(&d, &t);
            }
            g.enter_dungeon(&d).unwrap();
            let mut steps = 0;
            while !g.living().is_empty() && steps < 3000 {
                steps += 1;
                if g.fight.is_some() {
                    g.round(&d, &t);
                } else if g.log.last().is_some_and(|l| l.starts_with("A wall")) || g.party[0].roll(8) == 0 {
                    let by = 1 + 2 * g.party[0].roll(2) as u8;
                    g.turn(by);
                    g.log.push(String::new());
                } else {
                    g.step(&d, &t);
                }
            }
            let kills: u32 = g.party.iter().map(|m| m.kills).sum();
            let heals = g.log.iter().filter(|l| l.contains(" heals ")).count();
            let spells = g.log.iter().filter(|l| l.contains(" casts ")).count();
            eprintln!("{seed}: {steps} steps, {kills} kills, {spells} spells ({heals} heals), living {}", g.living().len());
            best = best.max(kills);
            cast += spells;
        }
        assert!(cast > 0, "the AI never cast a spell");
        assert!(best >= 5);
    }

    #[test]
    fn spells_follow_guild_rates_and_schools() {
        let d = data();
        // MORDOR.EXE: CInt(level x rate) <= Int(L / 2 + 0.5), so Paladins learn Minor Heal
        // (level 2, rate 3) at guild level 11; it was 6 before the audit's port.
        let paladin = hero(&d, "Pal", 0, 0, 2, 11);
        let minor = d.spells.iter().position(|s| s.name == "Minor Heal").unwrap();
        assert!(magic::known(&paladin, &d).contains(&minor));
        assert_eq!(magic::power(&paladin, &d, minor), 6.0);
        let early = hero(&d, "Pal10", 0, 0, 2, 10);
        assert!(!magic::known(&early, &d).contains(&minor));
        // fn142F: Int(6 / 6 x 13 SP x 3 x 1.9) = 74 when new.
        assert_eq!(magic::sp_cost(&paladin, &d, minor), 74);
        // The class cap (w71 + 2c): Paladins never learn Resurrect, nobody but casters
        // learns Depth Perception.
        let res = d.spells.iter().position(|s| s.name == "Resurrect").unwrap();
        assert!(magic::taught_at(&d, 2, res).is_none());
        // Wizards give up the mind spells (docs/balance.md).
        let sleep = d.spells.iter().position(|s| s.name == "Sleep").unwrap();
        assert!(magic::taught_at(&d, 10, sleep).is_none() && magic::taught_at(&d, 8, sleep).is_some());
        // Charm of Experience: Seekers at spell level 16, guild level 31.
        let charm = d.spells.iter().position(|s| s.id == magic::CHARM_OF_EXPERIENCE).unwrap();
        assert_eq!((magic::taught_at(&d, 5, charm), magic::taught_at(&d, 8, charm)), (Some(31), None));
        // fn0E5F: Firebolt (10/0) at spell level 1 does 6-13.
        let bolt = &d.spells[d.spells.iter().position(|s| s.name == "Firebolt").unwrap()];
        let mut c = hero(&d, "Roll", 0, 0, 8, 1);
        let rolls: Vec<f64> = (0..2000).map(|_| magic::hits(&mut c, bolt, 1.0, false)).collect();
        assert!(rolls.iter().all(|&r| (6.0..=13.0).contains(&r)), "{:?}", rolls.iter().fold((99.0f64, 0.0f64), |a, &r| (a.0.min(r), a.1.max(r))));
    }

    #[test]
    fn experience_potion_boosts_until_floor_change() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Quaffer", 0, 1, 0, 1));
        let potion = d.items.iter().position(|i| i.id == magic::POTION_OF_EXPERIENCE).unwrap();
        assert_eq!(g.stock(&d).count(potion, 0), crate::store::UNLIMITED);
        g.buy(&d, 0, potion, 0).unwrap();
        g.enter_dungeon(&d).unwrap();
        magic::use_item(&mut g, &d, &t, 0, potion);
        assert_eq!(g.party[0].add_xp(&d, &t, 10), 15);
        let p = g.pos();
        g.set_pos(&d, Pos { floor: 2, ..p });
        assert_eq!(g.party[0].add_xp(&d, &t, 10), 10);
        assert!(g.party[0].items.iter().all(|h| h.item != potion));
        // Using a sword by mistake keeps it.
        let sword = d.items.iter().position(|i| i.name == "Bronze Sword").unwrap();
        g.party[0].items.push(magic::new_held(&d, sword, true));
        magic::use_item(&mut g, &d, &t, 0, sword);
        assert!(g.party[0].items.iter().any(|h| h.item == sword));
    }

    #[test]
    fn healer_ai_heals_the_wounded() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Tank", 0, 1, 1, 6));
        g.add_ally(&d, hero(&d, "Medic", 1, 1, HEALER, 6)).unwrap();
        g.party[1].sp = 500;
        g.enter_dungeon(&d).unwrap();
        let kobold = d.monsters.iter().position(|m| m.name == "Kobold").unwrap();
        let foes = vec![crate::combat::Foe { monster: kobold, hp: 50, max_hp: 50, group: 0 }];
        g.fight = Some(crate::combat::Fight { foes, round: 0, order: Some(Act::Defend), back: g.pos(), lair: false, peaceful: false, chest: None, defending: vec![false; 2], gold: 0, opening: None, offers: vec![0] });
        g.party[0].hp = 3;
        // MORDOR.EXE: no heals while hostile monsters are about (fn1448).
        let act = crate::ai::decide(&g, &d, &t, 1);
        assert!(!matches!(act, Act::Cast(s, _) if magic::kind(&d.spells[s]) == Some(Magic::Heal)), "{act:?}");
        // Between fights the Healer mends the Tank.
        g.fight = None;
        crate::ai::tend(&mut g, &d, &t);
        assert!(g.party[0].hp > 3, "{}", g.party[0].hp);
    }

    #[test]
    fn dies_is_rescued_raised_and_saved() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(Character::new(&d, "Raise", 0, true, 1, start_stats(&d, 0)).unwrap());
        g.party[0].add_xp(&d, &t, 10_000);
        assert_eq!(g.party[0].xp[0], t.xp_needed(2, 3.0, 8.0) - 1, "pinned below level 3");
        g.party[0].make_level(&d, &t).unwrap();
        assert_eq!(g.party[0].level(), 2);
        g.enter_dungeon(&d).unwrap();
        let at = g.pos();
        assert!(g.hurt(&d, 0, 1000));
        assert_eq!(g.party[0].life, Life::Dead(at));
        // Bounded: a refused fee must never spin a test (it once filled the owner's RAM).
        for _ in 0..1000 {
            if g.party[0].life == Life::InMorgue {
                break;
            }
            g.party[0].gold = 10_000_000;
            g.rescue(&d, 0);
        }
        assert_eq!(g.party[0].life, Life::InMorgue);
        for _ in 0..1000 {
            if g.party[0].life == Life::Alive {
                break;
            }
            g.party[0].gold = 10_000_000;
            g.raise(&d, 0);
        }
        assert_eq!(g.party[0].life, Life::Alive);
        g.boxes.clear();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-save-test");
        g.party[0].save(&dir).unwrap();
        let back = Character::load(&dir.join("Raise.json")).unwrap();
        assert_eq!((back.levels, back.gold, back.stats), (g.party[0].levels.clone(), g.party[0].gold, g.party[0].stats));
        assert!(saved(&dir).contains(&"Raise".to_string()));
    }

    /// MORDOR.EXE's morgue: no rescue or raise without the gold, except a free raise up
    /// to best level 5; the odds are fn13D7's.
    #[test]
    fn morgue_needs_gold() {
        let d = data();
        let mut g = Game::new(hero(&d, "Broke", 0, 1, 1, 9));
        g.enter_dungeon(&d).unwrap();
        g.hurt(&d, 0, 10_000);
        (g.party[0].gold, g.party[0].bank) = (0, 0);
        g.rescue(&d, 0);
        assert!(matches!(g.party[0].life, Life::Dead(_)), "no rescue without the gold");
        g.party[0].life = Life::InMorgue;
        g.raise(&d, 0);
        assert_eq!(g.party[0].life, Life::InMorgue, "level 9 pays");
        g.party[0].levels = vec![0; d.guilds.len()];
        g.party[0].levels[1] = 5;
        for _ in 0..1000 {
            if g.party[0].life == Life::Alive {
                break;
            }
            g.raise(&d, 0);
        }
        assert_eq!(g.party[0].life, Life::Alive, "free raise at best level 5");
        assert!(g.party[0].age_days >= 16 * 365 + 54);
        let c = &mut g.party[0];
        c.stats[CON] = 12;
        assert_eq!(raise_odds(c, 1), 98);
        c.stats[CON] = 3;
        assert_eq!((raise_odds(c, 1), raise_odds(c, 2)), (33, 43));
    }

    /// MONGEN (fn0D4D): linked groups by w4, half to all of w32 each, nothing of spawn
    /// weight 0 outside a lair; a room keeps its wounded monsters (sub0A91).
    #[test]
    fn encounters_as_the_original() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Scout", 0, 1, 1, 5));
        g.enter_dungeon(&d).unwrap();
        let f = &d.floors[0];
        let rooms: Vec<usize> = (18..f.areas.len()).filter(|&k| f.areas[k].monster_types != 0 && f.areas[k].lair == 0).collect();
        let mut multi = 0;
        for &k in &rooms {
            for _ in 0..20 {
                let groups = g.mongen(&d, &f.areas[k], k as i16, false);
                let Some(&(first, _)) = groups.first() else { continue };
                multi += usize::from(groups.len() > 1);
                assert!(groups.len() <= 4 && d.monsters[first].weight > 0, "{groups:?}");
                for &(m, n) in &groups {
                    let w = d.monsters[m].w(32).max(1) as u32;
                    assert!((w / 2..=w).contains(&n) || n == 1, "{} x{n} of {w}", d.monsters[m].name);
                }
            }
        }
        assert!(multi > 0, "no encounter had followers");
        let area = rooms[0];
        let i = f.squares.iter().position(|s| s.area as usize == area).unwrap();
        g.set_pos(&d, Pos { x: i % f.width, y: i / f.width, floor: 1, facing: 0 });
        for _ in 0..50 {
            g.rooms.clear();
            g.enter_room(&d, &t, area as i16, false, g.pos());
            if g.fight.is_some() && g.living().len() == 1 {
                break;
            }
        }
        let Some(fight) = g.fight.as_mut() else { panic!("no encounter in 50 tries") };
        fight.foes[0].hp = 1;
        g.shelve(&d);
        g.enter_room(&d, &t, area as i16, false, g.pos());
        assert_eq!(g.fight.as_ref().map(|f| f.foes[0].hp), Some(1), "the wounded monster is still there");
    }

    /// Water takes F hits a tick from each member not levitating; a pit up to
    /// (F + 1) / 2 x 15.5 x ln(size + 2) + 1 (timAilment_Timer, sub0835).
    #[test]
    fn squares_as_the_original() {
        let d = data();
        let mut g = Game::new(hero(&d, "Wader", 0, 1, 1, 9));
        g.enter_dungeon(&d).unwrap();
        for (bit, test) in [(square::WATER, 0), (square::PIT, 1)] {
            let Some((fl, i)) = d.floors.iter().enumerate().find_map(|(k, f)| f.squares.iter().position(|s| s.flags & bit != 0).map(|i| (k, i))) else { continue };
            let f = &d.floors[fl];
            g.party[0].hp = 10_000;
            g.party[0].max_hp = 10_000;
            g.set_pos(&d, Pos { x: i % f.width, y: i / f.width, floor: fl + 1, facing: 0 });
            if test == 0 {
                g.ailments(&d);
                assert_eq!(g.party[0].hp, 10_000 - (fl as i32 + 1), "water takes the floor's number");
            } else {
                let back = g.pos();
                g.arrive(&d, &Tuning::load(&[]).unwrap(), back);
                let max = ((fl as f64 + 2.0) / 2.0 * 15.5 * (f64::from(d.races[0].w(28)) + 2.0).ln() + 1.0) as i32 + 1;
                assert!((1..=max).contains(&(10_000 - g.party[0].hp)), "{}", 10_000 - g.party[0].hp);
            }
        }
    }

    /// cmdMake_Click's quests (CInt(L / 25) as the level found, forfeit when pinned) and
    /// natural death past 98% of the lifespan (timAilment_Timer).
    #[test]
    fn quests_and_old_age() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut c = hero(&d, "Quester", 0, 1, 1, 50);
        for _ in 0..50 {
            let m = c.quest_pick_monster(&d, 2).unwrap();
            assert!(d.monsters[m].level <= 2, "{}", d.monsters[m].name);
        }
        c.quest = Some(0);
        c.xp[1] = t.xp_needed(51, d.races[0].xp_factor, d.guilds[1].xp_factor) - 1;
        assert!(c.may_forfeit(&d, &t) && c.make_level(&d, &t).is_err());
        c.forfeit(&d, &t).unwrap();
        assert_eq!((c.levels[1], c.quest), (47, None), "ln 51 = 3.9 levels lost");
        let mut g = Game::new(c);
        g.enter_dungeon(&d).unwrap();
        g.party[0].age_days = 365 * 100;
        for _ in 0..2000 {
            if !g.party[0].alive() {
                break;
            }
            g.ailments(&d);
        }
        assert!(!g.party[0].alive(), "a Human of 100 dies of old age");
    }

    /// sub0F74 and fn110D: two hands, one item per class, worn stat modifiers; wearable
    /// spell items stay at 0 charges; identification levels and their prices.
    #[test]
    fn items_as_the_original() {
        let d = data();
        let item = |n: &str| d.items.iter().position(|i| i.name == n).unwrap();
        let mut c = hero(&d, "Kit", 0, 1, 1, 5);
        c.stats[STR] = 14;
        c.stats[DEX] = 14;
        let add = |c: &mut Character, n: &str| {
            c.items.push(magic::new_held(&d, item(n), true));
            c.items.len() - 1
        };
        let axe = add(&mut c, "Bronze Battle Axe");
        assert_eq!(c.toggle_equip(&d, axe), Err(d.t(0xeda8_6a59).into()), "sword and shield fill both hands");
        let chain = add(&mut c, "Bronze Chain Mail");
        assert_eq!(c.toggle_equip(&d, chain), Err(d.t(0xb9a9_fca7).into()), "leather is worn already");
        let leather = c.items.iter().position(|h| h.item == item("Leather Armor")).unwrap();
        c.toggle_equip(&d, leather).unwrap();
        c.toggle_equip(&d, chain).unwrap();
        assert_eq!(c.stat(&d, DEX), 13, "chain mail costs a point of DEX");
        assert!(!magic::consumable(&d, item("Crest of the Warrior")) && magic::consumable(&d, item("Potion of Might")));
        let mut h = magic::new_held(&d, item("Bronze Chain Mail"), false);
        h.id = 2;
        let (p, full) = (crate::store::identify_price(&d, &h, 0), crate::store::sell_price(&d, &h, 0));
        h.id = 0;
        assert!(crate::store::sell_price(&d, &h, 0) > 2 * full && p > 0 && crate::store::identify_price(&d, &h, 0) == 0);
        // Alignment: an Evil wearer of a Good sword gets its A/D negated and can't take
        // it off; a Neutral one gets half (sub0F74). The store keeps one per alignment.
        let sword = item("Iron Sword");
        let s = crate::store::Store::fresh(&d);
        assert_eq!((s.count(sword, 0), s.count(sword, 1), s.count(sword, 3)), (0, 1, 1));
        let mut evil = hero(&d, "Evil", 0, 2, 1, 20);
        let bronze = evil.items.iter().position(|h| h.item == item("Bronze Sword")).unwrap();
        evil.toggle_equip(&d, bronze).unwrap();
        let bare = evil.att_def(&d, &Tuning::load(&[]).unwrap()).0;
        evil.items.push(Held { align: 1, ..magic::new_held(&d, sword, true) });
        let k = evil.items.len() - 1;
        evil.toggle_equip(&d, k).unwrap();
        let worn = evil.att_def(&d, &Tuning::load(&[]).unwrap()).0;
        assert_eq!(worn - bare, -i32::from(d.items[sword].attack));
        assert!(evil.toggle_equip(&d, k).is_err(), "an opposite item sticks");
    }

    /// timAilment_Timer's paralysis: strength = power level, the stay roll, gone with no
    /// monsters; the stat bonus is lost meanwhile (sub0D1D).
    #[test]
    fn paralysis_as_the_original() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Still", 0, 1, 1, 5));
        let c = &mut g.party[0];
        c.stats = [18, 10, 10, 15, 10, 17];
        let free = c.att_def(&d, &t);
        c.status.paralysis = 15;
        let res = f64::from(crate::monsters::resist(c, &d, 8));
        assert!((paralysis_stay(&d, c) - (84.12 - res).clamp(5.0, 90.0)).abs() < 0.05, "{}", paralysis_stay(&d, c));
        let held = c.att_def(&d, &t);
        assert!(held.0 < free.0 && held.1 < free.1);
        g.ailments(&d);
        assert_eq!(g.party[0].status.paralysis, 0, "no monsters, no paralysis");
    }

    #[test]
    fn chests_trap_and_pay_out() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Picker", 0, 1, 6, 10));
        g.enter_dungeon(&d).unwrap();
        let before = g.party[0].gold;
        let kobold = d.monsters.iter().position(|m| m.name == "Kobold").unwrap();
        for _ in 0..20 {
            let trap = crate::treasure::Trap::PoisonGas;
            g.chest = Some(crate::treasure::Chest { chest: true, trap, guess: trap, sure: 2, magic_lock: false, unlocked: false, monster: kobold, level: 1, gold: 1000 });
            crate::treasure::open(&mut g, &d, &t, 0);
            g.party[0].hp = g.party[0].max_hp;
        }
        assert!(g.party[0].gold > before && g.chest.is_none());
        // Some traps were disarmed, not all set off (the label names a sprung trap).
        assert!(g.log.iter().filter(|l| *l == d.t(0x4bf7_7b8c)).count() < 20);
    }

    /// The Hall starts with the original's holders; a better value takes a record and is
    /// announced once, a stronger creature takes the creature record, and it all keeps.
    /// As the original's Create Character window showed them (Human, then Dwarf).
    #[test]
    fn creation_points() {
        let d = data();
        assert_eq!((start_stats(&d, 0), d.races[0].stat_points), ([9, 9, 9, 11, 10, 11], 5));
        assert_eq!((start_stats(&d, 4), d.races[4].stat_points), ([8, 8, 12, 8, 8, 10], 10));
        // Strength down to its minimum pays for 5 more points; one past the pool is refused.
        let mut s = start_stats(&d, 4);
        (s[0], s[1], s[2]) = (3, 18, 17);
        assert!(Character::new(&d, "Dwarf", 4, false, 1, s).is_ok());
        s[2] = 18;
        assert!(Character::new(&d, "Dwarf", 4, false, 1, s).is_err());
    }

    #[test]
    fn hall_of_records() {
        let d = data();
        let mut g = Game::new(Character::new(&d, "Rec", 0, false, 1, start_stats(&d, 0)).unwrap());
        g.hall.today = "11/4/2026".into();
        assert!(!g.records(&d), "a new character beats no record");
        g.party[0].stats[0] = 15;
        g.party[0].bank = 6_000_000;
        assert!(g.records(&d));
        let set = |what| format!("{}{}!", d.t(0xfb4a_3a19), d.t(what));
        assert_eq!(g.log, [set(0x7c15_6b09), set(0xf3d2_0bbe)]);
        assert!(!g.records(&d), "announced once");
        let strong = d.monsters.iter().max_by_key(|m| crate::hall::might(m)).unwrap().id;
        g.hall.check(&d, crate::hall::CREATURE, "Rec", i64::from(strong));
        assert_eq!(g.hall.records[crate::hall::CREATURE].value, i64::from(strong));
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-hall-test");
        g.hall.save(&dir).unwrap();
        let back = Hall::load(&dir, String::new());
        assert_eq!((back.records[0].name.as_str(), back.records[0].value, back.records[0].date.as_deref()), ("Rec", 15, Some("11/4/2026")));
        assert_eq!(back.holder(&d, 1), d.t(0xc242_0a43));
        // Cloud saves: another device's hall gives only the records it holds higher.
        let (mut mine, mut other) = (back, Hall::default());
        other.records[1].name = "Far".into();
        other.records[1].value = 99;
        mine.merge(&d, other);
        assert_eq!(mine.records.iter().take(2).map(|r| (r.name.as_str(), r.value)).collect::<Vec<_>>(), [("Rec", 15), ("Far", 99)]);
        assert_eq!(mine.records[crate::hall::CREATURE].value, i64::from(strong));
    }

    /// The guilds speak the original's words, and a character past a guild's
    /// Guildmaster (the original's at first) takes over, with the guild's Crest.
    #[test]
    fn guilds_and_guildmasters() {
        let d = data();
        let mut g = Game::new(Character::new(&d, "Gm", 0, true, 1, start_stats(&d, 0)).unwrap());
        assert_eq!(g.party[0].welcome(&d, NOMAD), (format!("\n{}", d.t(0xd1c1_812b)), false));
        assert!(g.party[0].welcome(&d, 3).1, "a new Human can't join the Ninjas");
        let (name, level) = g.hall.guildmaster(&d, NOMAD);
        assert_eq!((name.as_str(), level), (d.t(0x0f17_479d), 64));
        g.party[0].levels[NOMAD] = 64;
        assert!(g.guildmaster(&d, 0).is_empty(), "level 64 doesn't pass a level 64 Guildmaster");
        g.party[0].levels[NOMAD] = 65;
        let said = g.guildmaster(&d, 0);
        assert_eq!(said.iter().map(|s| s.0).collect::<Vec<_>>(), [0x73d1_9d26, CONGRATULATIONS]);
        assert_eq!(d.items[g.party[0].items.last().unwrap().item].id, 299);
        assert_eq!(g.hall.guildmaster(&d, NOMAD), ("Gm".into(), 65));
        g.party[0].levels[NOMAD] = 66;
        assert!(g.guildmaster(&d, 0).is_empty(), "no second Crest or greeting");
        assert_eq!(g.hall.guildmaster(&d, NOMAD).1, 66);
    }

    /// Everyone carries Hands in the first slot: the weapon until another is wielded,
    /// back when it is put away, and never sold.
    #[test]
    fn hands() {
        let d = data();
        let mut c = Character::new(&d, "Bare", 0, false, 1, start_stats(&d, 0)).unwrap();
        assert_eq!((c.items[0].item, c.items[0].equipped, c.weapon(&d)), (HANDS, true, Some(HANDS)));
        let sword = d.items.iter().position(|i| i.name == "Bronze Sword").unwrap();
        c.items.push(magic::new_held(&d, sword, true));
        c.toggle_equip(&d, 1).unwrap();
        assert_eq!((c.items[0].equipped, c.weapon(&d)), (false, Some(sword)));
        c.toggle_equip(&d, 1).unwrap();
        assert_eq!((c.items[0].equipped, c.weapon(&d)), (true, Some(HANDS)));
        assert!(c.toggle_equip(&d, 0).is_err());
        let mut g = Game::new(c);
        g.sell(&d, 0, 0);
        assert_eq!(g.party[0].items[0].item, HANDS);
    }

    /// MORDOR.EXE's numbers: the hit roll, A/D, swings, gold and prices.
    #[test]
    fn original_rules() {
        let d = data();
        let mut c = hero(&d, "Roller", 0, 1, 0, 1);
        // Attack 20 against defense 10: 75%, and a swing lands when d100 < 75.
        let hits = (0..20_000).filter(|_| crate::combat::swing(&mut c, 20, 10, 10.0, 15, 1.0) > 0).count();
        assert!((hits as f64 / 20_000.0 - 0.74).abs() < 0.02, "{hits}");
        assert!((0..2000).all(|_| crate::combat::swing(&mut c, 10, 1000, 10.0, 15, 1.0) == 0 || c.roll(1) == 0));
        // Nomad gains 3/3 A/D every 2 levels up to 30; a Warrior swings again at 91.
        assert_eq!(guild_ad(&d, NOMAD, 30)[0] as i32, 45);
        c.levels[1] = 91;
        assert_eq!(c.swings(&d), 1);
        c.levels[1] = 90;
        assert_eq!(c.swings(&d), 0);
        // A second extra guild costs A² x 5356; the first is free.
        c.levels[1] = 0;
        assert_eq!(c.join_cost(&d, 1), 0);
        c.levels[1] = 1;
        assert_eq!(c.join_cost(&d, 6), 7_465_074);
        let dagger = d.items.iter().position(|i| i.name == "Iron Dagger").unwrap();
        let p = f64::from(d.items[dagger].price);
        assert_eq!(crate::store::buy_price(&d, dagger, 0, 1), (1.5 * p / 1.15).round() as u64);
        let kobold = d.monsters.iter().position(|m| m.name == "Kobold").unwrap();
        let gold = crate::treasure::gold(&mut c, &d, &[(kobold, 3)], false);
        assert!((400..10_000).contains(&gold), "{gold}");
    }

    /// With `DDJ_RETAIL` set: a strong party plays 400 steps on every floor, to shake out
    /// bad indexes and panics in the deeper floors' squares, monsters and specials.
    #[test]
    fn deep_floors_smoke() {
        let Ok(src) = std::env::var("DDJ_RETAIL") else { return };
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-retail-smoke");
        ddj_extract::import(Path::new(&src), &out).unwrap();
        let (d, t) = (GameData::load(&out).unwrap(), Tuning::load(&[]).unwrap());
        for floor in 1..=d.floors.len() {
            let mut g = Game::new(hero(&d, "Deep1", 0, 1, 1, 30));
            g.add_ally(&d, hero(&d, "Deep2", 1, 1, HEALER, 30)).unwrap();
            g.add_ally(&d, hero(&d, "Deep3", 1, 0, 9, 30)).unwrap();
            g.add_ally(&d, hero(&d, "Deep4", 0, 1, 6, 30)).unwrap();
            for m in &mut g.party {
                m.max_hp = 400;
                m.hp = 400;
                m.sp = m.max_sp(&d, &t);
            }
            g.enter_dungeon(&d).unwrap();
            let (x, y, f) = g.random_square(&d, floor);
            g.set_pos(&d, Pos { x, y, floor: f, facing: 0 });
            for _ in 0..400 {
                if g.living().is_empty() || g.pos().floor == 0 {
                    break;
                }
                if g.fight.is_some() {
                    g.round(&d, &t);
                } else if g.party[0].roll(6) == 0 {
                    g.turn(1);
                } else if g.chest.is_some() {
                    crate::treasure::open(&mut g, &d, &t, 0);
                } else {
                    g.step(&d, &t);
                }
            }
            let kills: u32 = g.party.iter().map(|m| m.kills).sum();
            eprintln!("floor {floor}: {kills} kills, {} living", g.living().len());
        }
    }
}
