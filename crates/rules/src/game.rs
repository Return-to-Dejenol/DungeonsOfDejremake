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
    mdr::{Floor, GameData, Words, square},
    monsters::Companion,
    store::{self, Store},
    treasure::Chest,
};

pub const SAVE_VERSION: u32 = 2;
/// Points to spend on top of [`start_stats`] when creating a character.
pub const FREE_POINTS: i16 = 5;
/// The character you control plus up to three allies.
pub const MAX_PARTY: usize = 4;
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
pub const ABILITIES: [&str; 6] = ["Thieving", "Backstab", "Critical hit", "Multiple swings", "Fighting", "Perception"];
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
    /// Fully identified (name, abilities and curses known).
    #[serde(default)]
    pub known: bool,
}

/// Lasting conditions; the city cures them on arrival.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    /// Steps or rounds left.
    pub poison: u16,
    pub disease: u16,
    pub paralysis: u16,
    pub stoned: bool,
}

/// Effects that last until the party changes floor.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Buffs {
    pub protection: i32,
    /// Bit n: resistance slot n raised by a spell.
    pub resist: u16,
    /// The strongest experience boost (potions, Charm of Experience); 0 = none.
    pub xp_mult: f32,
    pub levitate: bool,
    pub see_invisible: bool,
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
    rng: u64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Open,
    Wall,
    Door,
}

/// Midway between the race's minimum and maximum, less one, before free points.
pub fn start_stats(data: &GameData, race: usize) -> [i16; 6] {
    let r = &data.races[race];
    std::array::from_fn(|i| ((r.stat_min[i] + r.stat_max[i]) / 2 - 1).max(r.stat_min[i]))
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
        if (0..6).any(|i| stats[i] < r.stat_min[i] || stats[i] > r.stat_max[i]) || spent > FREE_POINTS {
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
            items: Vec::new(),
            pos: Pos::default(),
            life: Life::Alive,
            status: Status::default(),
            buffs: Buffs::default(),
            seen: Vec::new(),
            kills: 0,
            macros: ai::preset(ai::Role::Fighter),
            companions: Vec::new(),
            allies: Vec::new(),
            sanctuary: None,
            age_days: 16 * 365,
            quest: None,
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
    pub(crate) fn melee_rating(&self) -> i32 {
        (2 * i32::from(self.stats[STR]) + i32::from(self.stats[DEX])) / 3
    }

    /// Attack and defense as MORDOR.EXE keeps them: 10 each, the stat bonus, the best
    /// guild's A/D and the equipped items. The stat bonus is the original's (+4 a point
    /// of STR and DEX above 12, -4 below 8) on the melee rating, and STR above 15 adds
    /// defense (docs/balance.md).
    pub fn att_def(&self, data: &GameData, _t: &Tuning) -> (i32, i32) {
        let r = self.melee_rating();
        let bonus = 4 * ((r - 12).max(0) - (8 - r).max(0));
        let ad = |k: usize| self.guild_best(|g, l| guild_ad(data, g, l)[k]) as i32;
        let (mut a, mut d) = (10 + bonus + ad(0), 10 + bonus + ad(1) + (i32::from(self.stats[STR]) - 15).max(0) + self.buffs.protection);
        for h in self.items.iter().filter(|h| h.equipped) {
            a += i32::from(data.items[h.item].attack);
            d += i32::from(data.items[h.item].defense);
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

    pub(crate) fn weapon(&self, data: &GameData) -> Option<usize> {
        self.items.iter().find(|h| h.equipped && data.items[h.item].power > 0).map(|h| h.item)
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

    pub fn make_level(&mut self, data: &GameData, t: &Tuning) -> Result<String, String> {
        let (need, gold) = (self.xp_needed(data, t), Tuning::level_gold(self.level(), data.guilds[self.guild].gold_factor));
        if let Some(m) = self.quest {
            return Err(format!("The guild wants you to slay a {} first.", data.monsters[m].name));
        }
        if self.xp[self.guild] < need {
            return Err(format!("You need {} more experience.", need - self.xp[self.guild]));
        }
        if self.gold + self.bank < gold {
            return Err(format!("The guild asks {gold} gold."));
        }
        self.pay(gold);
        let g = &data.guilds[self.guild];
        let highest = self.levels.iter().all(|&l| l <= self.level());
        self.levels[self.guild] += 1;
        let level = self.level();
        let mut msg = format!("Welcome to level {level}!");
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
            msg += &format!(" You gain {gain} hits.");
        }
        // From level 5 a guild may set a quest at its quest chance (w32, MORDOR.EXE).
        if level >= 5 && (self.rnd() * 100.0).round_ties_even() <= f64::from(g.w(32)) {
            let floor = (level / 5).clamp(1, 15) as i16;
            let fits: Vec<usize> = (0..data.monsters.len()).filter(|&m| data.monsters[m].level == floor).collect();
            if !fits.is_empty() {
                let m = fits[self.roll(fits.len() as u32) as usize];
                self.quest = Some(m);
                msg += &format!(" The guild asks you to slay a {} before your next level.", data.monsters[m].name);
            }
        }
        Ok(msg)
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

    pub fn join(&mut self, data: &GameData, g: usize) -> Result<String, String> {
        let guild = &data.guilds[g];
        if self.levels[g] > 0 {
            self.guild = g;
            return Ok(format!("You are acting as a {} again.", guild.name));
        }
        if (0..6).any(|i| self.stats[i] < guild.stat_req[i]) {
            return Err("Your stats are too low.".into());
        }
        if guild.alignments & (1 << self.alignment) == 0 {
            return Err("The guild refuses your alignment.".into());
        }
        if guild.races & (1 << self.race) == 0 {
            return Err(format!("The guild won't take a {}.", data.races[self.race].name));
        }
        let cost = self.join_cost(data, g);
        if self.gold + self.bank < cost {
            return Err(format!("Joining costs {cost} gold."));
        }
        self.pay(cost);
        self.levels[g] = 1;
        self.guild = g;
        self.items.iter_mut().filter(|h| data.items[h.item].guilds & (1 << g) == 0).for_each(|h| h.equipped = false);
        self.macros = ai::preset(ai::Role::of(g));
        Ok(format!("Welcome to the {} guild!", guild.name))
    }

    /// One weapon, and one item of each other kind, may be equipped at a time.
    pub fn toggle_equip(&mut self, data: &GameData, slot: usize) -> Result<(), String> {
        let it = &data.items[self.items[slot].item];
        if self.items[slot].equipped {
            if crate::magic::cursed(data, self.items[slot].item, self.alignment) {
                return Err(format!("The {} is cursed! The store can uncurse it.", it.name));
            }
            self.items[slot].equipped = false;
            return Ok(());
        }
        if it.guilds & (1 << self.guild) == 0 {
            return Err(format!("Your guild can't use {}.", it.name));
        }
        if crate::magic::consumable(data, self.items[slot].item) {
            return Err(format!("{} is used, not worn.", it.name));
        }
        let clash = |o: &crate::mdr::Item| if it.power > 0 { o.power > 0 } else { o.power == 0 && o.kind == it.kind };
        for h in &mut self.items {
            if h.equipped && clash(&data.items[h.item]) {
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
            c.macros = ai::preset(ai::Role::of(c.guild));
            c.age_days = c.age_days.max(16 * 365);
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

/// Names of the characters saved in `dir`.
pub fn saved(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|d| d.iter().filter_map(|p| p.file_name()?.to_str()?.strip_suffix(".json").filter(|n| *n != "store").map(str::to_owned)).collect())
        .unwrap_or_default();
    v.sort();
    v
}


/// A running game: the party plus what isn't saved (the fight, cleared rooms, log).
/// `party[0]` is the character the player controls.
pub struct Game {
    pub party: Vec<Character>,
    pub fight: Option<Fight>,
    /// A chest left in the room after a fight, until the party moves on.
    pub chest: Option<Chest>,
    /// (floor, area) cleared since the last visit to town.
    pub cleared: Vec<(usize, i16)>,
    pub log: Vec<String>,
    /// The store's and Confinement's shared stock (saves/store.json).
    pub store: Store,
}

impl Game {
    pub fn new(lead: Character) -> Self {
        Self { party: vec![lead], fight: None, chest: None, cleared: Vec::new(), log: Vec::new(), store: Store::default() }
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
    pub fn add_ally(&mut self, ally: Character) -> Result<(), String> {
        if self.party.len() >= MAX_PARTY {
            return Err("A party holds at most four.".into());
        }
        if !ally.alive() {
            return Err(format!("{} must be raised first.", ally.name));
        }
        if self.party.iter().any(|m| m.alignment.abs_diff(ally.alignment) == 2) {
            return Err("Good and Evil won't travel together.".into());
        }
        if self.party.iter().any(|m| m.name == ally.name) {
            return Err(format!("{} is already in the party.", ally.name));
        }
        let name = ally.name.clone();
        self.party.push(ally);
        self.party[0].allies = self.party[1..].iter().map(|m| m.name.clone()).collect();
        self.say(format!("{name} joins the party."));
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
                    m.buffs = Buffs::default();
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
        self.cleared.clear();
        self.fight = None;
        self.say("You return to the city and rest.");
    }

    pub fn enter_dungeon(&mut self, data: &GameData) -> Result<(), String> {
        if let Some(m) = self.party.iter().find(|m| !m.alive()) {
            return Err(format!("{} is dead; visit the morgue or dismiss them first.", m.name));
        }
        let f = &data.floors[0];
        let i = f.squares.iter().position(|s| s.flags & square::STAIRS_UP != 0).unwrap_or(0);
        self.set_pos(data, Pos { x: i % f.width, y: i / f.width, floor: 1, facing: 0 });
        self.say("You descend into the dungeon.");
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
            // Peaceful monsters let you walk on through.
            self.fight = None;
        }
        if self.party.iter().any(|m| m.alive() && m.status.paralysis > 0) {
            return self.say("A paralysed party member can't be left behind.");
        }
        let Some(f) = self.floor(data) else { return };
        let Pos { x, y, facing, .. } = self.pos();
        let secret = match facing {
            0 => f.square(x, y).flags & square::UPPER_SECRET_DOOR != 0,
            1 => f.square(x, y).flags & square::RIGHT_SECRET_DOOR != 0,
            _ => ahead(f, x, y, facing).is_some_and(|(nx, ny)| f.square(nx, ny).flags & if facing == 2 { square::UPPER_SECRET_DOOR } else { square::RIGHT_SECRET_DOOR } != 0),
        };
        let to = ahead(f, x, y, facing).filter(|&(nx, ny)| (side(f, x, y, facing) != Side::Wall || secret) && f.square(nx, ny).flags & square::ROCK == 0);
        let Some((nx, ny)) = to else { return self.say("A wall blocks your way.") };
        if secret {
            self.say("You find a secret door!");
        }
        let back = self.pos();
        self.set_pos(data, Pos { x: nx, y: ny, ..back });
        self.walk_effects(data, t);
        self.arrive(data, back);
    }

    /// Each step: poison and disease bite, SP trickles back, and a Healer in the party
    /// slowly mends everyone (docs/balance.md).
    fn walk_effects(&mut self, data: &GameData, t: &Tuning) {
        let healer = self.party.iter().any(|m| m.alive() && m.levels[HEALER] > 0);
        let mut hurt = Vec::new();
        for (i, m) in self.party.iter_mut().enumerate().filter(|(_, m)| m.alive()) {
            let max_sp = m.max_sp(data, t);
            m.sp = (m.sp + ((f64::from(max_sp) * t.spells.regen_per_step).ceil() as i32)).min(max_sp);
            if healer {
                m.hp = (m.hp + (m.max_hp / 100).max(1)).min(m.max_hp);
            }
            // MORDOR.EXE: d200 + 2 (CON - 10) - 2 x strength above 185 (195 for disease)
            // throws it off; otherwise poison takes 2 hits a step and disease 1.
            let con = 2 * i32::from(m.stats[CON] - 10);
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
            m.status.paralysis = m.status.paralysis.saturating_sub(1);
        }
        for (i, dmg) in hurt {
            self.hurt(i, dmg);
        }
        self.companions_step(data);
    }

    /// Effects of the square just entered, then a possible encounter.
    pub(crate) fn arrive(&mut self, data: &GameData, back: Pos) {
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
            self.say("The floor spins you around!");
        }
        let levitating = self.party.iter().filter(|m| m.alive()).all(|m| m.buffs.levitate);
        if flags & square::PIT != 0 && !levitating {
            for i in self.living() {
                let dmg = 1 + self.party[i].roll(4 * floor as u32) as i32;
                self.say(format!("{} falls into a pit for {dmg} damage.", self.party[i].name));
                self.hurt(i, dmg);
            }
        }
        if flags & square::WATER != 0 && !levitating {
            for i in self.living() {
                // Heavy armour drags you under (defense 9+ items, the original's "heavy items").
                let heavy = self.party[i].items.iter().filter(|h| h.equipped).map(|h| i32::from(data.items[h.item].defense)).sum::<i32>() >= 12;
                if heavy && self.party[i].chance(0.25) {
                    let dmg = 2 + self.party[i].roll(3 * floor as u32) as i32;
                    self.say(format!("{} struggles in the water for {dmg} damage.", self.party[i].name));
                    self.hurt(i, dmg);
                }
            }
        }
        if flags & square::QUICKSAND != 0 && !levitating {
            for i in self.living() {
                let dmg = 1 + self.party[i].roll(2 * floor as u32) as i32;
                self.say(format!("{} sinks into quicksand for {dmg} damage.", self.party[i].name));
                if self.party[i].chance(0.1) && !self.party[i].items.is_empty() {
                    let n = self.party[i].items.len() as u32;
                    let s = self.party[i].roll(n) as usize;
                    if !self.party[i].items[s].equipped {
                        let h = self.party[i].items.remove(s);
                        self.say(format!("{} loses {} in the sand.", self.party[i].name, data.items[h.item].name));
                    }
                }
                self.hurt(i, dmg);
            }
        }
        if flags & square::EXTINGUISH != 0 {
            for m in &mut self.party {
                m.buffs = Buffs::default();
            }
            self.say("Your magic fades away.");
        }
        if self.living().is_empty() {
            return;
        }
        if flags & square::CHUTE != 0 && !levitating {
            let drop = f.chutes.iter().find(|c| c.at == (x, y)).map_or(1, |c| c.drop.max(1) as usize);
            let to = (floor + drop).min(data.floors.len());
            self.say(format!("You slide down a chute to level {to}!"));
            self.set_pos(data, Pos { floor: to, ..self.pos() });
            for i in self.living() {
                let dmg = 1 + self.party[i].roll(6 * drop as u32) as i32;
                self.hurt(i, dmg);
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
            return self.say("A teleporter whisks you away!");
        }
        if area != 0 && !self.cleared.contains(&(floor, area)) {
            self.spawn(data, area, flags & square::STUD != 0, back);
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
    pub(crate) fn hurt(&mut self, i: usize, dmg: i32) -> bool {
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
        let name = m.name.clone();
        self.say(format!("{name} has died!"));
        if self.living().is_empty() {
            self.fight = None;
            self.say("The whole party has fallen. Your bodies lie where you fell.");
            for m in &mut self.party {
                m.pos = Pos::default();
            }
        } else if i == 0 {
            let next = self.living()[0];
            self.switch_to(next);
            self.say(format!("You take control of {}.", self.party[0].name));
        }
        true
    }

    pub fn take_stairs(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.floor(data) else { return };
        let p = self.pos();
        let flags = f.square(p.x, p.y).flags;
        if self.fight.is_some() {
            self.say("Not while fighting.");
        } else if flags & square::STAIRS_UP != 0 {
            if p.floor == 1 {
                self.to_town(data, t);
            } else {
                self.set_pos(data, Pos { floor: p.floor - 1, ..p });
                self.say(format!("You climb to level {}.", p.floor - 1));
            }
        } else if flags & square::STAIRS_DOWN != 0 && p.floor < data.floors.len() {
            self.set_pos(data, Pos { floor: p.floor + 1, ..p });
            self.say(format!("You descend to level {}.", p.floor + 1));
            self.arrive(data, p);
        } else if flags & square::CHUTE != 0 && self.party.iter().filter(|m| m.alive()).all(|m| m.buffs.levitate) {
            self.say("You float down the chute.");
            self.set_pos(data, Pos { floor: (p.floor + 1).min(data.floors.len()), ..p });
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

    pub fn seer(&mut self, data: &GameData, name: &str) -> String {
        let Some(cost) = self.seer_cost(data, name) else { return "The Seer has never heard of it, or it lies too deep to see.".into() };
        let lead = &mut self.party[0];
        if lead.gold + lead.bank < cost {
            return format!("The Seer wants {cost} gold.");
        }
        lead.pay(cost);
        let name = name.trim().to_lowercase();
        if let Some(m) = self.party.iter().find(|m| m.name.to_lowercase() == name) {
            return match m.life {
                Life::Dead(p) => format!("{} lies at {},{} on level {} ({cost} gold).", m.name, p.x + 1, p.y + 1, p.floor),
                Life::InMorgue => format!("{} is in the morgue.", m.name),
                Life::Alive => format!("{} is alive and well.", m.name),
            };
        }
        let Some(m) = data.monsters.iter().position(|m| m.name.to_lowercase() == name) else { return "The Seer has never heard of it.".into() };
        let id = data.monsters[m].id;
        let lairs: Vec<String> = data.floors.iter().filter(|f| f.areas.iter().any(|a| a.lair == id)).map(|f| f.level.to_string()).collect();
        if lairs.is_empty() {
            format!("The {} wanders level {} ({cost} gold).", data.monsters[m].name, data.monsters[m].level)
        } else {
            format!("The {} keeps a lair on level {} ({cost} gold).", data.monsters[m].name, lairs.join(" and "))
        }
    }

    /// The store's stock, starting it fresh if none was loaded.
    pub fn stock(&mut self, data: &GameData) -> &mut Store {
        if self.store.items.len() != data.items.len() {
            self.store = Store::fresh(data);
        }
        &mut self.store
    }

    /// Member `i`'s price for an item (CHA's passive included).
    pub fn buy_price(&mut self, data: &GameData, i: usize, item: usize) -> u64 {
        let s = self.stock(data).count(item);
        store::cha_buy(&self.party[i], store::buy_price(data, item, s))
    }

    pub fn sell_price(&mut self, data: &GameData, i: usize, slot: usize) -> u64 {
        let h = self.party[i].items[slot].clone();
        let s = self.stock(data).count(h.item);
        store::cha_sell(&self.party[i], store::sell_price(data, &h, s))
    }

    pub fn identify_price(&mut self, data: &GameData, i: usize, slot: usize) -> u64 {
        let item = self.party[i].items[slot].item;
        let s = self.stock(data).count(item);
        store::identify_price(data, item, s)
    }

    pub fn buy(&mut self, data: &GameData, i: usize, item: usize) -> Result<String, String> {
        let price = self.buy_price(data, i, item);
        if self.store.count(item) == 0 {
            return Err("The store has none left.".into());
        }
        let c = &mut self.party[i];
        if c.gold + c.bank < price {
            return Err("You can't afford that.".into());
        }
        if c.items.len() >= 40 {
            return Err("You can't carry any more.".into());
        }
        c.pay(price);
        c.items.push(crate::magic::new_held(data, item, true));
        self.store.take(item);
        Ok(format!("You bought {} for {price} gold.", data.items[item].name))
    }

    pub fn sell(&mut self, data: &GameData, i: usize, slot: usize) -> String {
        let it = &data.items[self.party[i].items[slot].item];
        if (500..512).contains(&it.w(10)) {
            return format!("The store won't buy guild items like the {}.", it.name);
        }
        let price = self.sell_price(data, i, slot);
        let h = self.party[i].items.remove(slot);
        self.party[i].gold += price;
        self.store.put(data, &h);
        format!("Sold {} for {price} gold.", data.items[h.item].name)
    }

    pub fn identify(&mut self, data: &GameData, i: usize, slot: usize) -> Result<String, String> {
        let cost = self.identify_price(data, i, slot);
        let c = &mut self.party[i];
        if c.gold + c.bank < cost {
            return Err(format!("Identifying costs {cost} gold."));
        }
        c.pay(cost);
        c.items[slot].known = true;
        Ok(format!("It is {}.", data.items[c.items[slot].item].name))
    }

    /// Uncursing an equipped item costs 1.6 x its price and takes it off; the item
    /// stays cursed (MORDOR.EXE).
    pub fn uncurse(&mut self, data: &GameData, i: usize, slot: usize) -> Result<String, String> {
        let c = &mut self.party[i];
        let item = c.items[slot].item;
        let cost = store::uncurse_price(data, item);
        if c.gold + c.bank < cost {
            return Err(format!("Uncursing costs {cost} gold."));
        }
        c.pay(cost);
        c.items[slot].equipped = false;
        c.items[slot].known = false;
        Ok(format!("The {} comes off.", data.items[item].name))
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

    /// The fee comes from the dead character's purse and bank, then the party's.
    fn charge(&mut self, i: usize, cost: u64) -> u64 {
        let mut left = cost;
        for j in std::iter::once(i).chain(0..self.party.len()) {
            let m = &mut self.party[j];
            let take = left.min(m.gold + m.bank);
            m.pay(take);
            left -= take;
        }
        cost - left
    }

    pub fn rescue(&mut self, i: usize) {
        let Life::Dead(p) = self.party[i].life else { return };
        let paid = self.charge(i, self.rescue_cost(i));
        if self.party[i].rnd() * 100.0 <= 4.0 * p.floor as f64 {
            return self.say(format!("The rescuers were lost looking for {} (fee {paid} gold).", self.party[i].name));
        }
        self.party[i].life = Life::InMorgue;
        self.party[i].pos = Pos::default();
        self.say(format!("Rescuers bring {} to the morgue for {paid} gold.", self.party[i].name));
    }

    /// A raise works clamp(35 x CON / 4.5 + 10, 10, 98)% of the time (MORDOR.EXE);
    /// a failure ages the body a year, and a success often costs a point of CON.
    pub fn raise(&mut self, data: &GameData, i: usize) {
        if self.party[i].life != Life::InMorgue {
            return;
        }
        let paid = self.charge(i, self.raise_cost(i));
        let m = &mut self.party[i];
        let odds = (35.0 * f64::from(m.stats[CON]) / 4.5 + 10.0).round_ties_even().clamp(10.0, 98.0) as i32;
        if m.d100() > odds {
            m.age_days += 365;
            let name = m.name.clone();
            return self.say(format!("Complications! {name} ages a year and stays dead (fee {paid} gold)."));
        }
        m.life = Life::Alive;
        m.hp = m.max_hp;
        let min = data.races[m.race].stat_min[CON];
        let msg = if m.chance(0.5) && m.stats[CON] > min {
            m.stats[CON] -= 1;
            format!("{} is raised for {paid} gold, but loses a point of CON.", m.name)
        } else {
            format!("{} is raised for {paid} gold.", m.name)
        };
        self.say(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai::{Act, Target},
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
            g.add_ally(hero(&d, &format!("Healer{seed}"), 1, 1, HEALER, 4)).unwrap();
            g.add_ally(hero(&d, &format!("Mage{seed}"), 1, 0, 8, 4)).unwrap();
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
        let paladin = hero(&d, "Pal", 0, 0, 2, 6);
        let minor = d.spells.iter().position(|s| s.name == "Minor Heal").unwrap();
        assert!(magic::known(&paladin, &d).contains(&minor));
        let early = hero(&d, "Pal5", 0, 0, 2, 5);
        assert!(!magic::known(&early, &d).contains(&minor));
        // Wizards give up the mind spells (docs/balance.md).
        let sleep = d.spells.iter().position(|s| s.name == "Sleep").unwrap();
        assert!(magic::taught_at(&d, 10, sleep).is_none() && magic::taught_at(&d, 8, sleep).is_some());
        // Charm of Experience: Seekers at 16.
        let charm = d.spells.iter().position(|s| s.id == magic::CHARM_OF_EXPERIENCE).unwrap();
        assert_eq!((magic::taught_at(&d, 5, charm), magic::taught_at(&d, 8, charm)), (Some(16), None));
    }

    #[test]
    fn experience_potion_boosts_until_floor_change() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Quaffer", 0, 1, 0, 1));
        let potion = d.items.iter().position(|i| i.id == magic::POTION_OF_EXPERIENCE).unwrap();
        assert_eq!(g.stock(&d).count(potion), crate::store::UNLIMITED);
        g.buy(&d, 0, potion).unwrap();
        g.enter_dungeon(&d).unwrap();
        magic::use_item(&mut g, &d, &t, 0, potion);
        assert_eq!(g.party[0].add_xp(&d, &t, 10), 15);
        let p = g.pos();
        g.set_pos(&d, Pos { floor: 2, ..p });
        assert_eq!(g.party[0].add_xp(&d, &t, 10), 10);
        assert!(g.party[0].items.iter().all(|h| h.item != potion));
    }

    #[test]
    fn healer_ai_heals_the_wounded() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(hero(&d, "Tank", 0, 1, 1, 6));
        g.add_ally(hero(&d, "Medic", 1, 1, HEALER, 6)).unwrap();
        g.party[1].sp = 500;
        g.enter_dungeon(&d).unwrap();
        let kobold = d.monsters.iter().position(|m| m.name == "Kobold").unwrap();
        let foes = vec![crate::combat::Foe { monster: kobold, hp: 50, max_hp: 50, asleep: 0 }];
        g.fight = Some(crate::combat::Fight { foes, round: 0, order: Some(Act::Defend), back: g.pos(), lair: false, peaceful: false, chest: None, defending: vec![false; 2], gold: 0, opening: None });
        g.party[0].hp = 3;
        let act = crate::ai::decide(&g, &d, &t, 1);
        assert!(matches!(act, Act::Cast(s, Target::LowestAlly) if magic::kind(&d.spells[s]) == Some(Magic::Heal)), "{act:?}");
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
        assert!(g.hurt(0, 1000));
        assert_eq!(g.party[0].life, Life::Dead(at));
        g.party[0].gold = 100_000;
        while g.party[0].life != Life::InMorgue {
            g.rescue(0);
        }
        while g.party[0].life != Life::Alive {
            g.raise(&d, 0);
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-save-test");
        g.party[0].save(&dir).unwrap();
        let back = Character::load(&dir.join("Raise.json")).unwrap();
        assert_eq!((back.levels, back.gold, back.stats), (g.party[0].levels.clone(), g.party[0].gold, g.party[0].stats));
        assert!(saved(&dir).contains(&"Raise".to_string()));
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
        assert!(g.log.iter().any(|l| l.contains("disarms")));
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
        assert_eq!(crate::store::buy_price(&d, dagger, 1), (1.5 * p / 1.15).round() as u64);
        let kobold = &d.monsters[d.monsters.iter().position(|m| m.name == "Kobold").unwrap()];
        let gold = crate::treasure::gold(&mut c, kobold, 3, false);
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
            g.add_ally(hero(&d, "Deep2", 1, 1, HEALER, 30)).unwrap();
            g.add_ally(hero(&d, "Deep3", 1, 0, 9, 30)).unwrap();
            g.add_ally(hero(&d, "Deep4", 0, 1, 6, 30)).unwrap();
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
