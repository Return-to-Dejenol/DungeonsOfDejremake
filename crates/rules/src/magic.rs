//! Spells and magic items: who knows which spell (guild rates, narrowed to the four
//! caster schools in docs/balance.md), spell points, and every spell effect.

use serde::{Deserialize, Serialize};

use crate::{
    mdr::Words,
    Tuning,
    ai::Target,
    game::{Character, Game, Held, INT, Life, Pos, Status, WIS},
    mdr::{GameData, Item, Spell, square},
};

/// The kinds of spell the AI (and macros) ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Magic {
    Heal,
    MassHeal,
    Damage,
    MassDamage,
    Cure,
    Raise,
    Sleep,
    Protect,
}

const SEEKER: usize = 5;
const MAGE: usize = 8;
const SORCERER: usize = 9;
const WIZARD: usize = 10;
const HEALER: usize = 11;
/// The remake's additions to the tables (docs/balance.md, Experience boosts).
pub const CHARM_OF_EXPERIENCE: i16 = 200;
pub const POTION_OF_EXPERIENCE: i16 = 1000;
pub const GREATER_POTION_OF_EXPERIENCE: i16 = 1001;
/// Item kinds that are used up rather than worn.
const POTION: i16 = 23;
const SCROLL: i16 = 24;
const TOME: i16 = 25;

/// Adds Charm of Experience and the two experience potions to the imported tables.
pub fn add_remake_entries(d: &mut GameData) {
    d.spells.push(Spell {
        name: "Charm of Experience".into(),
        id: CHARM_OF_EXPERIENCE,
        class: 15,
        level: 16,
        sp: 60,
        targets: 0,
        groups: 0,
        base: 0,
        per_level: 0,
        effect: 0,
        stat_req: [0, 14, 14, 0, 0, 0],
        resist: -1,
        rest: Vec::new(),
    });
    for (name, id, price, level) in [("Potion of Experience", POTION_OF_EXPERIENCE, 7500, 0), ("Greater Potion of Experience", GREATER_POTION_OF_EXPERIENCE, 30000, 5)] {
        d.items.push(Item { name: name.into(), id, attack: 0, defense: 0, price, level, power: 0, guilds: 4095, damage_mult: 1.0, kind: POTION, rest: Vec::new() });
    }
}

/// The classes each reworked caster keeps (docs/balance.md, Magic guilds); location
/// (Set Sanctuary...) and magical (Charm of Opening) stay as the original.
fn school(guild: usize, class: i16) -> bool {
    let keep: &[i16] = match guild {
        SORCERER => &[0, 1, 2, 5, 13, 17],
        HEALER => &[4, 9, 12, 13],
        MAGE => &[3, 7, 8, 14],
        WIZARD => &[4, 6, 10, 11],
        _ => return true,
    };
    keep.contains(&class) || class == 15 || class == 16
}

/// Guild level at which `guild` teaches spell `s`, if it does.
pub fn taught_at(d: &GameData, guild: usize, s: usize) -> Option<u32> {
    let sp = &d.spells[s];
    if sp.id == CHARM_OF_EXPERIENCE {
        return (guild == SEEKER).then_some(16);
    }
    let rate = d.guilds[guild].spell_rates[sp.class as usize];
    (rate > 0.0 && school(guild, sp.class)).then(|| (f32::from(sp.level.max(1)) * rate).ceil() as u32)
}

/// The caster's spell level for `s`: the best guild's level / rate.
pub fn power(c: &Character, d: &GameData, s: usize) -> f32 {
    (0..d.guilds.len())
        .filter_map(|g| {
            let at = taught_at(d, g, s)?;
            (c.levels[g] >= at).then(|| c.levels[g] as f32 * f32::from(d.spells[s].level.max(1)) / at as f32)
        })
        .fold(0.0, f32::max)
}

pub fn known(c: &Character, d: &GameData) -> Vec<usize> {
    (0..d.spells.len()).filter(|&s| power(c, d, s) > 0.0).collect()
}

pub fn castable(c: &Character, d: &GameData, t: &Tuning, s: usize) -> bool {
    let sp = &d.spells[s];
    let _ = t;
    c.able() && c.sp >= i32::from(sp.sp) && (0..6).all(|i| c.stats[i] >= sp.stat_req[i]) && power(c, d, s) > 0.0
}

pub fn kind(sp: &Spell) -> Option<Magic> {
    let mass = sp.effect >= 200 || sp.targets >= 3;
    match sp.class {
        9 if sp.base > 0 => Some(if sp.effect >= 200 { Magic::MassHeal } else { Magic::Heal }),
        9 if matches!(sp.effect, 1 | 2 | 10 | 18) => Some(Magic::Cure),
        9 if matches!(sp.effect, 20 | 22) => Some(Magic::Raise),
        0 | 1 | 2 | 4 | 5 if sp.base > 0 => Some(if mass { Magic::MassDamage } else { Magic::Damage }),
        3 | 6 if sp.effect == 100 && sp.base == 0 => Some(Magic::Sleep),
        6 => Some(Magic::Damage),
        17 => Some(Magic::Protect),
        _ => None,
    }
}

/// The strongest castable spell of a kind (cures: one that fits someone's ailment).
pub fn best(g: &Game, d: &GameData, t: &Tuning, i: usize, k: Magic) -> Option<usize> {
    let c = &g.party[i];
    known(c, d).into_iter().filter(|&s| kind(&d.spells[s]) == Some(k) && castable(c, d, t, s)).max_by_key(|&s| (d.spells[s].level, d.spells[s].base))
}

/// A rough average melee hit, for the AI's "is a spell worth it" check.
pub fn melee_estimate(c: &Character, d: &GameData) -> i32 {
    let power = c.weapon(d).map_or(2, |w| d.items[w].power.max(1));
    i32::from(power) * 3 + 2
}

/// A carried healing potion's slot, preferring the strongest.
pub fn healing_potion(c: &Character, d: &GameData) -> Option<usize> {
    (0..c.items.len()).filter(|&i| item_spell(d, c.items[i].item).is_some_and(|s| kind(&d.spells[s]) == Some(Magic::Heal))).max_by_key(|&i| item_spell(d, c.items[i].item).map(|s| d.spells[s].base))
}

fn word(it: &Item, w: usize) -> i16 {
    it.w(w)
}

/// The spell an item casts when used (potions, scrolls, rods): the record's 1-based
/// spell number at w11, with charges at w13.
pub fn item_spell(d: &GameData, item: usize) -> Option<usize> {
    let n = word(&d.items[item], 11);
    (n > 0 && (n as usize) <= d.spells.len()).then(|| n as usize - 1)
}

pub fn consumable(d: &GameData, item: usize) -> bool {
    matches!(d.items[item].kind, POTION | SCROLL | TOME) || item_spell(d, item).is_some() && d.items[item].power == 0
}

/// Cursed (w40, MORDOR.EXE): it sticks once equipped.
pub fn cursed(d: &GameData, item: usize, _alignment: u8) -> bool {
    d.items[item].w(40) != 0
}

/// STR, INT, WIS, CON, CHA, DEX needed to equip (w26-31).
pub fn stat_req(d: &GameData, item: usize) -> [i16; 6] {
    std::array::from_fn(|i| word(&d.items[item], 26 + i))
}

pub fn new_held(d: &GameData, item: usize, known: bool) -> Held {
    let charges = if d.items[item].id >= POTION_OF_EXPERIENCE { 1 } else { word(&d.items[item], 13).max(0) };
    Held { item, equipped: false, charges, known }
}

/// Unidentified items show only their type, as in the original.
pub fn item_name(d: &GameData, h: &Held) -> String {
    let it = &d.items[h.item];
    let charges = if h.charges > 0 && consumable(d, h.item) { format!(" [{}]", h.charges) } else { String::new() };
    if h.known { format!("{}{charges}", it.name) } else { format!("Unidentified {}{charges}", d.item_types.get(it.kind as usize).map_or("item", String::as_str)) }
}

fn target_ally(g: &Game, t: Target, caster: usize) -> usize {
    match t {
        Target::Myself => caster,
        _ => (0..g.party.len()).filter(|&j| g.party[j].alive()).min_by_key(|&j| 100 * g.party[j].hp / g.party[j].max_hp.max(1)).unwrap_or(caster),
    }
}

/// Casts spell `s` for member `i`.
pub fn cast(g: &mut Game, d: &GameData, t: &Tuning, i: usize, s: usize, target: Target) {
    if !castable(&g.party[i], d, t, s) {
        return g.say(format!("{} can't cast that now.", g.party[i].name));
    }
    let anti = g.floor(d).is_some_and(|f| f.square(g.pos().x, g.pos().y).flags & square::ANTI_MAGIC != 0);
    if anti {
        return g.say("The square swallows the spell (anti-magic).");
    }
    g.party[i].sp -= i32::from(d.spells[s].sp);
    let p = power(&g.party[i], d, s);
    effect(g, d, t, i, s, p, target);
}

/// Uses a carried item of kind `item`: drink, read or zap it.
pub fn use_item(g: &mut Game, d: &GameData, t: &Tuning, i: usize, item: usize) {
    let it = &d.items[item];
    let name = g.party[i].name.clone();
    // Only potions, scrolls, tomes and items that cast a spell can be used; a used-up
    // sword or artifact keeps its place at [0] (the store recharges it).
    if !matches!(it.kind, POTION | SCROLL | TOME) && item_spell(d, item).is_none() {
        return g.say(format!("{name} can't use {}.", it.name));
    }
    let Some(slot) = g.party[i].items.iter().position(|h| h.item == item && (h.charges > 0 || consumable(d, item))) else { return g.say(format!("{} has no charges left.", it.name)) };
    g.party[i].items[slot].known = true;
    match it.id {
        POTION_OF_EXPERIENCE | GREATER_POTION_OF_EXPERIENCE => {
            let mult = if it.id == POTION_OF_EXPERIENCE { 1.5 } else { 2.0 };
            let b = &mut g.party[i].buffs;
            b.xp_mult = b.xp_mult.max(mult);
            g.say(format!("{name} feels eager to learn ({mult}x experience until you change floor)."));
        }
        _ if item_spell(d, item).is_some() => {
            let s = item_spell(d, item).unwrap();
            g.say(format!("{name} uses {}.", it.name));
            let level = f32::from(d.spells[s].level) + 2.0;
            effect(g, d, t, i, s, level, Target::LowestAlly);
        }
        _ => {
            // Stat potions and tomes: +1 to a stat up to the race maximum + 5 (the original).
            let stat = ["Might", "Intelligence|Learning", "Insight", "Fitness|Endurance", "Leadership", "Agility"].iter().position(|k| k.split('|').any(|k| it.name.contains(k)));
            match stat {
                Some(st) => {
                    let cap = d.races[g.party[i].race].stat_max[st] + 5;
                    let c = &mut g.party[i];
                    c.stats[st] = (c.stats[st] + 1).min(cap);
                    g.say(format!("{name}'s {} rises to {}.", crate::game::STATS[st], g.party[i].stats[st]));
                }
                None if it.name.contains("Youth") => {
                    g.party[i].age_days = g.party[i].age_days.saturating_sub(365);
                    g.say(format!("{name} feels a year younger."));
                }
                None => g.say(format!("Nothing happens when {name} uses {}.", it.name)),
            }
        }
    }
    let h = &mut g.party[i].items[slot];
    h.charges -= 1;
    if h.charges <= 0 && consumable(d, item) {
        g.party[i].items.remove(slot);
    }
}

/// What a spell does, at spell level `p`.
fn effect(g: &mut Game, d: &GameData, t: &Tuning, i: usize, s: usize, p: f32, target: Target) {
    let sp = &d.spells[s];
    let caster = g.party[i].name.clone();
    let stat_bonus = |c: &Character, st: usize| 1.0 + 0.02 * f32::from((c.stats[st] - 15).max(0));
    let amount = |g: &mut Game| i32::from(sp.base) + g.party[i].roll((f32::from(sp.per_level.max(0)) * p) as u32 + 1) as i32;
    g.say(format!("{caster} casts {}.", sp.name));
    if sp.id == CHARM_OF_EXPERIENCE {
        for m in g.party.iter_mut().filter(|m| m.alive()) {
            m.buffs.xp_mult = m.buffs.xp_mult.max(1.5);
        }
        return g.say("The party will learn 1.5x faster until you change floor.");
    }
    match kind(sp) {
        Some(Magic::Heal | Magic::MassHeal) => {
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            let heal = (amount(g) as f32 * stat_bonus(&g.party[i], WIS)) as i32;
            for j in who {
                let m = &mut g.party[j];
                m.hp = (m.hp + heal).min(m.max_hp);
                let name = m.name.clone();
                g.say(format!("{name} heals {heal}."));
            }
            return;
        }
        Some(Magic::Cure) => {
            let j = (0..g.party.len()).find(|&j| g.party[j].alive() && (g.party[j].status.poison + g.party[j].status.disease + g.party[j].status.paralysis) > 0).unwrap_or(i);
            let st = &mut g.party[j].status;
            match sp.effect {
                1 => st.poison = 0,
                2 => st.disease = 0,
                10 => st.paralysis = 0,
                _ => *st = Status::default(),
            }
            let name = g.party[j].name.clone();
            return g.say(format!("{name} is cured."));
        }
        Some(Magic::Raise) => {
            let Some(j) = (0..g.party.len()).find(|&j| matches!(g.party[j].life, Life::Dead(_))) else { return };
            if g.party[i].chance(0.75 + 0.02 * f64::from(p)) {
                let m = &mut g.party[j];
                m.life = Life::Alive;
                m.hp = if sp.effect == 22 { m.max_hp } else { 1 };
                let name = m.name.clone();
                g.say(format!("{name} lives again!"));
            } else {
                g.say("The spell fails to take hold.");
            }
            return;
        }
        Some(Magic::Protect) => {
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            for j in who {
                g.party[j].buffs.protection = g.party[j].buffs.protection.max(5 + p as i32);
            }
            return g.say("A shimmering shield surrounds you.");
        }
        _ => {}
    }
    match (sp.class, sp.effect) {
        (13, 50..=60) => {
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            for j in who {
                g.party[j].buffs.resist |= 1 << (sp.effect - 50);
            }
            g.say("You feel protected.");
        }
        (_, 3 | 203) => {
            for m in g.party.iter_mut().filter(|m| m.alive()) {
                m.buffs.levitate = true;
            }
            g.say("You float above the floor.");
        }
        (_, 4 | 204) => {
            for m in g.party.iter_mut().filter(|m| m.alive()) {
                m.buffs.protection = m.buffs.protection.max(4);
            }
            g.say("The party fades from sight.");
        }
        (_, 5) => {
            g.party[i].buffs.see_invisible = true;
            g.say("Your eyes tingle.");
        }
        (_, 7) => {
            let floor = g.pos().floor.max(1);
            let (x, y, f) = g.random_square(d, floor);
            g.fight = None;
            g.set_pos(d, Pos { x, y, floor: f, facing: g.pos().facing });
            g.say("The party is teleported!");
        }
        (_, 8 | 9) => {
            // Displacement and Ethereal Portal: through the wall ahead. Into rock is death.
            let Some(f) = g.floor(d) else { return };
            let pos = g.pos();
            if let Some((x, y)) = crate::game::ahead(f, pos.x, pos.y, pos.facing) {
                let rock = f.square(x, y).flags & square::ROCK != 0;
                g.fight = None;
                g.set_pos(d, Pos { x, y, ..pos });
                if rock {
                    for j in g.living() {
                        g.hurt(j, i32::MAX / 2);
                    }
                    g.say("The party is fused with rock!");
                } else {
                    g.say("You pass through the wall.");
                }
            }
        }
        (_, 11) => {
            let Some(f) = g.floor(d) else { return };
            let pos = g.pos();
            let rock = crate::game::ahead(f, pos.x, pos.y, pos.facing).is_some_and(|(x, y)| f.square(x, y).flags & square::ROCK != 0);
            g.say(if rock { "There is rock ahead." } else { "No rock ahead." });
        }
        (_, 12) => {
            let floor = g.pos().floor;
            g.say(format!("You are on level {floor}."));
        }
        (_, 13) => {
            let pos = g.pos();
            g.say(format!("You are at {},{} facing {}.", pos.x + 1, pos.y + 1, ["north", "east", "south", "west"][pos.facing as usize]));
        }
        (_, 14 | 15) => {
            let lost: Vec<String> = g.party.iter().filter_map(|m| match m.life {
                Life::Dead(p) => Some(format!("{} at {},{} on level {}", m.name, p.x + 1, p.y + 1, p.floor)),
                _ => None,
            }).collect();
            g.say(if lost.is_empty() { "You sense no lost souls.".into() } else { format!("You sense {}.", lost.join(", ")) });
        }
        (_, 16) => {
            let pos = g.pos();
            g.party[i].sanctuary = Some(pos);
            g.say("This place is now your sanctuary.");
        }
        (_, 17) => {
            if let Some(to) = g.party[i].sanctuary {
                g.fight = None;
                g.party[i].age_days += 365;
                g.set_pos(d, to);
                g.say("The party returns to its sanctuary.");
            }
        }
        (_, 19) => {
            // Fate: sometimes good, sometimes bad.
            let j = target_ally(g, Target::LowestAlly, i);
            if g.party[i].chance(0.5) {
                let m = &mut g.party[j];
                m.hp = m.max_hp;
                g.say("Fate smiles on you.");
            } else {
                let dmg = 1 + g.party[i].roll(10) as i32;
                g.hurt(j, dmg);
                g.say("Fate frowns.");
            }
        }
        (_, 21) => {
            for m in &mut g.party {
                m.status.stoned = false;
            }
            g.say("Stone turns back to flesh.");
        }
        (_, 30) => crate::treasure::unlock(g),
        (7 | 8, _) => crate::monsters::charm(g, d, i, s, p),
        (11 | 12, 75..=86) => {
            g.provoke();
            let kind = sp.effect - 75;
            let Some(f) = g.fight.as_ref() else { return };
            let hits: Vec<usize> = f.alive().filter(|&fi| kind == 5 || d.monsters[f.foes[fi].monster].kind == kind).take(sp.targets.max(1) as usize).collect();
            if hits.is_empty() {
                return g.say("It has no effect on these monsters.");
            }
            for fi in hits {
                let level = f32::from(d.monsters[g.fight.as_ref().unwrap().foes[fi].monster].level);
                if g.party[i].chance(f64::from(0.6 + 0.05 * (p - level)).clamp(0.1, 0.95)) {
                    let hp = g.fight.as_ref().unwrap().foes[fi].hp;
                    g.damage_foe(d, t, i, fi, hp, "banishes", 1.0);
                }
            }
        }
        _ => offensive(g, d, t, i, s, p, target),
    }
}

/// Damage, kill and mind spells on the foes.
fn offensive(g: &mut Game, d: &GameData, t: &Tuning, i: usize, s: usize, p: f32, target: Target) {
    let sp = &d.spells[s];
    if g.fight.is_none() {
        return g.say("There is nothing to aim at.");
    }
    g.provoke();
    let f = g.fight.as_ref().unwrap();
    let first = f.pick(target).unwrap_or(0);
    let mut hits: Vec<usize> = std::iter::once(first).chain(f.alive().filter(|&x| x != first)).take(sp.targets.max(1) as usize).collect();
    hits.retain(|&fi| f.foes[fi].hp > 0);
    let int_bonus = 1.0 + 0.02 * f32::from((g.party[i].stats[INT] - 15).max(0));
    // Sorcerers' elemental spells ignore half of a monster's resistance.
    let pierce = if g.party[i].guild == SORCERER && [0, 1, 2, 5].contains(&sp.class) { 0.5 } else { 1.0 };
    let wizard = g.party[i].guild == WIZARD && school(WIZARD, sp.class);
    for fi in hits {
        let Some(f) = g.fight.as_ref() else { return };
        if f.foes[fi].hp <= 0 {
            continue;
        }
        let m = &d.monsters[f.foes[fi].monster];
        let resist = if sp.resist >= 0 { f32::from(m.resist[sp.resist as usize].clamp(0, 100)) * pierce / 100.0 } else { 0.0 };
        if sp.effect == 100 && sp.base == 0 {
            // Sleep, Dazzle and the like: the foe loses a few rounds.
            if !g.party[i].chance(f64::from(resist)) {
                g.fight.as_mut().unwrap().foes[fi].asleep = 2 + g.party[i].roll(3) as u16;
                g.say(format!("The {} is overcome.", m.name));
            }
            continue;
        }
        if sp.effect == 100 && g.party[i].chance(f64::from((0.25 + 0.04 * (p - f32::from(m.level))) * (1.0 - resist)).clamp(0.02, 0.8)) {
            let hp = f.foes[fi].hp;
            if g.damage_foe(d, t, i, fi, hp, "slays", 1.0) && wizard {
                restore_sp(g, d, t, i);
            }
            continue;
        }
        let base = i32::from(sp.base) + g.party[i].roll((f32::from(sp.per_level.max(0)) * p) as u32 + 1) as i32;
        let dmg = ((base as f32 * int_bonus * (1.0 - resist)) as i32).max(if resist >= 1.0 { 0 } else { 1 });
        if g.damage_foe(d, t, i, fi, dmg, "blasts", 1.0) && wizard {
            restore_sp(g, d, t, i);
        }
    }
}

/// Wizards regain a tenth of their SP for a kill with a Wizard spell.
fn restore_sp(g: &mut Game, d: &GameData, t: &Tuning, i: usize) {
    let max = g.party[i].max_sp(d, t);
    g.party[i].sp = (g.party[i].sp + max / 10).min(max);
}
