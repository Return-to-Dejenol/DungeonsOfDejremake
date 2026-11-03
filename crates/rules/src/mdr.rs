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

/// Alignment bitmask: 1 good, 2 neutral, 4 evil.
pub type Alignments = i16;

#[derive(Debug)]
pub struct Race {
    pub name: String,
    /// STR, INT, WIS, CON, CHA, DEX.
    pub stat_min: [i16; 6],
    pub stat_max: [i16; 6],
    /// Percentages; which resistance each slot is has not been confirmed.
    pub resist: [i16; 12],
    pub alignments: Alignments,
    pub lifespan: i16,
    /// Probable: experience modifier (Human 3, Troll 9).
    pub xp_factor: f32,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Guild {
    pub name: String,
    /// Probable: experience modifier (Nomad 8, Wizard 30).
    pub xp_factor: f32,
    /// STR, INT, WIS, CON, CHA, DEX needed to join.
    pub stat_req: [i16; 6],
    pub alignments: Alignments,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Item {
    pub name: String,
    pub id: i16,
    pub price: u32,
    /// Bit n set: guild n (in `GameData::guilds` order) may use it.
    pub guilds: u16,
    /// Index into `GameData::item_types`.
    pub kind: i16,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Monster {
    pub name: String,
    pub id: i16,
    /// Probable: shallowest dungeon level it appears on.
    pub level: i16,
    pub rest: Vec<u8>,
}

#[derive(Debug)]
pub struct Spell {
    pub name: String,
    pub id: i16,
    /// Probable: guild level needed to learn it.
    pub level: i16,
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
            c.skip(3);
            let xp_factor = c.f32();
            c.skip(1);
            let stat_req = c.words();
            c.skip(1);
            Guild { name, xp_factor, stat_req, alignments: c.i16(), rest: c.rest(start) }
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
            let id = c.i16();
            c.skip(2);
            let price = c.u32();
            c.skip(10);
            let guilds = c.i16() as u16;
            c.skip(7);
            Item { name, id, price, guilds, kind: c.i16(), rest: c.rest(start) }
        }).collect();
        let mo = Mdr::new(m5, 160)?;
        let monsters = rows(&mo, 2, 3).into_iter().map(|i| {
            let mut c = mo.rec(i);
            let name = c.str();
            let start = c.p;
            c.skip(2);
            let id = c.i16();
            c.skip(4);
            Monster { name, id, level: c.i16(), rest: c.rest(start) }
        }).collect();
        let sp = Mdr::new(m2, 75)?;
        let spells = rows(&sp, 1, 2).into_iter().map(|i| {
            let mut c = sp.rec(i);
            let name = c.str();
            let start = c.p;
            let id = c.i16();
            c.skip(1);
            Spell { name, id, level: c.i16(), rest: c.rest(start) }
        }).collect();
        // MDATA11 has no version: record 0 is the floor count, then 1-based
        // record numbers of each floor's header.
        let du = Mdr { b: m11, size: 20 };
        let floors = (1..=du.count(0)).map(|i| Floor::parse(&du, du.count(i).saturating_sub(1))).collect();

        Ok(Self { races, guilds, item_types, item_classes, monster_types, monster_families, items, monsters, spells, floors })
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

        assert_eq!(d.items.len(), 366);
        let dagger = d.items.iter().find(|i| i.name == "Bronze Dagger").unwrap();
        assert_eq!((dagger.price, dagger.guilds, d.item_types[dagger.kind as usize].as_str()), (250, 4095, "Dagger"));
        let mut ids: Vec<_> = d.items.iter().map(|i| i.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 366);

        assert_eq!(d.monsters.len(), 401);
        assert_eq!(d.monsters.iter().find(|m| m.name == "Troll").unwrap().level, 3);
        assert_eq!(d.spells.len(), 105);
        assert_eq!((d.spells[0].name.as_str(), d.spells[104].name.as_str()), ("Charm of Opening", "Shock Lance"));
    }
}
