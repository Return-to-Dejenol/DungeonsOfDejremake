//! Readers for the original game's `MDATA*.MDR` tables. Layout notes are in
//! `docs/formats.md`; fields documented as "probable" are inferred from value
//! patterns only. Every record keeps its undecoded bytes in `rest`.

use std::{fs, io, path::Path};

/// A Visual Basic random-access file: fixed-size records, record 0 holds the
/// version string, and bytes past a record's fields are leftover disk junk.
struct Mdr<'a> {
    b: &'a [u8],
    size: usize,
}

impl<'a> Mdr<'a> {
    fn new(b: &'a [u8], size: usize) -> io::Result<Self> {
        let m = Self { b, size };
        match m.rec(0).str().as_str() {
            "1.1" => Ok(m),
            v => Err(io::Error::other(format!("unsupported MDR version {v:?}"))),
        }
    }

    fn rec(&self, i: usize) -> Cur<'a> {
        let s = (i * self.size).min(self.b.len());
        Cur { b: &self.b[s..(s + self.size).min(self.b.len())], p: 0 }
    }

    /// Header records begin with an Integer count, written over junk.
    fn count(&self, i: usize) -> usize {
        self.rec(i).i16().max(0) as usize
    }
}

struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0; N];
        if let Some(s) = self.b.get(self.p..self.p + N) {
            out.copy_from_slice(s);
        }
        self.p += N;
        out
    }
    fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.take())
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }
    fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take())
    }
    /// VB Currency: an i64 scaled by 10,000.
    fn currency(&mut self) -> i64 {
        i64::from_le_bytes(self.take()) / 10_000
    }
    fn skip(&mut self, words: usize) {
        self.p += 2 * words;
    }
    fn words<const N: usize>(&mut self) -> [i16; N] {
        std::array::from_fn(|_| self.i16())
    }
    /// Length-prefixed Latin-1 string.
    fn str(&mut self) -> String {
        let n = self.i16().max(0) as usize;
        let s = self.b.get(self.p..(self.p + n).min(self.b.len())).unwrap_or(&[]);
        self.p += n;
        s.iter().map(|&c| char::from(c)).collect()
    }
    /// Everything after the name, as stored, for fields not decoded yet.
    fn rest(&self, from: usize) -> Vec<u8> {
        self.b.get(from..).unwrap_or(&[]).to_vec()
    }
}

/// Raw words of a record (`rest` starts at w0, after the name), for the fields the
/// original's code reads by number: `w(9)` is w9, `long(35)` the Long at w35-36.
pub trait Words {
    fn raw(&self) -> &[u8];
    fn w(&self, n: usize) -> i16 {
        self.raw().get(2 * n..2 * n + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
    }
    fn long(&self, n: usize) -> i32 {
        self.raw().get(2 * n..2 * n + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn single(&self, n: usize) -> f32 {
        f32::from_bits(self.long(n) as u32)
    }
}

macro_rules! words {
    ($($t:ty),*) => {$(impl Words for $t {
        fn raw(&self) -> &[u8] {
            &self.rest
        }
    })*};
}
words!(Race, Guild, Item, Monster);

/// Alignment bitmask: 1 good, 2 neutral, 4 evil.
pub type Alignments = i16;

#[derive(Debug)]
pub struct Race {
    pub name: String,
    /// STR, INT, WIS, CON, CHA, DEX.
    pub stat_min: [i16; 6],
    pub stat_max: [i16; 6],
    /// Percentages: fire, cold, electric, mind, disease, poison, magic, stone,
    /// paralysis, drain, acid, then an unused slot.
    pub resist: [i16; 12],
    pub alignments: Alignments,
    pub lifespan: i16,
    /// Experience modifier (Human 3, Troll 9).
    pub xp_factor: f32,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Guild {
    pub name: String,
    /// Average HP per level up to `hp_cap` levels, then `hp_after` per level.
    pub hp_per_level: i16,
    pub hp_cap: i16,
    pub hp_after: i16,
    /// Experience modifier (Nomad 8, Wizard 30).
    pub xp_factor: f32,
    /// STR, INT, WIS, CON, CHA, DEX needed to join.
    pub stat_req: [i16; 6],
    pub alignments: Alignments,
    /// Thieving, backstab, critical hit, multiple swings, fighting and perception, as
    /// the refcard shows them /10 (Warrior fighting 1.2, Nomad 0.7).
    pub abilities: [f32; 6],
    /// Per spell class (see [`Spell::class`]): guild levels per spell level, 0 = not
    /// taught. A spell is learned at guild level `spell.level x rate`.
    pub spell_rates: [f32; 18],
    /// Bit n set: race n (in `GameData::races` order) may join.
    pub races: u16,
    /// Tenths added to the gold for making a level (w111).
    pub gold_factor: i16,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Item {
    pub name: String,
    pub id: i16,
    /// Probable: attack and defense when equipped.
    pub attack: i16,
    pub defense: i16,
    pub price: u32,
    /// Probable: deepest-first floor it is found on; 0 means sold in the store only.
    pub level: i16,
    /// Swings a weapon adds per round (w9, the bracketed number in item info); 0 for
    /// non-weapons.
    pub power: i16,
    /// Bit n set: guild n (in `GameData::guilds` order) may use it.
    pub guilds: u16,
    /// Probable: damage multiplier.
    pub damage_mult: f32,
    /// Index into `GameData::item_types`.
    pub kind: i16,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Monster {
    pub name: String,
    pub id: i16,
    /// Probable: attack, defense and average hit points.
    pub attack: i16,
    pub defense: i16,
    pub hp: i16,
    /// Resistance percentages, in the race order (fire, cold, electric, mind, disease,
    /// poison, magic, stone, paralysis, drain, acid, unused).
    pub resist: [i16; 12],
    /// How many can appear together.
    pub group: i16,
    /// Special attacks (w22), bits as MORDOR.EXE names them: 1 poison, 2 disease, 4
    /// paralyze, 8 fire breath, 16 cold breath, 32 acid, 64 electrocute, 128 drain, 256
    /// stone, 512 age, 1024 critical hit, 2048 backstab, 4096 destroy item, 8192 steal.
    pub specials: u16,
    /// Flags (w20): 1 sees invisible, 2 invisible, 4 resists all magic, 8 resists
    /// charms, 16 resists weapons, 32 immune to weapons.
    pub flags: i16,
    /// The spell classes it casts (bit n = class n), at level `power_level / 4`.
    pub spells: u16,
    /// Spawn weight within its monster type (w26; Clean-Ups 66).
    pub weight: i16,
    /// Percent chances of leaving a box (w28) or a chest (w29); MORDOR.EXE rolls both
    /// against one number, so the chest wins. w27 is unused.
    pub box_chance: i16,
    pub chest_chance: i16,
    /// Percent chance a container holds an item (w66), then a tenth of it for each more.
    pub item_chance: i16,
    /// Gold exponent (w33): a group's gold grows as 10^w33.
    pub gold_exp: i16,
    /// Bit n set: the container can carry trap n (w35-36; bit 0 = not trapped).
    pub traps: u32,
    /// The level used for damage, traps and companion prices (w37).
    pub power_level: i16,
    /// STR, INT, WIS, CON, CHA, DEX and a spare (w38-44).
    pub stats: [i16; 7],
    /// Item classes its treasure is drawn from (w51-60): -1 any, below -1 one item.
    pub item_classes: [i16; 10],
    /// Highest item level it leaves (w65).
    pub item_level: i16,
    /// 1-based tile in the `MONS` bitmap of MG11.DLL (60x60, ten per row).
    pub picture: i16,
    /// Dungeon level it appears on (lairs on each floor hold monsters of that level).
    pub level: i16,
    /// Index into `GameData::monster_types`; rooms pick spawns by this.
    pub kind: i16,
    pub rest: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Spell {
    pub name: String,
    pub id: i16,
    /// The help's spell classes: 0 fire, 1 cold, 2 electric, 3 mind, 4 damage, 5 element,
    /// 6 kill, 7 charm, 8 bind, 9 heal, 10 movement, 11 banish, 12 dispel, 13 resistant,
    /// 14 visual, 15 magical, 16 location, 17 protection.
    pub class: i16,
    /// Spell level; a guild teaches it at `level x` the guild's rate for the class.
    pub level: i16,
    pub sp: i16,
    /// Monsters affected, and groups affected.
    pub targets: i16,
    pub groups: i16,
    /// Damage or healing: `base/per_level` in the help's spell info.
    pub base: i16,
    pub per_level: i16,
    /// What it does beyond damage: 100 a mind or death effect, 75-86 the monster type a
    /// charm, banish or dispel works on, 200+ a mass (whole party) spell, other values
    /// the specific effect (cures, travel, resistances; see `magic.rs`).
    pub effect: i16,
    /// STR, INT, WIS, CON, CHA, DEX needed to cast.
    pub stat_req: [i16; 6],
    /// Resistance slot that weakens it (race/monster order), or -1.
    pub resist: i16,
    pub rest: Vec<u8>,
}

/// Square flags, in the order of the original map editor's checkboxes.
/// "Right" is +x (east) and "upper" is +y (north).
pub mod square {
    pub const RIGHT_WALL: u32 = 1 << 0;
    pub const UPPER_WALL: u32 = 1 << 1;
    pub const RIGHT_DOOR: u32 = 1 << 2;
    pub const UPPER_DOOR: u32 = 1 << 3;
    pub const RIGHT_SECRET_DOOR: u32 = 1 << 4;
    pub const UPPER_SECRET_DOOR: u32 = 1 << 5;
    pub const FACE_NORTH: u32 = 1 << 6;
    pub const FACE_EAST: u32 = 1 << 7;
    pub const FACE_SOUTH: u32 = 1 << 8;
    pub const FACE_WEST: u32 = 1 << 9;
    pub const EXTINGUISH: u32 = 1 << 10;
    pub const PIT: u32 = 1 << 11;
    pub const STAIRS_UP: u32 = 1 << 12;
    pub const STAIRS_DOWN: u32 = 1 << 13;
    pub const TELEPORTER: u32 = 1 << 14;
    pub const WATER: u32 = 1 << 15;
    pub const QUICKSAND: u32 = 1 << 16;
    pub const ROTATOR: u32 = 1 << 17;
    pub const ANTI_MAGIC: u32 = 1 << 18;
    pub const ROCK: u32 = 1 << 19;
    pub const FOG: u32 = 1 << 20;
    pub const CHUTE: u32 = 1 << 21;
    /// Labelled "Stud" in the editor; meaning unknown.
    pub const STUD: u32 = 1 << 22;
}

#[derive(Debug)]
pub struct Square {
    /// Index into `Floor::areas`; 0 is open corridor.
    pub area: i16,
    /// `square::*` bits.
    pub flags: u32,
}

/// A room. Its squares share an area number.
#[derive(Debug)]
pub struct Area {
    /// Bit n set: `GameData::monster_types[n]` may spawn here.
    pub monster_types: u32,
    /// Monster id of the room's lair occupant, or 0.
    pub lair: i16,
}

/// Positions are 0-based (x east, y north); floors are 1-based levels.
#[derive(Debug)]
pub struct Teleporter {
    pub at: (usize, usize),
    /// Probable: `None` sends the party to a random square, as does a
    /// teleporter square with no record.
    pub to: Option<(usize, usize, i16)>,
}

#[derive(Debug)]
pub struct Chute {
    pub at: (usize, usize),
    /// Floors fallen.
    pub drop: i16,
}

#[derive(Debug)]
pub struct Floor {
    pub level: i16,
    pub width: usize,
    pub height: usize,
    /// Row-major from the south-west corner.
    pub squares: Vec<Square>,
    pub areas: Vec<Area>,
    pub teleporters: Vec<Teleporter>,
    pub chutes: Vec<Chute>,
}

impl Floor {
    pub fn square(&self, x: usize, y: usize) -> &Square {
        &self.squares[y * self.width + x]
    }

    /// One floor of `MDATA11`, starting at its header record.
    fn parse(m: &Mdr<'_>, first: usize) -> Self {
        let [width, height, level] = m.rec(first).words();
        let (width, height) = (width.max(0) as usize, height.max(0) as usize);
        let mut next = first + 1;
        let mut take = |n: usize| {
            let r = next..next + n;
            next += n;
            r
        };
        let squares = take(width * height).map(|i| {
            let mut c = m.rec(i);
            Square { area: c.i16(), flags: c.currency() as u32 }
        }).collect();
        // Areas in use, then a fixed 201 slots (VB `Dim a(200)`).
        let used = m.count(take(1).start);
        let areas = take(201).take(used + 1).map(|i| {
            let mut c = m.rec(i);
            Area { monster_types: c.u32(), lair: c.i16() }
        }).collect();
        // Slot count, then slots; unused slots have x = 0. Coordinates are 1-based.
        let mut slots = || {
            let n = m.count(take(1).start);
            take(n).map(|i| m.rec(i).words::<5>()).filter(|w| w[0] > 0).collect::<Vec<_>>()
        };
        let at = |w: &[i16]| ((w[0] - 1) as usize, (w[1] - 1).max(0) as usize);
        let teleporters = slots().iter().map(|w| Teleporter {
            at: at(w),
            to: (w[4] > 0).then(|| ((w[2] - 1).max(0) as usize, (w[3] - 1).max(0) as usize, w[4])),
        }).collect();
        let chutes = slots().iter().map(|w| Chute { at: at(w), drop: w[2] }).collect();
        Self { level, width, height, squares, areas, teleporters, chutes }
    }
}

#[derive(Debug)]
pub struct GameData {
    pub races: Vec<Race>,
    pub guilds: Vec<Guild>,
    pub item_types: Vec<String>,
    pub item_classes: Vec<String>,
    pub monster_types: Vec<String>,
    /// A 15-entry variant of `monster_types`; purpose unknown.
    pub monster_families: Vec<String>,
    pub items: Vec<Item>,
    pub monsters: Vec<Monster>,
    pub spells: Vec<Spell>,
    pub floors: Vec<Floor>,
}

impl GameData {
    /// Reads the tables from an imported `gamedata/` folder.
    pub fn load(dir: &Path) -> io::Result<Self> {
        let f = |n: &str| fs::read(dir.join(n));
        Self::parse(&f("MDATA1.MDR")?, &f("MDATA2.MDR")?, &f("MDATA3.MDR")?, &f("MDATA5.MDR")?, &f("MDATA11.MDR")?)
    }

    pub fn parse(m1: &[u8], m2: &[u8], m3: &[u8], m5: &[u8], m11: &[u8]) -> io::Result<Self> {
        // MDATA1: six counts in records 1-6, then the tables back to back.
        let t = Mdr::new(m1, 260)?;
        let counts: Vec<usize> = (1..=6).map(|i| t.count(i)).collect();
        let mut next = 7;
        let mut section = |n: usize| {
            let r = next..next + n;
            next += n;
            r
        };
        let races = section(counts[0]).map(|i| {
            let mut c = t.rec(i);
            let name = c.str();
            let start = c.p;
            let stat_min = c.words();
            c.skip(1);
            let stat_max = c.words();
            c.skip(1);
            let resist = c.words();
            let alignments = c.i16();
            c.skip(3);
            let lifespan = c.i16();
            Race { name, stat_min, stat_max, resist, alignments, lifespan, xp_factor: c.f32(), rest: c.rest(start) }
        }).collect();
        let guilds = section(counts[1]).map(|i| {
            let mut c = t.rec(i);
            let name = c.str();
            let start = c.p;
            let [hp_per_level, hp_cap, hp_after] = c.words();
            let xp_factor = c.f32();
            c.skip(1);
            let stat_req = c.words();
            c.skip(1);
            let alignments = c.i16();
            c.skip(1);
            let abilities = std::array::from_fn(|_| c.f32());
            c.skip(6);
            let spell_rates = std::array::from_fn(|_| c.f32());
            c.skip(40);
            let races = c.i16() as u16;
            c.skip(1);
            let gold_factor = c.i16();
            Guild { name, hp_per_level, hp_cap, hp_after, xp_factor, stat_req, alignments, abilities, spell_rates, races, gold_factor, rest: c.rest(start) }
        }).collect();
        let mut names = |n: usize| section(n).map(|i| t.rec(i).str()).collect::<Vec<_>>();
        let (item_types, item_classes, monster_types, monster_families) =
            (names(counts[2]), names(counts[3]), names(counts[4]), names(counts[5]));

        // Other files: record 0 version, then counts; data from `first`.
        let rows = |m: &Mdr<'_>, count_rec: usize, first: usize| (first..first + m.count(count_rec)).collect::<Vec<_>>();
        let it = Mdr::new(m3, 125)?;
        let items = rows(&it, 2, 3).into_iter().map(|i| {
            let mut c = it.rec(i);
            let name = c.str();
            let start = c.p;
            let [id, attack, defense] = c.words();
            let price = c.u32();
            let level = c.i16();
            c.skip(3);
            let power = c.i16();
            c.skip(5);
            let guilds = c.i16() as u16;
            c.skip(2);
            let damage_mult = c.f32();
            c.skip(3);
            Item { name, id, attack, defense, price, level, power, guilds, damage_mult, kind: c.i16(), rest: c.rest(start) }
        }).collect();
        let mo = Mdr::new(m5, 160)?;
        let monsters = rows(&mo, 2, 3).into_iter().map(|i| {
            let mut c = mo.rec(i);
            let name = c.str();
            let start = c.p;
            let [attack, defense, id, hp, _, picture] = c.words();
            c.skip(1);
            let level = c.i16();
            let resist = c.words();
            let [flags, _, specials, _, spells, _, weight, _, box_chance, chest_chance, _, _, group, gold_exp, _] = c.words();
            let traps = c.u32();
            let power_level = c.i16();
            let stats = c.words();
            let kind = c.i16();
            c.skip(5);
            let item_classes = c.words();
            c.skip(4);
            let [item_level, item_chance] = c.words();
            Monster {
                name,
                id,
                attack,
                defense,
                hp,
                resist,
                group: group.max(1),
                specials: specials as u16,
                flags,
                spells: spells as u16,
                weight,
                box_chance,
                chest_chance,
                item_chance,
                gold_exp,
                traps,
                power_level,
                stats,
                item_classes,
                item_level,
                picture,
                level,
                kind,
                rest: c.rest(start),
            }
        }).collect();
        let sp = Mdr::new(m2, 75)?;
        let spells = rows(&sp, 1, 2).into_iter().map(|i| {
            let mut c = sp.rec(i);
            let name = c.str();
            let start = c.p;
            let [id, class, level, sp] = c.words();
            c.skip(2);
            let [targets, groups, base, per_level, effect] = c.words();
            let stat_req = c.words();
            c.skip(1);
            Spell { name, id, class, level, sp, targets, groups, base, per_level, effect, stat_req, resist: c.i16(), rest: c.rest(start) }
        }).collect();
        // MDATA11 has no version: record 0 is the floor count, then 1-based
        // record numbers of each floor's header.
        let du = Mdr { b: m11, size: 20 };
        let floors = (1..=du.count(0)).map(|i| Floor::parse(&du, du.count(i).saturating_sub(1))).collect();

        let mut d = Self { races, guilds, item_types, item_classes, monster_types, monster_families, items, monsters, spells, floors };
        crate::magic::add_remake_entries(&mut d);
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(src: &Path, test: &str) -> GameData {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target").join(test);
        ddj_extract::import(src, &out).unwrap();
        GameData::load(&out).unwrap()
    }

    fn shareware() -> GameData {
        import(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/MORDOR11.ZIP"), "ddj-rules-test")
    }

    /// Every teleporter and chute record sits on a square flagged for it, and
    /// every lair names a real monster.
    fn check_floors(d: &GameData) {
        for (i, f) in d.floors.iter().enumerate() {
            assert_eq!((f.level as usize, f.width, f.height), (i + 1, 30, 30));
            let flagged = |bit| f.squares.iter().filter(|s| s.flags & bit != 0).count();
            // Floor 8 has two teleporter squares with no record (random, probably).
            assert!(f.teleporters.len() <= flagged(square::TELEPORTER), "floor {}", f.level);
            assert_eq!(f.chutes.len(), flagged(square::CHUTE), "floor {}", f.level);
            assert!(f.teleporters.iter().all(|t| f.square(t.at.0, t.at.1).flags & square::TELEPORTER != 0));
            assert!(f.chutes.iter().all(|c| f.square(c.at.0, c.at.1).flags & square::CHUTE != 0));
            assert!(f.areas.iter().all(|a| a.lair == 0 || d.monsters.iter().any(|m| m.id == a.lair)));
        }
    }

    #[test]
    fn decodes_shareware_floors() {
        let d = shareware();
        assert_eq!(d.floors.len(), 3);
        check_floors(&d);
        let f = &d.floors[0];
        assert_eq!(f.areas.len(), 158);
        assert_eq!(f.squares.iter().filter(|s| s.flags & square::STAIRS_UP != 0).count(), 1);
        assert_eq!((f.chutes[0].at, f.chutes[0].drop), ((9, 29), 1));
        assert_eq!(f.teleporters[0].at, (1, 16));
        assert_eq!(d.monsters.iter().find(|m| m.id == f.areas[151].lair).unwrap().name, "Aboleth");
    }

    /// Set `DDJ_RETAIL` to a registered copy (folder or ZIP) to check all 15 floors.
    #[test]
    fn decodes_registered_floors() {
        let Ok(src) = std::env::var("DDJ_RETAIL") else { return };
        let d = import(Path::new(&src), "ddj-rules-retail");
        assert_eq!(d.floors.len(), 15);
        check_floors(&d);
    }

    #[test]
    fn decodes_shareware_tables() {
        let d = shareware();
        assert_eq!((d.races.len(), d.guilds.len(), d.item_types.len()), (9, 12, 35));
        assert_eq!((d.item_classes.len(), d.monster_types.len(), d.monster_families.len()), (24, 18, 15));
        let human = &d.races[0];
        assert_eq!((human.name.as_str(), human.stat_min, human.stat_max), ("Human", [4, 4, 4, 6, 5, 6], [17, 18, 18, 17, 18, 18]));
        assert_eq!((human.alignments, human.lifespan, human.xp_factor), (7, 100, 3.0));
        let paladin = d.guilds.iter().find(|g| g.name == "Paladin").unwrap();
        assert_eq!((paladin.alignments, paladin.stat_req), (1, [14, 9, 9, 10, 16, 15]));
        assert_eq!(d.guilds[0].xp_factor, 8.0);

        assert_eq!(d.items.len(), 366 + 2);
        let dagger = d.items.iter().find(|i| i.name == "Bronze Dagger").unwrap();
        assert_eq!((dagger.price, dagger.guilds, d.item_types[dagger.kind as usize].as_str()), (250, 4095, "Dagger"));
        let mut ids: Vec<_> = d.items.iter().map(|i| i.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 366 + 2);

        assert_eq!((dagger.attack, dagger.defense, dagger.level, dagger.power, dagger.damage_mult), (3, 0, 0, 1, 1.0));
        let warrior = &d.guilds[1];
        assert_eq!((warrior.hp_per_level, warrior.hp_cap, warrior.hp_after, warrior.abilities[4]), (6, 28, 2, 1.2));
        // Paladins learn Minor Heal (heal class, spell level 2) at guild level 6.
        let (paladin, minor) = (&d.guilds[2], d.spells.iter().find(|s| s.name == "Minor Heal").unwrap());
        assert_eq!((minor.class, minor.level, minor.base, minor.per_level), (9, 2, 5, 2));
        assert_eq!(f32::from(minor.level) * paladin.spell_rates[minor.class as usize], 6.0);
        let portal = d.spells.iter().find(|s| s.name == "Ethereal Portal").unwrap();
        assert_eq!(portal.stat_req, [14, 18, 18, 0, 0, 18]);
        // Giants (race 2) may join Nomad and Warrior; nobody is barred from Nomad.
        assert!(d.guilds[0].races == 511 && warrior.races & 4 != 0);

        assert_eq!(d.monsters.len(), 401);
        let troll = d.monsters.iter().find(|m| m.name == "Troll").unwrap();
        assert_eq!((troll.attack, troll.defense, troll.hp, troll.level), (86, 69, 35, 3));
        let kobold = d.monsters.iter().find(|m| m.name == "Kobold").unwrap();
        assert_eq!((kobold.picture, d.monster_types[kobold.kind as usize].as_str()), (2, d.monster_types[0].as_str()));
        assert_eq!((kobold.box_chance, kobold.chest_chance, kobold.item_chance, kobold.traps, kobold.power_level, kobold.stats[5]), (40, 5, 50, 31, 2, 7));
        assert_eq!(d.spells.len(), 105 + 1);
        assert_eq!((d.spells[0].name.as_str(), d.spells[104].name.as_str()), ("Charm of Opening", "Shock Lance"));
    }
}
