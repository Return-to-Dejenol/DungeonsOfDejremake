//! The AutoMap as MORDOR.EXE keeps it: what the character playing has worked out of each
//! square (sub0835, mdrmazPerceiveSquare, mdrmazPerceiveAhead), and being lost.

use crate::game::{Character, Game, WIS, ahead, side, Side};
use crate::mdr::{Floor, GameData, square::*};

/// Map bit 23: the square was visited (the AutoMap fills it black).
pub const VISITED: u32 = 1 << 23;
/// The special squares a perception roll may wrongly name: facing, extinguish, rotator,
/// anti-magic and stud (sub087C's Select Case).
const GUESSES: [u32; 8] = [6, 7, 8, 9, 10, 17, 18, 22];
/// Text ids of the square names (gv194E), by flag bit, for the bits a roll can name.
fn name(bit: u32) -> u32 {
    match bit {
        6 => 0xe9c0_d4d7,
        7 => 0xcf8e_ea41,
        8 => 0x2d62_94d5,
        9 => 0x78e7_b5c3,
        10 => 0x9601_b28d,
        17 => 0xf3b4_bd3a,
        18 => 0x9029_a013,
        _ => 0x5399_1b83,
    }
}

/// Lost, as the original's three flags; `floor` is the level the AutoMap shows while the
/// level is lost (the last one known).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lost {
    pub pos: bool,
    pub level: bool,
    pub dir: bool,
    pub floor: usize,
}

impl Lost {
    pub fn any(&self) -> bool {
        self.pos || self.level || self.dir
    }
}

/// Records what entering (x, y) shows (sub0835): its walls and doors unless it is fog,
/// pits, stairs, water, quicksand, fog and chutes, the map's edge, and the walls and doors
/// of the squares west and south. A secret door not yet known is found when `found()`,
/// else it goes down as a wall.
pub fn record(f: &Floor, map: &mut [u32], x: usize, y: usize, mut found: impl FnMut() -> bool) {
    let flags = f.square(x, y).flags;
    let fog = flags & FOG != 0;
    let mut sides = |v: &mut u32, fl: u32, wall: u32, door: u32, secret: u32| {
        if fog {
            return;
        }
        *v |= fl & (wall | door);
        if fl & secret != 0 && *v & secret == 0 {
            if found() { *v = (*v | secret) & !wall } else { *v |= wall }
        }
    };
    let i = y * f.width + x;
    let mut v = map[i];
    sides(&mut v, flags, RIGHT_WALL, RIGHT_DOOR, RIGHT_SECRET_DOOR);
    sides(&mut v, flags, UPPER_WALL, UPPER_DOOR, UPPER_SECRET_DOOR);
    v |= flags & (PIT | STAIRS_UP | STAIRS_DOWN | WATER | QUICKSAND | FOG | CHUTE);
    if !fog && x + 1 == f.width {
        v |= RIGHT_WALL;
    }
    if !fog && y + 1 == f.height {
        v |= UPPER_WALL;
    }
    map[i] = v | VISITED;
    if x > 0 {
        sides(&mut map[i - 1], f.square(x - 1, y).flags, RIGHT_WALL, RIGHT_DOOR, RIGHT_SECRET_DOOR);
    }
    if y > 0 {
        sides(&mut map[i - f.width], f.square(x, y - 1).flags, UPPER_WALL, UPPER_DOOR, UPPER_SECRET_DOOR);
    }
}

impl Character {
    /// Perception (fn11D4, ability 5): (rate + ln L / ln 1.25) x rate x 0.3 from the best
    /// guild, plus WIS and a quarter of its worn modifier, less 2 per floor; 5 to 90.
    pub fn perception(&self, d: &GameData) -> f64 {
        let guild = self.guild_best(|g, l| {
            let r = f64::from(Self::guild_ability(d, g, 5));
            (r + f64::from(l).ln() / 1.25f64.ln()) * r * 0.3
        });
        let wis = f64::from(self.stats[WIS]) + f64::from(self.mods(d)[WIS]) / 4.0;
        (guild + wis - 2.0 * self.pos.floor as f64).clamp(5.0, 90.0)
    }

    /// Saves from before version 5 kept only the squares seen: each becomes visited, with
    /// what entering it shows (secret doors as walls).
    pub fn upgrade_map(&mut self, d: &GameData) {
        for (fi, seen) in std::mem::take(&mut self.seen).into_iter().enumerate() {
            let Some(f) = d.floors.get(fi) else { break };
            let map = self.map_of(d, fi + 1);
            for i in (0..seen.len().min(map.len())).filter(|&i| seen[i]) {
                record(f, map, i % f.width, i / f.width, || false);
            }
        }
    }

    /// This character's map of `floor`, sized to it.
    pub(crate) fn map_of(&mut self, d: &GameData, floor: usize) -> &mut Vec<u32> {
        let f = &d.floors[floor - 1];
        if self.map.len() < d.floors.len() {
            self.map.resize(d.floors.len(), Vec::new());
        }
        let m = &mut self.map[floor - 1];
        m.resize(f.width * f.height, 0);
        m
    }
}

impl Game {
    /// What the character playing knows of (x, y) on this floor.
    pub fn known(&self, d: &GameData, x: usize, y: usize) -> u32 {
        let Some(f) = self.floor(d) else { return 0 };
        self.lead().map.get(self.pos().floor - 1).and_then(|m| m.get(y * f.width + x)).copied().unwrap_or(0)
    }

    /// Adds `bits` to (x, y) on this floor's map; nothing goes down while lost (fn0ABD).
    pub(crate) fn learn(&mut self, d: &GameData, x: usize, y: usize, bits: u32) {
        let floor = self.pos().floor;
        if floor == 0 || self.lost.any() {
            return;
        }
        let w = d.floors[floor - 1].width;
        self.lead_mut().map_of(d, floor)[y * w + x] |= bits;
    }

    /// The living member with the best perception, and its value.
    fn keenest(&self, d: &GameData) -> (usize, f64) {
        self.living().into_iter().map(|i| (i, self.party[i].perception(d))).fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a })
    }

    /// Entering a square (sub0835): finding the way when lost, then the map.
    pub(crate) fn map_arrival(&mut self, d: &GameData) {
        let Some(f) = self.floor(d) else { return };
        let p = self.pos();
        let sharp = self.lead().perception(d);
        if self.lost.pos || self.lost.level {
            let q = sharp.ln().powi(2);
            if self.lost.level && q / 15.0 > self.party[0].rnd() * 100.0 {
                self.lost.level = false;
                self.say(d.t(0xb7d3_7534));
            }
            if self.lost.dir && q / 20.0 > self.party[0].rnd() * 100.0 {
                self.lost.dir = false;
                self.say(d.t(0x4848_a458));
            }
            if q / 25.0 > self.party[0].rnd() * 100.0 {
                self.lost = Lost::default();
                self.say(d.t(0x1044_9d31));
            }
        }
        // A deeper level than this character has been to (sub0835's header check).
        if !self.lost.level && p.floor > self.lead().deepest {
            self.party[0].deepest = p.floor;
            let line = format!("\"{}\"{}{}.", self.lead().name, d.t(0x6192_afdd), p.floor);
            let g = self.lead().guild;
            self.hall.note(g, line);
        }
        if self.known(d, p.x, p.y) & VISITED != 0 {
            if self.lost.pos || self.lost.level {
                self.say(d.t(0x0573_bda8));
            }
            self.lost = Lost::default();
        }
        if !self.lost.any() {
            let lead = &mut self.party[0];
            let mut map = std::mem::take(lead.map_of(d, p.floor));
            record(f, &mut map, p.x, p.y, || lead.roll(100) as f64 * 5.0 < sharp);
            *lead.map_of(d, p.floor) = map;
        }
        let flags = f.square(p.x, p.y).flags;
        for bit in [18, 22, 10, 6, 7, 8, 9, 17] {
            if flags & (1 << bit) != 0 {
                self.perceive_square(d, bit, false);
            }
        }
    }

    /// A special square noticed (mdrmazPerceiveSquare): the party's best perception P
    /// notices it P% of the time and names it rightly half of those (the teleporter always,
    /// `forced` too), else names another special square, as surely as P says.
    pub(crate) fn perceive_square(&mut self, d: &GameData, bit: u32, forced: bool) {
        let p = self.pos();
        if !forced && self.lead().panel.no_perception {
            return;
        }
        let (who, sharp) = if forced { (0, 0.0) } else { self.keenest(d) };
        let tele = bit == 14;
        if !forced && f64::from(self.party[0].d100()) > sharp && !tele {
            return;
        }
        if self.known(d, p.x, p.y) & (1 << bit) != 0 && !tele {
            return;
        }
        let mut sure = false;
        let named = if f64::from(self.party[0].d100()) <= sharp / 2.0 || tele || forced {
            if !self.lost.pos && !self.lost.level {
                sure = true;
                self.learn(d, p.x, p.y, 1 << bit);
            }
            bit
        } else {
            loop {
                let n = self.party[0].roll(23);
                if GUESSES.contains(&n) && n != bit {
                    break n;
                }
            }
        };
        if forced {
            return;
        }
        let alone = self.party.len() == 1;
        if tele {
            return self.say(d.t(if alone { 0x6e7f_7df7 } else { 0xd5ce_1bdb }));
        }
        let word = match sharp as i32 {
            _ if sure => 0x3e7b_4329,
            ..=39 => 0x0eba_69ec,
            40..=49 => 0x0c0b_271c,
            50..=59 => 0x0366_03c1,
            60..=69 => 0x54d2_5349,
            70..=85 => 0xed35_4321,
            86..=95 => 0x93c4_fcc3,
            _ => 0x3bdf_7b2b,
        };
        let line = match (sharp <= 35.0, alone) {
            (true, true) => d.t(0x3c78_91e7).to_string(),
            (true, false) => format!("{}{}", self.quoted(who), d.t(0xdb17_e3dd)),
            (false, true) => format!("{}{}{}{}{}", d.t(0xb516_ab72), d.t(word), d.t(0xa1da_187a), d.t(name(named)), d.t(0x9bb8_a372)),
            (false, false) => format!("{}{}{}{}{}{}", self.quoted(who), d.t(0x9ef2_f919), d.t(word), d.t(0x9d32_b17e), d.t(name(named)), d.t(0x9bb8_a372)),
        };
        self.say(line);
    }

    /// A rotator may leave the leader without a sense of direction: perception below
    /// Rnd x 70 + 2 x floor (sub0835).
    pub(crate) fn rotator_lost(&mut self, d: &GameData) {
        let floor = self.pos().floor as f64;
        if self.lead().perception(d) < self.party[0].rnd() * 70.0 + floor * 2.0 {
            self.lost.pos = true;
            self.lost.dir = true;
            self.say(d.t(if self.party.len() > 1 { 0xc10c_5184 } else { 0x9ee7_83ac }));
        }
    }

    /// After a teleporter, chute or trap: landing on a square not on the map leaves the
    /// party completely lost (sub08F5, sub08E1). `from` is the floor left.
    pub(crate) fn landed(&mut self, d: &GameData, from: usize, tell: bool) {
        let p = self.pos();
        if p.floor > 0 && !self.lost.any() && self.known(d, p.x, p.y) == 0 {
            self.lost = Lost { pos: true, level: true, dir: true, floor: from };
            if tell {
                self.say(d.t(0xccab_2048));
            }
        }
    }

    /// Perceive Ahead (each redraw of the maze): the party's best perception, at most 95,
    /// is the chance to spot each of quicksand, stairs, a teleporter, chute, pit, rotator
    /// or extinguish square in the square ahead, unless a wall or door is in the way.
    pub(crate) fn perceive_ahead(&mut self, d: &GameData) {
        let Some(f) = self.floor(d) else { return };
        if self.living().is_empty() || self.lead().panel.no_perception {
            return;
        }
        let p = self.pos();
        let Some((x, y)) = ahead(f, p.x, p.y, p.facing).filter(|_| side(f, p.x, p.y, p.facing) == Side::Open) else { return };
        let (who, sharp) = self.keenest(d);
        let sharp = sharp.min(95.0);
        let (flags, here) = (f.square(x, y).flags, f.square(p.x, p.y).flags);
        let mut seen = 0;
        for (bit, you, named) in [
            (QUICKSAND, 0x7af2_ad63, 0xb100_fd71),
            (STAIRS_UP, 0x6fcf_5fc5, 0xbc28_d43f),
            (STAIRS_DOWN, 0x4666_4d6c, 0x8c16_5966),
            (TELEPORTER, 0xfa62_2109, 0x6dfc_422f),
            (CHUTE, 0xd9c6_c890, 0x6ae2_d7ae),
            (PIT, 0x6d9f_b5d8, 0xb797_f9e2),
            (ROTATOR, 0xae57_bb18, 0x43bc_bd4a),
            (EXTINGUISH, 0xb08b_5268, 0x7821_cf7e),
        ] {
            if flags & bit != 0 && self.party[0].rnd() * 100.0 < sharp {
                seen |= bit;
                if bit == QUICKSAND && here & QUICKSAND != 0 {
                    continue;
                }
                let line = if self.party.len() == 1 { d.t(you).to_string() } else { format!("{}{}", self.quoted(who), d.t(named)) };
                self.say(line);
            }
        }
        if seen != 0 {
            self.learn(d, x, y, seen);
        }
    }
}
