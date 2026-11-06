//! Boxes and chests as MORDOR.EXE runs them: which monsters leave them, their traps
//! (from the monster's trap list), what the party thinks the trap is, opening (which
//! disarms first, on thieving), the gold the group carried and the items it drew.

use crate::{
    Tuning,
    game::{Character, DEX, Game, INT, THIEVING, WIS},
    magic::{GREATER_POTION_OF_EXPERIENCE, POTION_OF_EXPERIENCE},
    mdr::{GameData, Monster, Words, square},
};

/// In MORDOR.EXE's numbering, so the monsters' trap lists apply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trap {
    None,
    PoisonGas,
    DiseaseSpores,
    Exploding,
    Fire,
    Slime,
    Fate,
    Teleporter,
    Blackout,
    Fear,
    Withering,
}

pub const TRAPS: [Trap; 11] =
    [Trap::None, Trap::PoisonGas, Trap::DiseaseSpores, Trap::Exploding, Trap::Fire, Trap::Slime, Trap::Fate, Trap::Teleporter, Trap::Blackout, Trap::Fear, Trap::Withering];

impl Trap {
    pub fn name(self) -> &'static str {
        ["Not Trapped", "Poison Gas", "Disease Spores", "Exploding", "Fire", "Slime", "Fate", "Teleporter", "Blackout", "Fear", "Withering"][self as usize]
    }
}

#[derive(Clone, Debug)]
pub struct Chest {
    /// A chest pays half again a box's gold.
    pub chest: bool,
    pub trap: Trap,
    /// What the party thinks the trap is, and how sure (1-4), as the original's bullets.
    pub guess: Trap,
    pub sure: u8,
    pub magic_lock: bool,
    /// Opened by Charm of Opening: such chests hold fewer items.
    pub unlocked: bool,
    /// The monster that left it (index into `GameData::monsters`).
    pub monster: usize,
    /// Its level found (w7), for messages and old callers.
    pub level: i16,
    /// The group's gold, set when the fight ends.
    pub gold: u64,
}

/// The container an encounter's monster leaves (MORDOR.EXE): one d99 roll against its
/// chest % (w29) and box % (w28), +25 in its lair. Chests are trapped 89% of the time,
/// boxes 74%, with a trap from the monster's list. Magic locks follow the remake's floor
/// table (rare on floors 1-5), and never on anti-magic or extinguish squares.
pub fn chest_for(g: &mut Game, d: &GameData, m: usize, lair: bool) -> Option<Chest> {
    let mon = &d.monsters[m];
    let floor = g.pos().floor as u32;
    let sq = g.floor(d).map_or(0, |f| f.square(g.pos().x, g.pos().y).flags);
    let r = &mut g.party[0];
    let roll = r.rnd() * 99.0;
    let b = if lair && mon.chest_chance > 0 { 25.0 } else { 0.0 };
    let chest = roll < f64::from(mon.chest_chance) + b;
    if !chest && roll >= f64::from(mon.box_chance) + b {
        return None;
    }
    let mut trap = Trap::None;
    if mon.traps > 0 && r.d100() < if chest { 90 } else { 75 } {
        let k = loop {
            let k = r.roll(12);
            if mon.traps >> k & 1 != 0 {
                break k as usize;
            }
        };
        trap = TRAPS.get(k).copied().unwrap_or(Trap::None);
    }
    let lock = Tuning::load(&[]).map(|t| t.magic_lock_chance(floor)).unwrap_or(0.0);
    let magic_lock = sq & (square::ANTI_MAGIC | square::EXTINGUISH) == 0 && r.chance(lock);
    let mut c = Chest { chest, trap, guess: trap, sure: 1, magic_lock, unlocked: false, monster: m, level: mon.level.max(1), gold: 0 };
    examine(&mut g.party[0], d, &mut c, floor);
    Some(c)
}

/// A group's gold (MORDOR.EXE): B = e x 10^(e - 0.5) / 4 for the monster's gold exponent
/// e, a quarter to five quarters of it times log2(level + 1) (500 at least), then
/// ln(n + 1)² / 2 x n / 2 for n monsters, and half again if magically locked.
pub fn gold(r: &mut Character, mon: &Monster, n: u32, locked: bool) -> u64 {
    let e = f64::from(mon.gold_exp);
    let b = (e * 10f64.powf(e - 0.5) / 4.0).round_ties_even();
    let mut one = ((b / 4.0 + r.rnd() * b / 2.0 + r.rnd() * b / 2.0) * (f64::from(mon.level) + 1.0).log2()).round_ties_even();
    if one < 500.0 {
        one = (r.rnd() * 500.0).floor() + 2.0;
    }
    let n = f64::from(n);
    let pot = one * (n + 1.0).ln().powi(2) / 2.0 * n / 2.0;
    (pot * if locked { 1.5 } else { 1.0 }) as u64
}

/// Thieving in percent against the container's monster (MORDOR.EXE): WIS / 4 + DEX / 2,
/// plus (4.5 + (L + 1)^(1 / ln 15)) x rate x 1.4 for the best guild, less
/// 9.375 x ln(monster level) x ln(floor + 1); 5-98.
pub fn thieving(c: &Character, d: &GameData, mlevel: i16, floor: u32) -> f64 {
    let s = f64::from(c.stats[WIS]) / 4.0 + f64::from(c.stats[DEX]) / 2.0;
    let skill = c.guild_best(|g, l| (4.5 + ((f64::from(l) + 1.0).ln() / 15f64.ln()).exp()) * f64::from(Character::guild_ability(d, g, THIEVING)) / 10.0 * 14.0);
    let hard = 6.25 * f64::from(mlevel.max(1)).ln() * (f64::from(floor) + 1.0).ln();
    (s + skill - 1.5 * hard).clamp(5.0, 98.0)
}

/// The controlled character looks the container over (MORDOR.EXE's guess): WIS less
/// the monster's level and DEX, plus thieving / 1.2 and ln(thief level)², against d100.
fn examine(c: &mut Character, d: &GameData, ch: &mut Chest, floor: u32) {
    let mon = &d.monsters[ch.monster];
    let wis = f64::from(c.stats[WIS]) - (f64::from(mon.power_level.max(1)).ln() * f64::from(mon.stats[5].max(1)).ln() * 2.5).round_ties_even();
    let t = thieving(c, d, mon.power_level, floor).round_ties_even().max(5.0);
    let (_, l) = c.best_rate(d, THIEVING);
    let p = ((wis.clamp(2.0, 98.0) + t / 1.2).floor() + l.max(1.0).ln().powi(2)).round_ties_even();
    ch.sure = (p.clamp(1.0, 100.0) / 23.0).floor().max(1.0) as u8;
    ch.guess = if p < f64::from(c.d100()) { TRAPS[c.roll(11) as usize] } else { ch.trap };
}

/// The best opener's chance to disarm this container (its thieving).
pub fn disarm_chance(g: &Game, d: &GameData, c: &Chest) -> (usize, f64) {
    let (lvl, floor) = (d.monsters[c.monster].power_level, g.pos().floor as u32);
    g.living().into_iter().map(|j| (j, thieving(&g.party[j], d, lvl, floor) / 100.0)).max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((0, 0.0))
}

/// After a fight the container stays in the room for the party to open.
pub fn leave_chest(g: &mut Game, c: Chest) {
    g.say(format!("The monsters left a {}{}.", if c.chest { "chest" } else { "box" }, if c.magic_lock { ", magically locked" } else { "" }));
    g.chest = Some(c);
}

pub fn unlock(g: &mut Game) {
    let c = g.fight.as_mut().and_then(|f| f.chest.as_mut()).or(g.chest.as_mut());
    match c {
        Some(c) if c.magic_lock => {
            (c.magic_lock, c.unlocked) = (false, true);
            g.say("The magic lock springs open.");
        }
        _ => g.say("There's nothing locked here."),
    }
}

/// Opens the container. In a fight it takes CInt(10 - 2 ln(thieving - 10)) rounds (2 at
/// least) of the opener's turns, and provokes peaceful monsters. Opening disarms first,
/// at the opener's thieving %; a slip sets the trap off.
pub fn open(g: &mut Game, d: &GameData, t: &Tuning, i: usize) {
    let in_fight = g.fight.as_ref().is_some_and(|f| f.chest.is_some());
    let Some(c) = (if in_fight { g.fight.as_ref().and_then(|f| f.chest.clone()) } else { g.chest.clone() }) else {
        return;
    };
    if c.magic_lock {
        return g.say("It is magically locked. Cast Charm of Opening.");
    }
    let (best, _) = disarm_chance(g, d, &c);
    let who = if in_fight || i != 0 { i } else { best };
    let floor = g.pos().floor as u32;
    let skill = thieving(&g.party[who], d, d.monsters[c.monster].power_level, floor);
    let name = g.party[who].name.clone();
    if in_fight {
        g.provoke();
        let f = g.fight.as_mut().unwrap();
        let left = match f.opening {
            Some((j, n)) if j == who => n - 1,
            _ => ((10.0 - 2.0 * (skill - 10.0).max(1.0).ln()).round_ties_even() as u32).max(2) - 1,
        };
        if left > 0 {
            f.opening = Some((who, left));
            return g.say(format!("{name} works at the {} ({left} more rounds).", if c.chest { "chest" } else { "box" }));
        }
        f.opening = None;
        f.chest = None;
    } else {
        g.chest = None;
    }
    if c.trap != Trap::None {
        if f64::from(g.party[who].d100()) <= skill {
            let lv = f64::from(g.party[who].level());
            let xp = (skill * f64::from(d.monsters[c.monster].power_level) / 2.0 / (1.0 + 3.0 * lv / 999.0) * (1.0 - 0.04 * (g.party.len() as f64 - 1.0))).round_ties_even() as u64;
            let xp = g.party[who].add_xp(d, t, xp);
            g.say(format!("{name} disarms the {} trap (+{xp} XP).", c.trap.name()));
        } else {
            g.say(format!("{name} sets off the {} trap!", c.trap.name()));
            if spring(g, d, who, &c) {
                return;
            }
        }
    }
    loot(g, d, &c);
}

/// A trap's effect (MORDOR.EXE), mostly scaled by the floor. True if the party was
/// moved away from the container.
fn spring(g: &mut Game, d: &GameData, who: usize, c: &Chest) -> bool {
    let l = g.pos().floor.max(1) as f64;
    match c.trap {
        Trap::PoisonGas | Trap::DiseaseSpores => {
            // The original catches only the resistant (an inverted test); here
            // resistance / 2 is the chance to shrug it off.
            let (slot, poison) = if c.trap == Trap::PoisonGas { (5, true) } else { (4, false) };
            for j in g.living() {
                let res = crate::monsters::resist(&g.party[j], d, slot);
                let m = &mut g.party[j];
                if m.rnd() * 100.0 >= f64::from(res) / 2.0 {
                    let st = &mut m.status;
                    if poison { st.poison = st.poison.max(l as u16) } else { st.disease = st.disease.max(l as u16) }
                }
            }
        }
        Trap::Exploding | Trap::Fire => {
            let per = if c.trap == Trap::Fire { 6.0 } else { 4.0 };
            for j in g.living() {
                let res = if c.trap == Trap::Fire { crate::monsters::resist(&g.party[j], d, 0) } else { 0 };
                let dmg = ((g.party[j].rnd() * per * l * (l + 2.0).ln()).floor() + 1.0) * (1.0 - f64::from(res.clamp(0, 100)) / 200.0);
                g.hurt(j, dmg as i32);
            }
        }
        Trap::Slime => {
            let m = &mut g.party[who];
            if m.d100() < 50 + 2 * l as i32 {
                let s = m.roll(40) as usize;
                if s < m.items.len() && !(500..512).contains(&d.items[m.items[s].item].w(10)) && m.items[s].item != crate::game::HANDS {
                    let h = m.items.remove(s);
                    g.say(format!("The slime eats {}!", d.items[h.item].name));
                }
            }
        }
        Trap::Fate => {
            let m = &mut g.party[who];
            m.hp = (m.rnd() * f64::from(m.max_hp)).floor() as i32 + 1;
            m.sp = (m.rnd() * f64::from(5 * (m.stats[INT] + m.stats[WIS]))).floor() as i32 + 1;
        }
        Trap::Teleporter => {
            // The original sends only the opener; the remake keeps the party together.
            let floor = g.pos().floor;
            let (x, y, f) = g.random_square(d, floor);
            let p = g.pos();
            g.set_pos(d, crate::game::Pos { x, y, floor: f, facing: p.facing });
            return true;
        }
        Trap::Blackout => g.party[who].buffs = Default::default(),
        Trap::Fear => {
            g.party[who].companions.retain(|c| c.bind >= 12);
            g.say("The companions flee in terror.");
        }
        Trap::Withering => g.party[who].age_days += 28,
        Trap::None => {}
    }
    false
}

/// The gold (half again for a chest), then the items: d100 against the monster's item %
/// (w66; -25 for a chest, worse if unlocked by spell), then a tenth of it for each more.
fn loot(g: &mut Game, d: &GameData, c: &Chest) {
    g.share_gold(if c.chest { c.gold * 3 / 2 } else { c.gold });
    let mon = &d.monsters[c.monster];
    let l = g.pos().floor as f64;
    let mut chance = i32::from(mon.item_chance);
    let mut r = g.party[0].d100() - if c.chest { 25 } else { 0 } + if c.unlocked { (5.0 * (l + 10.0).ln()) as i32 } else { 0 };
    if r > 90 && chance != 100 {
        r = 90;
    }
    while r <= chance && chance > 0 {
        let item = random_item(&mut g.party[0], d, c.monster, c.chest);
        g.give(d, item);
        r = g.party[0].d100();
        chance = (f64::from(mon.item_chance) / 10.0).round_ties_even() as i32;
    }
}

/// An item from the monster's treasure (MORDOR.EXE): one of its ten item classes, at a
/// level from half its item level (one more for a chest) to its item level, weighted by
/// rarity (w6). Experience potions sometimes (the remake's).
pub fn random_item(r: &mut Character, d: &GameData, m: usize, chest: bool) -> Option<usize> {
    let mon = &d.monsters[m];
    if r.chance(0.04) {
        let greater = mon.level >= 5 && r.chance(0.3);
        let id = if greater { GREATER_POTION_OF_EXPERIENCE } else { POTION_OF_EXPERIENCE };
        return d.items.iter().position(|it| it.id == id);
    }
    let max = mon.item_level.max(1);
    let min = ((f64::from(max) / 2.0).round_ties_even() as i16 + i16::from(chest)).clamp(1, max);
    for _ in 0..20 {
        let class = mon.item_classes[r.roll(10) as usize];
        if class < -1 {
            if let Some(i) = d.items.iter().position(|it| it.id == -class - 2) {
                return Some(i);
            }
            continue;
        }
        let fits: Vec<(usize, u32)> = (0..d.items.len())
            .filter(|&i| (class == -1 || d.items[i].kind == class) && (min..=max).contains(&d.items[i].level) && d.items[i].id < POTION_OF_EXPERIENCE)
            .map(|i| (i, d.items[i].w(6).max(0) as u32))
            .collect();
        let total: u32 = fits.iter().map(|f| f.1).sum();
        if total == 0 {
            continue;
        }
        let mut pick = r.roll(total);
        for (i, w) in fits {
            if pick < w {
                return Some(i);
            }
            pick -= w;
        }
    }
    None
}

/// Whether the finder knows what it found (MORDOR.EXE): the item's difficulty,
/// 25 + (ln(mult + 1) + 0.3) x ln(w17 + 1) x level, against (INT + WIS) / 2 x
/// ln((best level + 5) / 10 + 1); and only items of level 5 or less are named.
pub fn identifies(c: &Character, d: &GameData, item: usize) -> bool {
    let it = &d.items[item];
    let hard = (25.0 + ((f64::from(it.damage_mult) + 1.0).ln() + 0.3) * (f64::from(it.w(17).max(0)) + 1.0).ln() * f64::from(it.level)).round_ties_even();
    let best = f64::from(*c.levels.iter().max().unwrap_or(&1));
    let skill = f64::from(c.stats[INT] + c.stats[WIS]) / 2.0 * ((best + 5.0) / 10.0 + 1.0).ln();
    hard <= skill && it.level <= 5
}
