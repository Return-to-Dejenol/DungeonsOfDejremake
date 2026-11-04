//! Chests and boxes (the help's Encounters: Treasure): traps, what the party thinks the
//! trap is, disarming with thieving skill, magic locks, and the loot.

use crate::{
    Tuning,
    game::{DEX, Game, PERCEPTION, THIEVING},
    magic::{GREATER_POTION_OF_EXPERIENCE, POTION_OF_EXPERIENCE},
    mdr::GameData,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trap {
    None,
    DiseaseSpores,
    PoisonGas,
    ExplodingBox,
    FireTrap,
    Slime,
    Fate,
    Teleporter,
    Fear,
    Withering,
    Blackout,
}

pub const TRAPS: [Trap; 11] = [Trap::None, Trap::DiseaseSpores, Trap::PoisonGas, Trap::ExplodingBox, Trap::FireTrap, Trap::Slime, Trap::Fate, Trap::Teleporter, Trap::Fear, Trap::Withering, Trap::Blackout];

impl Trap {
    pub fn name(self) -> &'static str {
        match self {
            Trap::None => "No Trap",
            Trap::DiseaseSpores => "Disease Spores",
            Trap::PoisonGas => "Poison Gas",
            Trap::ExplodingBox => "Exploding Box",
            Trap::FireTrap => "Fire Trap",
            Trap::Slime => "Slime",
            Trap::Fate => "Fate",
            Trap::Teleporter => "Teleporter",
            Trap::Fear => "Fear",
            Trap::Withering => "Withering",
            Trap::Blackout => "Blackout",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chest {
    /// A chest holds the best treasure; a box the average find.
    pub chest: bool,
    pub trap: Trap,
    /// What the party thinks the trap is, and how sure (1-4), as the original's stars.
    pub guess: Trap,
    pub sure: u8,
    pub magic_lock: bool,
    /// The level of the monster that set it: trap strength and loot.
    pub level: i16,
}

/// A container for an encounter: lairs always have a chest; others by the monster's
/// odds. Magic locks follow the floor table in `modern.toml` (rare on floors 1-5).
pub fn chest_for(g: &mut Game, d: &GameData, m: usize, lair: bool) -> Option<Chest> {
    let mon = &d.monsters[m];
    let r = &mut g.party[0];
    let chest = lair || r.chance(f64::from(mon.chest_chance.max(0)) / 300.0);
    if !chest && !r.chance(f64::from(mon.box_chance.max(0)) / 200.0) {
        return None;
    }
    let trap = if r.chance(0.3) { Trap::None } else { TRAPS[1 + r.roll(10) as usize] };
    let floor = g.pos().floor as u32;
    let lock = crate::Tuning::load(&[]).map(|t| t.magic_lock_chance(floor)).unwrap_or(0.0);
    let mut c = Chest { chest, trap, guess: trap, sure: 1, magic_lock: g.party[0].chance(lock), level: mon.level.max(1) };
    examine(g, d, &mut c);
    Some(c)
}

/// The best perceiver in the party looks the container over.
fn examine(g: &mut Game, d: &GameData, c: &mut Chest) {
    let p = g.living().into_iter().map(|j| g.party[j].ability(d, PERCEPTION)).fold(0.0, f32::max);
    let sure = (f64::from(p) / 8.0 - f64::from(c.level) / 6.0).clamp(0.0, 0.95);
    c.sure = 1 + (sure * 4.0) as u8;
    c.guess = if g.party[0].chance(0.35 + sure) { c.trap } else { TRAPS[g.party[0].roll(11) as usize] };
}

/// The best disarmer's chance on this container (thieving, DEX, the trap's level).
pub fn disarm_chance(g: &Game, d: &GameData, c: &Chest) -> (usize, f64) {
    g.living()
        .into_iter()
        .map(|j| {
            let m = &g.party[j];
            let skill = f64::from(m.ability(d, THIEVING)) * 0.04 + f64::from((m.stats[DEX] - 12).max(0)) * 0.02;
            (j, (0.3 + skill - f64::from(c.level) * 0.03).clamp(0.05, 0.97))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0))
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
            c.magic_lock = false;
            g.say("The magic lock springs open.");
        }
        _ => g.say("There's nothing locked here."),
    }
}

/// Opens the container: the best disarmer tries first; a slip sets the trap off.
/// In combat this provokes peaceful monsters.
pub fn open(g: &mut Game, d: &GameData, t: &Tuning, i: usize) {
    let in_fight = g.fight.as_ref().is_some_and(|f| f.chest.is_some());
    if in_fight {
        g.provoke();
    }
    let Some(c) = (if in_fight { g.fight.as_ref().and_then(|f| f.chest.clone()) } else { g.chest.clone() }) else { return };
    if c.magic_lock {
        return g.say("It is magically locked. Cast Charm of Opening.");
    }
    let (j, p) = disarm_chance(g, d, &c);
    let who = if i == 0 { j } else { i };
    let name = g.party[who].name.clone();
    if c.trap != Trap::None {
        if g.party[who].chance(p) {
            g.say(format!("{name} disarms the {}.", c.trap.name()));
        } else {
            g.say(format!("{name} sets off a {}!", c.trap.name()));
            spring(g, d, who, &c);
        }
    }
    if in_fight {
        g.fight.as_mut().unwrap().chest = None;
    } else {
        g.chest = None;
    }
    loot(g, d, t, &c);
}

fn spring(g: &mut Game, d: &GameData, who: usize, c: &Chest) {
    let lvl = c.level.max(1) as u32;
    match c.trap {
        Trap::DiseaseSpores | Trap::PoisonGas => {
            for j in g.living() {
                let st = &mut g.party[j].status;
                if c.trap == Trap::PoisonGas { st.poison = st.poison.max(10 + 2 * lvl as u16) } else { st.disease = st.disease.max(20 + 3 * lvl as u16) }
            }
        }
        Trap::ExplodingBox => {
            for j in g.living() {
                let dmg = 1 + g.party[j].roll(4 * lvl) as i32;
                g.hurt(j, dmg);
            }
        }
        Trap::FireTrap => {
            for j in g.living() {
                let mut dmg = 2 + g.party[j].roll(6 * lvl) as i32;
                if j != who || g.party[j].buffs.resist & 1 != 0 {
                    dmg /= 2;
                }
                g.hurt(j, dmg);
            }
        }
        Trap::Slime => {
            let m = &mut g.party[who];
            if !m.items.is_empty() {
                let s = m.roll(m.items.len() as u32) as usize;
                let h = m.items.remove(s);
                g.say(format!("The slime eats {}!", d.items[h.item].name));
            }
        }
        Trap::Fate => {
            if g.party[who].chance(0.5) {
                let m = &mut g.party[who];
                m.hp = m.max_hp;
            } else {
                let dmg = 1 + g.party[who].roll(8 * lvl) as i32;
                g.hurt(who, dmg);
            }
        }
        Trap::Teleporter => {
            let floor = g.pos().floor;
            let (x, y, f) = g.random_square(d, floor);
            let p = g.pos();
            g.set_pos(d, crate::game::Pos { x, y, floor: f, facing: p.facing });
        }
        Trap::Fear => {
            g.party[0].companions.clear();
            g.say("Your companions flee in terror.");
        }
        Trap::Withering => g.party[who].age_days += 28,
        Trap::Blackout => g.party[who].buffs = Default::default(),
        Trap::None => {}
    }
}

/// Gold and items, shared out among the living (as the original's divvying up).
fn loot(g: &mut Game, d: &GameData, t: &Tuning, c: &Chest) {
    let lvl = u64::from(c.level.max(1) as u16);
    let (lo, hi) = if c.chest { (60, 240) } else { (20, 80) };
    let gold = lvl * (lo + u64::from(g.party[0].roll((hi - lo) as u32)));
    let living = g.living();
    for &j in &living {
        g.party[j].gold += gold / living.len() as u64;
    }
    g.say(format!("Inside: {gold} gold."));
    let floor = g.pos().floor.max(1);
    let n = if c.chest { 1 + g.party[0].roll(2) } else { u32::from(g.party[0].chance(0.4)) };
    for _ in 0..n {
        let item = random_item(g, d, floor + usize::from(c.chest));
        g.give(d, item);
    }
    let _ = t;
}

/// A dungeon item found on `floor` or shallower; experience potions sometimes.
pub fn random_item(g: &mut Game, d: &GameData, floor: usize) -> Option<usize> {
    let r = &mut g.party[0];
    if r.chance(0.04) {
        let greater = floor >= 5 && r.chance(0.3);
        let id = if greater { GREATER_POTION_OF_EXPERIENCE } else { POTION_OF_EXPERIENCE };
        return d.items.iter().position(|it| it.id == id);
    }
    let found: Vec<usize> = (0..d.items.len()).filter(|&i| (1..=floor as i16).contains(&d.items[i].level) && d.items[i].price > 0 && d.items[i].id < POTION_OF_EXPERIENCE).collect();
    (!found.is_empty()).then(|| found[r.roll(found.len() as u32) as usize])
}
