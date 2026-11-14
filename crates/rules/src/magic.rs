//! Spells and magic items: who knows which spell (guild rates, narrowed to the four
//! caster schools in docs/balance.md), spell points, and every spell effect.

use serde::{Deserialize, Serialize};

use crate::{
    mdr::Words,
    Tuning,
    ai::Target,
    game::{CHAR_HIT, Character, Game, Held, INT, Life, Pos, Status, WIS},
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

/// The original's spell level for guild level L: Int(L / 2 + 0.5).
pub fn spell_level(l: u32) -> u32 {
    l.div_ceil(2)
}

/// CInt(spell level x rate): the spell level a guild needs to cast it.
fn need(sp: &Spell, rate: f32) -> u32 {
    (f64::from(sp.level) * f64::from(rate)).round_ties_even() as u32
}

/// Whether guild g teaches spell s (mdrcharCheckCast): a rate for its class, a level no
/// higher than the guild's max for the class (w71 + 2c), within the remake's schools.
fn teaches(d: &GameData, g: usize, s: usize) -> bool {
    let sp = &d.spells[s];
    if sp.id == CHARM_OF_EXPERIENCE {
        return g == SEEKER;
    }
    let c = sp.class as usize;
    d.guilds[g].spell_rates[c] > 0.0 && f32::from(sp.level) <= d.guilds[g].single(71 + 2 * c) && school(g, sp.class)
}

/// The spell level guild g needs for spell s (Charm of Experience: Seekers at 16).
fn needs(d: &GameData, g: usize, s: usize) -> u32 {
    let sp = &d.spells[s];
    if sp.id == CHARM_OF_EXPERIENCE { 16 } else { need(sp, d.guilds[g].spell_rates[sp.class as usize]).max(1) }
}

/// Guild level at which `guild` teaches spell `s`, if it does: its spell level reaches
/// CInt(level x rate) at guild level twice that, less one.
pub fn taught_at(d: &GameData, guild: usize, s: usize) -> Option<u32> {
    teaches(d, guild, s).then(|| 2 * needs(d, guild, s) - 1)
}

/// The caster's spell level for `s` (fn1417): the best spell level among the joined guilds
/// that teach it, once one of them is high enough to cast it; 0 if it isn't known.
pub fn power(c: &Character, d: &GameData, s: usize) -> f32 {
    let joined = || (0..d.guilds.len()).filter(|&g| c.levels[g] > 0 && teaches(d, g, s));
    if !joined().any(|g| needs(d, g, s) <= spell_level(c.levels[g])) {
        return 0.0;
    }
    joined().map(|g| spell_level(c.levels[g])).max().unwrap_or(0) as f32
}

pub fn known(c: &Character, d: &GameData) -> Vec<usize> {
    (0..d.spells.len()).filter(|&s| power(c, d, s) > 0.0).collect()
}

/// A spell's SP cost (fn142F): Int(CInt(level x rate) / spell level x SP x rate x 1.9)
/// from the cheapest joined guild, no less than the smallest CInt(level x rate).
pub fn sp_cost(c: &Character, d: &GameData, s: usize) -> i32 {
    let sp = &d.spells[s];
    if sp.id == CHARM_OF_EXPERIENCE {
        return i32::from(sp.sp);
    }
    let cl = sp.class as usize;
    let (mut cost, mut least) = (0, -1);
    for g in (0..d.guilds.len()).filter(|&g| c.levels[g] > 0 && f32::from(sp.level) <= d.guilds[g].single(71 + 2 * cl) && school(g, sp.class)) {
        let rate = Some(d.guilds[g].spell_rates[cl]).filter(|&r| r != 0.0).unwrap_or(1.0);
        let learn = need(sp, rate) as i32;
        let x = (f64::from(learn) / f64::from(spell_level(c.levels[g]).clamp(1, 255)) * f64::from(sp.sp) * f64::from(rate) * 1.9).floor() as i32;
        if x <= cost || cost == 0 {
            cost = x;
            if learn < least || least == -1 {
                least = learn;
            }
            cost = cost.max(least);
        }
    }
    cost
}

pub fn castable(c: &Character, d: &GameData, t: &Tuning, s: usize) -> bool {
    let sp = &d.spells[s];
    let _ = t;
    c.able() && c.pos.floor > 0 && c.sp >= sp_cost(c, d, s) && (0..6).all(|i| c.stats[i] >= sp.stat_req[i]) && power(c, d, s) > 0.0
}

/// When a spell may be thrown (fn1448): heal-class spells and those without hits are
/// for peace, Cure Paralysis and Teleport for both, the rest for combat.
#[derive(Clone, Copy, PartialEq)]
pub enum When {
    Peace,
    War,
    Both,
}

pub fn when(sp: &Spell) -> When {
    match () {
        _ if matches!(sp.effect, 7 | 10) => When::Both,
        _ if sp.base == 0 || sp.class == 9 => When::Peace,
        _ => When::War,
    }
}

/// mdrcharCheckCast's refusal, as a text id: a peace spell while hostile monsters are
/// about, a combat spell with none, and above level 7 against peaceful ones. Items too.
pub fn refusal(g: &Game, sp: &Spell) -> Option<u32> {
    let foes = g.fight.as_ref().filter(|f| !f.over());
    match (foes, when(sp)) {
        (Some(f), When::Peace) if !f.peaceful => Some(0x9b8e_2e2f),
        (None, When::War) => Some(0xca85_6849),
        (Some(f), When::War | When::Both) if f.peaceful && sp.level > 7 => Some(0x43b8_37ee),
        _ => None,
    }
}

/// A spell's hits (MONSMOD.monGetSpellHits; healing in mdrcharCastSpell): D = base x
/// (1 + ln(l + 1) / ln 5) + Int(l / 10 x per level), log10 l for healing, less 1 to D / 2,
/// at caster level l.
pub(crate) fn hits(c: &mut Character, sp: &Spell, l: f64, heal: bool) -> f64 {
    let l = l.clamp(1.0, 255.0);
    let b = f64::from(sp.base);
    let d = b + b * if heal { l.log10() } else { (l + 1.0).ln() / 5f64.ln() } + (l / 10.0 * f64::from(sp.per_level)).floor();
    (d - ((c.rnd() * (d - (l - 1.0) / (l * l)) / 2.0).floor() + 1.0)).floor()
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

/// The strongest known spell of a kind the character's stats allow, whatever its SP.
pub fn best_known(c: &Character, d: &GameData, k: Magic) -> Option<usize> {
    known(c, d).into_iter().filter(|&s| kind(&d.spells[s]) == Some(k) && (0..6).all(|i| c.stats[i] >= d.spells[s].stat_req[i])).max_by_key(|&s| (d.spells[s].level, d.spells[s].base))
}

/// The strongest spell of a kind that can be cast now.
pub fn best(g: &Game, d: &GameData, t: &Tuning, i: usize, k: Magic) -> Option<usize> {
    let c = &g.party[i];
    known(c, d).into_iter().filter(|&s| kind(&d.spells[s]) == Some(k) && castable(c, d, t, s) && refusal(g, &d.spells[s]).is_none()).max_by_key(|&s| (d.spells[s].level, d.spells[s].base))
}

/// A rough average melee hit, for the AI's "is a spell worth it" check.
pub fn melee_estimate(c: &Character, d: &GameData) -> i32 {
    let power = c.weapon(d).map_or(2, |w| d.items[w].power.max(1));
    i32::from(power) * 3 + 2
}

/// A carried healing potion's slot, preferring the strongest.
pub fn healing_potion(c: &Character, d: &GameData) -> Option<usize> {
    (0..c.items.len())
        .filter(|&i| matches!(d.items[c.items[i].item].kind, POTION) && item_spell(d, c.items[i].item).is_some_and(|s| kind(&d.spells[s]) == Some(Magic::Heal)))
        .max_by_key(|&i| item_spell(d, c.items[i].item).map(|s| d.spells[s].base))
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

/// An item's name, or the original's "Unidentified Item", with its charges.
pub fn item_name(d: &GameData, h: &Held) -> String {
    let it = &d.items[h.item];
    let charges = if h.charges > 0 && consumable(d, h.item) { format!(" [{}]", h.charges) } else { String::new() };
    format!("{}{charges}", if h.known { &it.name } else { d.t(0xee45_3c5e) })
}

fn target_ally(g: &Game, t: Target, caster: usize) -> usize {
    match t {
        Target::Myself => caster,
        _ => (0..g.party.len()).filter(|&j| g.party[j].alive()).min_by_key(|&j| 100 * g.party[j].hp / g.party[j].max_hp.max(1)).unwrap_or(caster),
    }
}

/// Casts spell `s` for member `i`.
pub fn cast(g: &mut Game, d: &GameData, t: &Tuning, i: usize, s: usize, target: Target) {
    if g.party[i].pos.floor == 0 {
        return;
    }
    if !castable(&g.party[i], d, t, s) {
        // MAINMOD2.mdrcharCheckCast's reasons.
        let why = if g.party[i].sp < sp_cost(&g.party[i], d, s) { 0x08de_a3fd } else { 0x3218_29b9 };
        return g.tell(i, CHAR_HIT, format!("{}\"{}\"!", d.t(why), d.spells[s].name), None);
    }
    if !gate(g, d, i, s) {
        return;
    }
    g.party[i].sp -= sp_cost(&g.party[i], d, s);
    effect(g, d, t, i, s, 0, target);
}

/// The combat gates and anti-magic, told the original's way; true if the spell may go.
fn gate(g: &mut Game, d: &GameData, i: usize, s: usize) -> bool {
    let sp = &d.spells[s];
    if let Some(why) = refusal(g, sp) {
        let tail = if why == 0x43b8_37ee { format!("{}.", g.party[i].name) } else { String::new() };
        g.tell(i, CHAR_HIT, format!("\"{}\"{}{tail}", sp.name, d.t(why)), None);
        return false;
    }
    let anti = g.floor(d).is_some_and(|f| f.square(g.pos().x, g.pos().y).flags & square::ANTI_MAGIC != 0);
    if anti {
        g.say(d.t(0x533a_0453));
    }
    !anti
}

/// Uses a carried item of kind `item`: drink, read or zap it.
pub fn use_item(g: &mut Game, d: &GameData, t: &Tuning, i: usize, item: usize) {
    let it = &d.items[item];
    let name = g.party[i].name.clone();
    // Only potions, scrolls, tomes and items that cast a spell can be used; a used-up
    // sword or artifact keeps its place at [0] (the store recharges it).
    if !matches!(it.kind, POTION | SCROLL | TOME) && item_spell(d, item).is_none() {
        return g.tell(i, CHAR_HIT, d.t(0x3ed9_504a).into(), None);
    }
    let Some(slot) = g.party[i].items.iter().position(|h| h.item == item && (h.charges > 0 || consumable(d, item))) else {
        let named = format!("\"{name}\"{}", d.t(0x769d_d43e));
        return g.tell(i, CHAR_HIT, d.t(0x8d26_48cf).into(), Some(named));
    };
    g.party[i].items[slot].known = true;
    match it.id {
        POTION_OF_EXPERIENCE | GREATER_POTION_OF_EXPERIENCE => {
            let mult = if it.id == POTION_OF_EXPERIENCE { 1.5 } else { 2.0 };
            let b = &mut g.party[i].buffs;
            b.xp_mult = b.xp_mult.max(mult);
            g.say(format!("{name} feels eager to learn ({mult}x experience until you change floor)."));
        }
        _ if item_spell(d, item).is_some() => {
            // Items cast at their own level (w41; -1 the user's spell level) for no SP.
            let s = item_spell(d, item).unwrap();
            if !gate(g, d, i, s) {
                return;
            }
            let level = it.w(41);
            effect(g, d, t, i, s, if level == 0 { -1 } else { level }, Target::LowestAlly);
        }
        _ => {
            // Stat potions and tomes: +1 to a stat up to the race maximum + 5 (the original).
            let stat = ["Might", "Intelligence|Learning", "Insight", "Fitness|Endurance", "Leadership", "Agility"].iter().position(|k| k.split('|').any(|k| it.name.contains(k)));
            match stat {
                Some(st) => {
                    let cap = d.races[g.party[i].race].stat_max[st] + 5;
                    let c = &mut g.party[i];
                    let was = c.stats[st];
                    c.stats[st] = (c.stats[st] + 1).min(cap);
                    let (by, stat, female) = (c.stats[st] - was, d.t(crate::game::STAT_NAMES[st]), c.female);
                    // MAINMOD2.mdrcharStatRaiseMessage
                    let you = format!("{}{stat}{}{by}.", d.t(0x5736_375e), d.t(0x0ea0_01a2));
                    let named = format!("\"{name}\"{}{}{stat}{}{by}.", d.t(0x3b35_7578), d.t(if female { 0xe43b_05c4 } else { 0x7dac_81f1 }), d.t(0xdc07_e37a));
                    g.tell(i, CHAR_HIT, you, Some(named));
                }
                None if it.name.contains("Youth") => {
                    g.party[i].age_days = g.party[i].age_days.saturating_sub(365);
                    g.tell(i, CHAR_HIT, d.t(0x7356_987a).into(), None);
                }
                None => g.tell(i, CHAR_HIT, d.t(0x9080_da93).into(), Some(format!("\"{name}\"{}", d.t(0x8366_5842)))),
            }
        }
    }
    let h = &mut g.party[i].items[slot];
    h.charges -= 1;
    if h.charges <= 0 && consumable(d, item) {
        g.party[i].items.remove(slot);
    }
}

/// What a spell does. `item` is the casting item's level (w41), -1 the caster's current
/// spell level, 0 for a spell (its caster level, fn1417).
fn effect(g: &mut Game, d: &GameData, t: &Tuning, i: usize, s: usize, item: i16, target: Target) {
    let sp = &d.spells[s];
    let caster = g.party[i].name.clone();
    let stat_bonus = |c: &Character, st: usize| 1.0 + 0.02 * f32::from((c.stats[st] - 15).max(0));
    let current = f64::from(spell_level(g.party[i].level()));
    let p = match item {
        0 => f64::from(power(&g.party[i], d, s)),
        ..0 => current,
        l => f64::from(l),
    };
    let heal_level = if item > 0 { f64::from(item) } else { current };
    // Out of a fight, alone: the original's "Fireball" thrown.
    let cast = if g.fight.is_none() && g.party.len() == 1 { format!("\"{}\"{}", sp.name, d.t(0x408b_8649)) } else { format!("{caster} casts {}.", sp.name) };
    g.say(cast);
    if sp.id == CHARM_OF_EXPERIENCE {
        for m in g.party.iter_mut().filter(|m| m.alive()) {
            m.buffs.xp_mult = m.buffs.xp_mult.max(1.5);
        }
        return g.say("The party will learn 1.5x faster until you change floor.");
    }
    match kind(sp) {
        Some(Magic::Heal | Magic::MassHeal) => {
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            for j in who {
                let heal = (hits(&mut g.party[i], sp, heal_level, true) as f32 * stat_bonus(&g.party[i], WIS)) as i32;
                let m = &mut g.party[j];
                m.hp = (m.hp + heal.max(0)).min(m.max_hp);
                // Cure also clears poison and disease.
                if sp.effect == 18 {
                    (m.status.poison, m.status.disease) = (0, 0);
                }
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
            return;
        }
        Some(Magic::Raise) => {
            // fn13D7's odds on the body's CON (Raise Dead +10, Resurrect +30); a failure
            // may bring complications, which raise anyway (fn11A1).
            let Some(j) = (0..g.party.len()).find(|&j| matches!(g.party[j].life, Life::Dead(_))) else { return };
            let m = &mut g.party[j];
            let ok = (m.rnd() * 100.0).floor() as i32 <= crate::game::raise_odds(m, i32::from(sp.effect) - 19) || crate::game::complications(d, m).is_some();
            if ok {
                m.life = Life::Alive;
                m.hp = if sp.effect == 22 { m.max_hp } else { 1 };
            }
            g.say(d.t(if ok { 0x97b7_670b } else { 0x8327_8e1b }));
            return;
        }
        Some(Magic::Protect) => {
            // Protection holds Int(spell level / 2) + 1 (fn13A8 turns it into defense).
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            for j in who {
                g.party[j].buffs.protection = g.party[j].buffs.protection.max((p / 2.0) as i32 + 1);
            }
            return;
        }
        _ => {}
    }
    match (sp.class, sp.effect) {
        (13, 50..=60) => {
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            for j in who {
                g.party[j].buffs.resist |= 1 << (sp.effect - 50);
            }
        }
        // Levitation, Sight Veil (unseen) and See Invisible: one member, or all (200+).
        (_, 3 | 4 | 5 | 203 | 204) => {
            let who: Vec<usize> = if sp.effect >= 200 { g.living() } else { vec![target_ally(g, target, i)] };
            for j in who {
                let b = &mut g.party[j].buffs;
                match sp.effect % 200 {
                    3 => b.levitate = true,
                    4 => b.invisible = true,
                    _ => b.see_invisible = true,
                }
            }
        }
        (_, 7) => {
            let floor = g.pos().floor.max(1);
            let (x, y, f) = g.random_square(d, floor);
            g.fight = None;
            g.set_pos(d, Pos { x, y, floor: f, facing: g.pos().facing });
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
                        g.hurt(d, j, i32::MAX / 2);
                    }
                    g.say(d.t(0xf350_a09d));
                }
            }
        }
        (_, 11) => {
            let Some(f) = g.floor(d) else { return };
            let pos = g.pos();
            let rock = crate::game::ahead(f, pos.x, pos.y, pos.facing).is_some_and(|(x, y)| f.square(x, y).flags & square::ROCK != 0);
            g.say(d.t(if rock { 0x2e1c_8b23 } else { 0x69ab_64ec }));
        }
        (_, 12) => {
            let floor = g.pos().floor;
            g.say(format!("{} {floor}.", d.t(0x5e6f_feb9)));
        }
        (_, 13) => {
            let pos = g.pos();
            g.say(format!("{}{}.", d.t(0x0b58_e5e4), d.t(crate::game::DIRECTIONS[pos.facing as usize])));
        }
        (_, 14 | 15) => {
            // Locate: how far right, forward and down each lost member lies.
            let at = g.pos();
            let lost: Vec<String> = g.party.iter().filter_map(|m| match m.life {
                Life::Dead(p) => Some(format!("\"{}\"{}{}{}{}{}{}.", m.name, d.t(0x245a_5613), p.x as i64 - at.x as i64, d.t(0x072c_223a), p.y as i64 - at.y as i64, d.t(0xba09_ee62), p.floor as i64 - at.floor as i64)),
                _ => None,
            }).collect();
            if lost.is_empty() {
                g.say("You sense no lost souls.");
            }
            lost.into_iter().for_each(|l| g.say(l));
        }
        (_, 16) => {
            let pos = g.pos();
            g.party[i].sanctuary = Some(pos);
        }
        (_, 17) => match g.party[i].sanctuary {
            Some(to) => {
                g.fight = None;
                g.party[i].age_days += 365;
                g.set_pos(d, to);
            }
            None => g.say(format!("\"{}\"{}", d.t(0x1536_6ebd), d.t(0xd748_25b0))),
        },
        (_, 19) => {
            // Fate (sub0F29): the caster's hits and SP re-rolled from 1 to their max.
            let max_sp = g.party[i].max_sp(d, t);
            let m = &mut g.party[i];
            m.hp = (m.rnd() * f64::from(m.max_hp)).floor() as i32 + 1;
            m.sp = (m.rnd() * f64::from(max_sp)).floor() as i32 + 1;
        }
        (_, 21) => {
            // Restore Flesh: fn13D7's odds with +20.
            for m in g.party.iter_mut().filter(|m| m.status.stoned) {
                if (m.rnd() * 100.0).floor() as i32 <= crate::game::raise_odds(m, 2) {
                    m.status.stoned = false;
                }
            }
        }
        (_, 30) => crate::treasure::unlock(g, d),
        // Control, Restrain and Domination bind a companion to the spell's level + ln((10 +
        // spell level) / 5), Life Domination for life (mdrcharCastSpell).
        (8, _) => {
            let bind = if sp.effect == 150 { 100.0 } else { f64::from(sp.level) + ((10.0 + p) / 5.0).ln() };
            if let Some(c) = g.party[0].companions.iter_mut().min_by_key(|c| c.bind) {
                c.bind = c.bind.max(bind as u8);
            }
        }
        _ => offensive(g, d, t, i, s, p, target),
    }
}

/// Damage, kill, charm, banish and dispel spells (mdrcharCastSpell's combat branch): up
/// to w7 groups from the targeted one, up to w6 monsters of each, one after another; the
/// first that survives ends that group. Kill spells (effect 100) and charms work only
/// when the hits reach the monster's own; flag 4 resists all spells, flag 8 charms, and
/// typed spells (effects 75-99) touch only their monster family (fn0E5F).
fn offensive(g: &mut Game, d: &GameData, t: &Tuning, i: usize, s: usize, p: f64, target: Target) {
    let sp = &d.spells[s];
    let Some(f) = g.fight.as_ref() else { return };
    let first = f.pick(target).map_or(0, |fi| f.foes[fi].group);
    let groups = f.groups_from(first);
    if sp.class == 7 && g.party[0].companions.len() >= crate::monsters::MAX_COMPANIONS {
        return g.say(format!("\"{}\"{}", sp.name, d.t(0x92fb_ac7a)));
    }
    g.provoke();
    let int_bonus = 1.0 + 0.02 * f64::from((g.party[i].stats[INT] - 15).max(0));
    // Sorcerers' elemental spells ignore half of a monster's resistance.
    let pierce = if g.party[i].guild == SORCERER && [0, 1, 2, 5].contains(&sp.class) { 0.5 } else { 1.0 };
    let wizard = g.party[i].guild == WIZARD && school(WIZARD, sp.class);
    let (mut n, mut hurt, mut charmed) = (0, 0, 0);
    for gr in groups.into_iter().take(sp.groups.max(1) as usize) {
        for _ in 0..sp.targets.max(1) {
            let Some((fi, m, hp)) = g.fight.as_ref().and_then(|f| f.front(gr).map(|fi| (fi, f.foes[fi].monster, f.foes[fi].hp))) else { break };
            let mon = &d.monsters[m];
            let res = if sp.resist >= 0 { f64::from(mon.resist[sp.resist as usize].clamp(0, 100)) * pierce } else { 0.0 };
            let mut dmg = (hits(&mut g.party[i], sp, p, false) * int_bonus * (1.0 - res / 100.0)).round_ties_even() as i32;
            let family = (75..100).contains(&sp.effect) && d.monster_type_family.get(mon.kind as usize).copied() != Some(sp.effect - 75);
            if dmg < 1 || mon.flags & 4 != 0 || sp.class == 7 && mon.flags & 8 != 0 || family {
                dmg = 0;
            }
            if (sp.effect == 100 || sp.class == 7) && dmg < hp || dmg == 0 {
                break;
            }
            if sp.class == 7 {
                // fn10BA: it joins with the hits it had, bound at the spell's level.
                g.award(d, t, i, m, hp, 1.0);
                g.fight.as_mut().unwrap().foes[fi].hp = 0;
                let bind = sp.level.clamp(1, 100) as u8;
                let bound = crate::monsters::bind_steps(&g.party[i], bind);
                g.party[0].companions.push(crate::monsters::Companion { monster: m, hp, max_hp: hp, bound, bind });
                charmed += 1;
                if g.party[0].companions.len() >= crate::monsters::MAX_COMPANIONS {
                    break;
                }
                continue;
            }
            if !g.damage_foe(d, t, i, fi, dmg, "blasts", None, 1.0) {
                hurt = dmg;
                break;
            }
            n += 1;
            if wizard {
                restore_sp(g, d, t, i);
            }
        }
    }
    if charmed > 0 {
        let you = format!("{}{}!", d.t(0xee68_3ce2), crate::game::count_word(d, charmed));
        g.fought(i, you, format!("{} charms {charmed}.", g.party[i].name));
    } else {
        g.said_cast(d, i, &sp.name, 0x17f1_3590, n, hurt);
    }
}

impl Game {
    /// The controlled character's label after an attack spell (mdrcharCastSpell):
    /// "You Killed Two!", "You Killed One and damaged one for 9!", "You damaged one for
    /// 9!", or "You failed in casting "Fireball"!".
    fn said_cast(&mut self, d: &GameData, i: usize, spell: &str, verb: u32, n: usize, hurt: i32) {
        if i != 0 {
            return;
        }
        let you = d.t(0x35d0_2f14);
        self.said[CHAR_HIT] = match (n, hurt) {
            (0, 0) => format!("{you}{}\"{spell}\"!", d.t(0x5d5d_7642)),
            (0, h) => format!("{you}{} {h}!", d.t(0x30bb_da77)),
            (n, 0) => format!("{you}{} {}!", d.t(verb), crate::game::count_word(d, n)),
            (n, h) => format!("{you}{} {}{} {h}!", d.t(verb), crate::game::count_word(d, n), d.t(0xa013_c0c8)),
        };
    }
}

/// Wizards regain a tenth of their SP for a kill with a Wizard spell.
fn restore_sp(g: &mut Game, d: &GameData, t: &Tuning, i: usize) {
    let max = g.party[i].max_sp(d, t);
    g.party[i].sp = (g.party[i].sp + max / 10).min(max);
}
