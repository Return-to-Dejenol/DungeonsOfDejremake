//! The first playable slice: one character, the town's guilds, store and morgue, and the
//! dungeon with room spawns and timed melee rounds. Numbers come from the imported
//! tables and [`Tuning`]; the remake's rules are in docs/balance.md.

use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    Tuning,
    mdr::{Floor, GameData, square},
};

pub const SAVE_VERSION: u32 = 1;
/// Points to spend on top of [`start_stats`] when creating a character.
pub const FREE_POINTS: i16 = 5;
pub const STATS: [&str; 6] = ["STR", "INT", "WIS", "CON", "CHA", "DEX"];
const STR: usize = 0;
const CON: usize = 3;
const DEX: usize = 5;

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
    /// The corpse lies here until rescuers bring it to the morgue.
    Dead(Pos),
    InMorgue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Held {
    /// Index into `GameData::items`.
    pub item: usize,
    pub equipped: bool,
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
    pub hp: i32,
    pub max_hp: i32,
    /// Attack/defense from guild teaching; the best guild counts.
    pub guild_ad: f64,
    pub items: Vec<Held>,
    pub pos: Pos,
    pub life: Life,
    /// The automap: squares seen, per floor, row-major.
    pub seen: Vec<Vec<bool>>,
    pub kills: u32,
    rng: u64,
}

pub struct Foe {
    pub monster: usize,
    pub hp: i32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Fight,
    Defend,
    Run,
}

pub struct Fight {
    pub foes: Vec<Foe>,
    pub action: Action,
    /// Where the party came from, for running away.
    pub back: Pos,
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
        if (0..6).any(|i| stats[i] < r.stat_min[i] || stats[i] > r.stat_max[i]) || spent > FREE_POINTS {
            return Err("Stats are outside the race's range.".into());
        }
        if r.alignments & (1 << alignment) == 0 {
            return Err(format!("{}s cannot be that alignment.", r.name));
        }
        let mut levels = vec![0; data.guilds.len()];
        levels[0] = 1;
        let hp = i32::from(stats[CON]) + 4;
        let seed = name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3));
        Ok(Self {
            version: SAVE_VERSION,
            name: name.trim().into(),
            race,
            female,
            alignment,
            stats,
            xp: vec![0; levels.len()],
            levels,
            guild: 0,
            gold: 1500,
            hp,
            max_hp: hp,
            guild_ad: 0.0,
            items: Vec::new(),
            pos: Pos::default(),
            life: Life::Alive,
            seen: Vec::new(),
            kills: 0,
            rng: seed | 1,
        })
    }

    /// 0..n, from a generator saved with the character.
    pub fn roll(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % u64::from(n.max(1))) as u32
    }

    fn chance(&mut self, p: f64) -> bool {
        f64::from(self.roll(10_000)) < p * 10_000.0
    }

    pub fn level(&self) -> u32 {
        self.levels[self.guild]
    }

    /// Total XP the current guild wants before its next level.
    pub fn xp_needed(&self, data: &GameData, t: &Tuning) -> u64 {
        t.xp_needed(self.level(), data.races[self.race].xp_factor, data.guilds[self.guild].xp_factor)
    }

    /// Melee rating (2 x STR + DEX) / 3 sets to-hit; STR above 15 adds defense.
    pub fn att_def(&self, data: &GameData) -> (i32, i32) {
        let rating = (2 * i32::from(self.stats[STR]) + i32::from(self.stats[DEX])) / 3;
        let (mut a, mut d) = (10 + self.guild_ad as i32 + rating - 10, 10 + self.guild_ad as i32 + (i32::from(self.stats[STR]) - 15).max(0));
        for h in self.items.iter().filter(|h| h.equipped) {
            a += i32::from(data.items[h.item].attack);
            d += i32::from(data.items[h.item].defense);
        }
        (a, d)
    }

    fn weapon(&self, data: &GameData) -> Option<usize> {
        self.items.iter().find(|h| h.equipped && data.items[h.item].power > 0).map(|h| h.item)
    }

    pub fn add_xp(&mut self, data: &GameData, t: &Tuning, xp: u64) {
        // Pinned: XP stops one short of the level after next until the next level is made.
        let cap = t.xp_needed(self.level() + 1, data.races[self.race].xp_factor, data.guilds[self.guild].xp_factor) - 1;
        self.xp[self.guild] = (self.xp[self.guild] + xp).min(cap);
    }

    pub fn make_level(&mut self, data: &GameData, t: &Tuning) -> Result<String, String> {
        let (need, gold) = (self.xp_needed(data, t), Tuning::level_gold(self.level()));
        if self.xp[self.guild] < need {
            return Err(format!("You need {} more experience.", need - self.xp[self.guild]));
        }
        if self.gold < gold {
            return Err(format!("The guild asks {gold} gold."));
        }
        self.gold -= gold;
        let g = &data.guilds[self.guild];
        let highest = self.levels.iter().all(|&l| l <= self.level());
        self.levels[self.guild] += 1;
        let level = self.level();
        self.guild_ad = self.guild_ad.max(f64::from(level - 1) * f64::from(g.fighting) * t.combat.ad_per_fighting);
        let mut msg = format!("Welcome to level {level}!");
        if highest {
            let avg = if level <= g.hp_cap as u32 { g.hp_per_level } else { g.hp_after };
            let gain = (i32::from(avg) - 1 + self.roll(3) as i32).max(1) + (i32::from(self.stats[CON]) - 16).max(0);
            self.max_hp += gain;
            self.hp += gain;
            msg += &format!(" You gain {gain} hits.");
        }
        Ok(msg)
    }

    /// Joining is free for the first guild after Nomad, then 5,000 gold per guild held.
    pub fn join_cost(&self) -> u64 {
        5_000 * (self.levels.iter().filter(|&&l| l > 0).count() as u64 - 1)
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
        let cost = self.join_cost();
        if self.gold < cost {
            return Err(format!("Joining costs {cost} gold."));
        }
        self.gold -= cost;
        self.levels[g] = 1;
        self.guild = g;
        self.items.iter_mut().filter(|h| data.items[h.item].guilds & (1 << g) == 0).for_each(|h| h.equipped = false);
        Ok(format!("Welcome to the {} guild!", guild.name))
    }

    pub fn buy(&mut self, data: &GameData, item: usize) -> Result<String, String> {
        let it = &data.items[item];
        if self.gold < u64::from(it.price) {
            return Err("You can't afford that.".into());
        }
        if self.items.len() >= 40 {
            return Err("You can't carry any more.".into());
        }
        self.gold -= u64::from(it.price);
        self.items.push(Held { item, equipped: false });
        Ok(format!("You bought {}.", it.name))
    }

    /// The store pays half price.
    pub fn sell(&mut self, data: &GameData, slot: usize) -> String {
        let h = self.items.remove(slot);
        let price = u64::from(data.items[h.item].price) / 2;
        self.gold += price;
        format!("Sold {} for {price} gold.", data.items[h.item].name)
    }

    /// One weapon, and one item of each other kind, may be equipped at a time.
    pub fn toggle_equip(&mut self, data: &GameData, slot: usize) -> Result<(), String> {
        let it = &data.items[self.items[slot].item];
        if self.items[slot].equipped {
            self.items[slot].equipped = false;
            return Ok(());
        }
        if it.guilds & (1 << self.guild) == 0 {
            return Err(format!("Your guild can't use {}.", it.name));
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

    fn see(&mut self, data: &GameData) {
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
        let c: Self = serde_json::from_slice(&fs::read(path)?)?;
        if c.version != SAVE_VERSION {
            return Err(io::Error::other(format!("save version {} is not supported", c.version)));
        }
        Ok(c)
    }
}

/// Names of the characters saved in `dir`.
pub fn saved(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|d| d.flatten().filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(".json").map(str::to_owned)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The store's stock: store-only items at ordinary prices, cheapest first.
pub fn stock(data: &GameData) -> Vec<usize> {
    let mut v: Vec<usize> = (0..data.items.len()).filter(|&i| data.items[i].level == 0 && (1..=100_000).contains(&data.items[i].price)).collect();
    v.sort_by_key(|&i| data.items[i].price);
    v
}

/// A running game: the character plus what isn't saved (the fight, cleared rooms, log).
pub struct Game {
    pub ch: Character,
    pub fight: Option<Fight>,
    /// (floor, area) cleared since the last visit to town.
    pub cleared: Vec<(usize, i16)>,
    pub log: Vec<String>,
}

impl Game {
    pub fn new(ch: Character) -> Self {
        Self { ch, fight: None, cleared: Vec::new(), log: Vec::new() }
    }

    fn say(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
    }

    pub fn floor<'a>(&self, data: &'a GameData) -> Option<&'a Floor> {
        self.ch.pos.floor.checked_sub(1).map(|f| &data.floors[f])
    }

    /// Arriving in town: rest to full health; the dungeon repopulates.
    fn to_town(&mut self) {
        self.ch.pos = Pos::default();
        self.ch.hp = self.ch.max_hp;
        self.cleared.clear();
        self.fight = None;
        self.say("You return to the city and rest.");
    }

    pub fn enter_dungeon(&mut self, data: &GameData) -> Result<(), String> {
        if self.ch.life != Life::Alive {
            return Err("Only the living may enter.".into());
        }
        let f = &data.floors[0];
        let i = f.squares.iter().position(|s| s.flags & square::STAIRS_UP != 0).unwrap_or(0);
        self.ch.pos = Pos { x: i % f.width, y: i / f.width, floor: 1, facing: 0 };
        self.ch.see(data);
        self.say("You descend into the dungeon.");
        Ok(())
    }

    pub fn turn(&mut self, by: u8) {
        if self.fight.is_none() {
            self.ch.pos.facing = (self.ch.pos.facing + by) % 4;
        }
    }

    pub fn step(&mut self, data: &GameData) {
        if self.fight.is_some() {
            return self.say("You can't walk away; choose Run.");
        }
        let Some(f) = self.floor(data) else { return };
        let Pos { x, y, facing, .. } = self.ch.pos;
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
        let back = self.ch.pos;
        self.ch.pos.x = nx;
        self.ch.pos.y = ny;
        self.arrive(data, back);
    }

    /// Effects of the square just entered, then a possible encounter.
    fn arrive(&mut self, data: &GameData, back: Pos) {
        self.ch.see(data);
        let f = self.floor(data).unwrap();
        let Pos { x, y, floor, .. } = self.ch.pos;
        let sq = f.square(x, y);
        let (flags, area) = (sq.flags, sq.area);
        for (bit, dir) in [(square::FACE_NORTH, 0), (square::FACE_EAST, 1), (square::FACE_SOUTH, 2), (square::FACE_WEST, 3)] {
            if flags & bit != 0 {
                self.ch.pos.facing = dir;
            }
        }
        if flags & square::ROTATOR != 0 {
            self.ch.pos.facing = self.ch.roll(4) as u8;
            self.say("The floor spins you around!");
        }
        if flags & square::PIT != 0 {
            let dmg = 1 + self.ch.roll(4 * floor as u32) as i32;
            self.say(format!("You fall into a pit for {dmg} damage."));
            if self.hurt(dmg) {
                return;
            }
        }
        if flags & square::CHUTE != 0 {
            let drop = f.chutes.iter().find(|c| c.at == (x, y)).map_or(1, |c| c.drop.max(1) as usize);
            let to = (floor + drop).min(data.floors.len());
            let dmg = 1 + self.ch.roll(6 * drop as u32) as i32;
            self.say(format!("You slide down a chute to level {to} and take {dmg} damage."));
            self.ch.pos.floor = to;
            self.ch.see(data);
            self.hurt(dmg);
            return;
        }
        if flags & square::TELEPORTER != 0 {
            let tp = f.teleporters.iter().find(|tp| tp.at == (x, y)).and_then(|tp| tp.to);
            let (tx, ty, tf) = match tp {
                Some((tx, ty, tf)) => (tx, ty, (tf as usize).clamp(1, data.floors.len())),
                None => loop {
                    let (rx, ry) = (self.ch.roll(f.width as u32) as usize, self.ch.roll(f.height as u32) as usize);
                    if f.square(rx, ry).flags & square::ROCK == 0 {
                        break (rx, ry, floor);
                    }
                },
            };
            self.ch.pos = Pos { x: tx, y: ty, floor: tf, facing: self.ch.pos.facing };
            self.ch.see(data);
            return self.say("A teleporter whisks you away!");
        }
        if area != 0 && !self.cleared.contains(&(floor, area)) {
            self.spawn(data, area, flags & square::STUD != 0, back);
        }
    }

    /// Rooms hold their lair monster, or a group of a type the room allows.
    fn spawn(&mut self, data: &GameData, area: i16, stud: bool, back: Pos) {
        let floor = self.ch.pos.floor;
        let a = &data.floors[floor - 1].areas[area as usize];
        let level = floor as i16 + i16::from(stud);
        let pick = if a.lair != 0 {
            data.monsters.iter().position(|m| m.id == a.lair)
        } else {
            if !self.ch.chance(0.6) {
                self.cleared.push((floor, area));
                return;
            }
            let fits: Vec<usize> = (0..data.monsters.len())
                .filter(|&i| a.monster_types >> data.monsters[i].kind & 1 != 0 && data.monsters[i].level == level.min(16))
                .collect();
            (!fits.is_empty()).then(|| fits[self.ch.roll(fits.len() as u32) as usize])
        };
        let Some(m) = pick else { return };
        let mon = &data.monsters[m];
        let n = if a.lair != 0 { 1 } else { 1 + self.ch.roll(mon.group.max(1) as u32) };
        let foes = (0..n).map(|_| Foe { monster: m, hp: (i32::from(mon.hp) * (75 + self.ch.roll(51) as i32) / 100).max(1) }).collect();
        self.say(format!("You encounter {n} {}{}!", mon.name, if n > 1 { "s" } else { "" }));
        self.fight = Some(Fight { foes, action: Action::Fight, back });
    }

    /// Takes damage; true if it killed the character.
    fn hurt(&mut self, dmg: i32) -> bool {
        self.ch.hp -= dmg;
        if self.ch.hp > 0 {
            return false;
        }
        self.ch.hp = 0;
        self.ch.life = Life::Dead(self.ch.pos);
        self.fight = None;
        self.say("You have died. Your body lies where you fell.");
        self.ch.pos = Pos::default();
        true
    }

    pub fn take_stairs(&mut self, data: &GameData) {
        let Some(f) = self.floor(data) else { return };
        let flags = f.square(self.ch.pos.x, self.ch.pos.y).flags;
        if self.fight.is_some() {
            self.say("Not while fighting.");
        } else if flags & square::STAIRS_UP != 0 {
            if self.ch.pos.floor == 1 {
                self.to_town();
            } else {
                self.ch.pos.floor -= 1;
                self.ch.see(data);
                self.say(format!("You climb to level {}.", self.ch.pos.floor));
            }
        } else if flags & square::STAIRS_DOWN != 0 && self.ch.pos.floor < data.floors.len() {
            self.ch.pos.floor += 1;
            let back = self.ch.pos;
            self.say(format!("You descend to level {}.", self.ch.pos.floor));
            self.arrive(data, back);
        } else {
            self.say("There are no stairs here.");
        }
    }

    /// One combat round: the character acts, then every living foe attacks.
    pub fn round(&mut self, data: &GameData, t: &Tuning) {
        let Some(action) = self.fight.as_ref().map(|f| f.action) else { return };
        let c = &t.combat;
        let hit = |att: i32, def: i32| (0.5 + f64::from(att - def) / c.hit_spread).clamp(0.05, 0.95);
        let (att, mut def) = self.ch.att_def(data);
        if action == Action::Defend {
            def += def / 2;
        }
        if action == Action::Run {
            if self.ch.chance(0.6) {
                let back = self.fight.take().unwrap().back;
                self.ch.pos = back;
                return self.say("You flee back the way you came.");
            }
            self.say("You fail to get away!");
        }
        if action == Action::Fight {
            let target = self.fight.as_ref().unwrap().foes.iter().position(|f| f.hp > 0).unwrap();
            let m = &data.monsters[self.fight.as_ref().unwrap().foes[target].monster];
            if self.ch.chance(hit(att, i32::from(m.defense))) {
                let (power, mult) = self.ch.weapon(data).map_or((2, 1.0), |w| (data.items[w].power, data.items[w].damage_mult));
                let dmg = ((1 + self.ch.roll(power as u32 * c.weapon_die) as i32 + (i32::from(self.ch.stats[STR]) - 10) / 2).max(1) as f32 * mult) as i32;
                let foe = &mut self.fight.as_mut().unwrap().foes[target];
                foe.hp -= dmg;
                let killed = foe.hp <= 0;
                self.say(format!("You hit the {} for {dmg}{}", m.name, if killed { " and kill it!" } else { "." }));
                if killed {
                    self.ch.kills += 1;
                    let floor = self.ch.pos.floor as u32;
                    let base = (f64::from(m.hp) * f64::from(m.level) * c.xp_per_hp_level + f64::from(m.attack) / 2.0) as u64;
                    let xp = t.xp_award(base, floor, 0);
                    self.ch.add_xp(data, t, xp);
                    let gold = u64::from(m.level.max(1) as u32 * (c.gold_min + self.ch.roll(c.gold_max - c.gold_min + 1)));
                    self.ch.gold += gold;
                    self.say(format!("You gain {xp} experience and find {gold} gold."));
                }
            } else {
                self.say(format!("You miss the {}.", m.name));
            }
        }
        let foes: Vec<usize> = self.fight.as_ref().unwrap().foes.iter().filter(|f| f.hp > 0).map(|f| f.monster).collect();
        if foes.is_empty() {
            return self.win(data, t);
        }
        for m in foes {
            let m = &data.monsters[m];
            if self.ch.chance(hit(i32::from(m.attack), def)) {
                let dmg = 1 + self.ch.roll(m.level.max(1) as u32 * c.monster_die + (m.attack.max(0) / 20) as u32) as i32;
                self.say(format!("The {} hits you for {dmg}.", m.name));
                if self.hurt(dmg) {
                    return;
                }
            }
        }
    }

    fn win(&mut self, data: &GameData, t: &Tuning) {
        self.fight = None;
        let Pos { x, y, floor, .. } = self.ch.pos;
        let area = data.floors[floor - 1].square(x, y).area;
        self.cleared.push((floor, area));
        self.say("The room is clear.");
        if self.ch.chance(t.combat.item_chance) {
            let found: Vec<usize> = (0..data.items.len()).filter(|&i| (1..=floor as i16).contains(&data.items[i].level) && data.items[i].price > 0).collect();
            if !found.is_empty() && self.ch.items.len() < 40 {
                let item = found[self.ch.roll(found.len() as u32) as usize];
                self.ch.items.push(Held { item, equipped: false });
                self.say(format!("You find {}!", data.items[item].name));
            }
        }
    }

    /// Rescuers fetch the corpse: 250 x floor² gold, or everything you have.
    pub fn rescue_cost(&self) -> u64 {
        match self.ch.life {
            Life::Dead(p) => 250 * (p.floor as u64).pow(2),
            _ => 0,
        }
    }

    pub fn raise_cost(&self) -> u64 {
        100 + 50 * u64::from(self.ch.levels.iter().sum::<u32>()).pow(2)
    }

    pub fn rescue(&mut self) {
        let cost = self.rescue_cost().min(self.ch.gold);
        self.ch.gold -= cost;
        self.ch.life = Life::InMorgue;
        self.say(format!("Rescuers bring your body to the morgue for {cost} gold."));
    }

    /// Raising always works in the slice, but often costs a point of CON.
    pub fn raise(&mut self, data: &GameData) {
        let cost = self.raise_cost().min(self.ch.gold);
        self.ch.gold -= cost;
        self.ch.life = Life::Alive;
        self.ch.hp = self.ch.max_hp;
        let min = data.races[self.ch.race].stat_min[CON];
        if self.ch.chance(0.5) && self.ch.stats[CON] > min {
            self.ch.stats[CON] -= 1;
            self.say(format!("You are raised for {cost} gold, but lose a point of CON."));
        } else {
            self.say(format!("You are raised for {cost} gold."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = root.join("target/ddj-game-test");
        ddj_extract::import(&root.join("tests/fixtures/MORDOR11.ZIP"), &out).unwrap();
        GameData::load(&out).unwrap()
    }

    /// Human Nomads in the standard kit walk floor 1 (forward, turning at walls), fighting
    /// what they meet. Prints how they fared, as a rough check on the start values.
    #[test]
    fn plays_floor_one() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        // The HelpLesson's "standard attire for a beginning Nomad": 1,180 of the 1,500 gold.
        let kit = ["Bronze Sword", "Leather Armor", "Leather Gloves", "Leather Boots", "Cloth Cloak", "Wooden Shield", "Leather Cap"]
            .map(|n| stock(&d).into_iter().find(|&i| d.items[i].name == n).unwrap());
        let mut best = 0;
        for name in ["Ann", "Bo", "Cy", "Di", "Ed"] {
            let mut g = Game::new(Character::new(&d, name, 0, false, 1, start_stats(&d, 0)).unwrap());
            for (slot, item) in kit.into_iter().enumerate() {
                g.ch.buy(&d, item).unwrap();
                g.ch.toggle_equip(&d, slot).unwrap();
            }
            assert_eq!((g.ch.gold, g.ch.att_def(&d)), (320, (18, 22)));
            g.enter_dungeon(&d).unwrap();
            let mut steps = 0;
            while g.ch.life == Life::Alive && steps < 3000 {
                steps += 1;
                if g.fight.is_some() {
                    g.round(&d, &t);
                } else if g.log.last().is_some_and(|l| l.starts_with("A wall")) || g.ch.roll(8) == 0 {
                    let by = 1 + 2 * g.ch.roll(2) as u8;
                    g.turn(by);
                    g.log.push(String::new());
                } else {
                    g.step(&d);
                }
            }
            let seen = g.ch.seen[0].iter().filter(|&&s| s).count();
            eprintln!("{name}: {steps} steps, {} kills, {} XP, {seen} squares seen, {:?}", g.ch.kills, g.ch.xp[0], g.ch.life);
            best = best.max(g.ch.kills);
        }
        assert!(best >= 3);
    }

    #[test]
    fn levels_dies_and_is_raised() {
        let (d, t) = (data(), Tuning::load(&[]).unwrap());
        let mut g = Game::new(Character::new(&d, "Raise", 0, true, 1, start_stats(&d, 0)).unwrap());
        g.ch.add_xp(&d, &t, 10_000);
        assert_eq!(g.ch.xp[0], t.xp_needed(2, 3.0, 8.0) - 1, "pinned below level 3");
        g.ch.make_level(&d, &t).unwrap();
        assert_eq!(g.ch.level(), 2);
        assert!(g.ch.max_hp > 15 && g.ch.gold == 1500 - 39);

        g.enter_dungeon(&d).unwrap();
        let at = g.ch.pos;
        assert!(g.hurt(1000));
        assert_eq!(g.ch.life, Life::Dead(at));
        assert_eq!(g.rescue_cost(), 250);
        g.rescue();
        g.raise(&d);
        assert!(g.ch.life == Life::Alive && g.ch.hp == g.ch.max_hp);

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ddj-save-test");
        g.ch.save(&dir).unwrap();
        let back = Character::load(&dir.join("Raise.json")).unwrap();
        assert_eq!((back.levels, back.gold, back.stats), (g.ch.levels.clone(), g.ch.gold, g.ch.stats));
        assert!(saved(&dir).contains(&"Raise".to_string()));
    }
}
