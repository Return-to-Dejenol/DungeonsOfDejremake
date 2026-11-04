//! Monsters' side of combat: peaceful groups, attacks and special attacks (poison,
//! breath, theft...), monster spells, and companions won by charming or bought in
//! Holding & Confinement.

use serde::{Deserialize, Serialize};

use crate::{
    Tuning,
    combat::hit_chance,
    game::{CHA, CON, Character, Game, WIS},
    mdr::GameData,
};

/// A monster fighting for the party.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Companion {
    /// Index into `GameData::monsters`.
    pub monster: usize,
    pub hp: i32,
    pub max_hp: i32,
    /// Steps left before a bound companion breaks free (or a free one wanders off).
    pub bound: u32,
}

pub const MAX_COMPANIONS: usize = 4;

const POISON: u16 = 1;
const DISEASE: u16 = 2;
const PARALYSIS: u16 = 4;
const FIRE_BREATH: u16 = 8;
const COLD_BREATH: u16 = 16;
const ACID: u16 = 32;
const ELECTRIC_BREATH: u16 = 64;
const DRAIN: u16 = 128;
const STONE: u16 = 256;
const AGE: u16 = 512;
const CRITICAL: u16 = 1024;
const STEAL_GOLD: u16 = 2048;
const DESTROY: u16 = 4096;
const STEAL_ITEM: u16 = 8192;

/// Resistance chance for slot `r` (race order): the race's, plus 50 for a resist spell,
/// plus CON (body: poison, disease, stone, paralysis, breath) or WIS (monster spells)
/// at 3% a point above 12.
fn resists(c: &Character, d: &GameData, slot: usize, stat: usize) -> f64 {
    let race = f64::from(d.races[c.race].resist.get(slot).copied().unwrap_or(0).max(0));
    let spell = if c.buffs.resist & (1 << slot) != 0 { 50.0 } else { 0.0 };
    let stat = 3.0 * f64::from((c.stats[stat] - 12).max(0));
    ((race + spell + stat) / 100.0).min(0.95)
}

/// Whether a new group is peaceful: the monster's own chance, better with CHA.
pub fn peaceful(g: &mut Game, d: &GameData, m: usize) -> bool {
    if g.party.iter().any(|c| c.quest == Some(m)) {
        return false;
    }
    let cha = f64::from(g.party[0].stats[CHA] - 10) * 0.03;
    let p = f64::from(d.monsters[m].peaceful.max(0)) / 100.0 * (1.0 + cha);
    g.party[0].chance(p)
}

/// Every awake foe attacks a random living member or companion.
pub fn attack(g: &mut Game, d: &GameData, t: &Tuning) {
    let Some(f) = g.fight.as_ref() else { return };
    let attackers: Vec<(usize, usize)> = f.alive().filter(|&fi| f.foes[fi].asleep == 0).map(|fi| (fi, f.foes[fi].monster)).collect();
    for (fi, m) in attackers {
        if g.fight.is_none() || g.living().is_empty() {
            return;
        }
        if g.fight.as_ref().is_some_and(|f| f.foes[fi].hp <= 0) {
            continue;
        }
        let mon = &d.monsters[m];
        let living = g.living();
        let n = living.len() + g.party[0].companions.len();
        let pick = g.party[0].roll(n as u32) as usize;
        if pick >= living.len() {
            hit_companion(g, d, t, m, pick - living.len());
            continue;
        }
        let j = living[pick];
        if mon.spells != 0 && mon.magic_level > 0 && g.party[0].chance(0.3) {
            monster_spell(g, d, m, j);
            continue;
        }
        let (_, mut def) = g.party[j].att_def(d, t);
        if g.fight.as_ref().is_some_and(|f| f.defending.get(j).copied().unwrap_or(false)) {
            def += def / 2;
        }
        if mon.specials & (FIRE_BREATH | COLD_BREATH | ELECTRIC_BREATH) != 0 && g.party[0].chance(0.2) {
            breath(g, d, m);
            continue;
        }
        if !g.party[j].chance(hit_chance(&t.combat, i32::from(mon.attack), def)) {
            continue;
        }
        let mut dmg = 1 + g.party[j].roll(mon.level.max(1) as u32 * t.combat.monster_die + (mon.attack.max(0) / 20) as u32) as i32;
        if mon.specials & CRITICAL != 0 && g.party[j].chance(0.1) {
            dmg *= 2;
            g.say(format!("The {} lands a critical hit!", mon.name));
        }
        let name = g.party[j].name.clone();
        g.say(format!("The {} hits {name} for {dmg}.", mon.name));
        if g.hurt(j, dmg) {
            continue;
        }
        specials(g, d, fi, m, j);
    }
}

fn hit_companion(g: &mut Game, d: &GameData, t: &Tuning, m: usize, k: usize) {
    let mon = &d.monsters[m];
    let comp = &d.monsters[g.party[0].companions[k].monster];
    if !g.party[0].chance(hit_chance(&t.combat, i32::from(mon.attack), i32::from(comp.defense))) {
        return;
    }
    let dmg = 1 + g.party[0].roll(mon.level.max(1) as u32 * t.combat.monster_die) as i32;
    let c = &mut g.party[0].companions[k];
    c.hp -= dmg;
    let name = comp.name.clone();
    if c.hp <= 0 {
        g.party[0].companions.remove(k);
        g.say(format!("The {} kills your {name}.", mon.name));
    } else {
        g.say(format!("The {} hits your {name} for {dmg}.", mon.name));
    }
}

/// On a hit, each special attack the monster has may land (one in four), resisted by
/// the matching resistance.
fn specials(g: &mut Game, d: &GameData, fi: usize, m: usize, j: usize) {
    let mon = &d.monsters[m];
    let lvl = mon.level.max(1) as u16;
    let name = g.party[j].name.clone();
    for (bit, slot) in [(POISON, 5), (DISEASE, 4), (PARALYSIS, 8), (STONE, 7), (DRAIN, 9), (ACID, 10), (AGE, 99)] {
        if mon.specials & bit == 0 || !g.party[j].chance(0.25) {
            continue;
        }
        let res = if slot == 99 { 0.0 } else { resists(&g.party[j], d, slot, CON) };
        if g.party[j].chance(res) {
            continue;
        }
        let c = &mut g.party[j];
        let msg = match bit {
            POISON => {
                c.status.poison = c.status.poison.max(10 + 2 * lvl);
                format!("{name} is poisoned!")
            }
            DISEASE => {
                c.status.disease = c.status.disease.max(20 + 3 * lvl);
                format!("{name} is diseased!")
            }
            PARALYSIS => {
                c.status.paralysis = c.status.paralysis.max(3 + lvl / 2);
                format!("{name} is paralysed!")
            }
            STONE => {
                c.status.stoned = true;
                format!("{name} is turned to stone!")
            }
            DRAIN => {
                let st = c.roll(6) as usize;
                let min = d.races[c.race].stat_min[st];
                if c.stats[st] > min {
                    c.stats[st] -= 1;
                }
                c.max_hp = (c.max_hp - 1).max(1);
                format!("{name} feels drained!")
            }
            ACID => {
                let dmg = 1 + c.roll(lvl as u32 * 2) as i32;
                c.hp -= dmg;
                format!("Acid burns {name} for {dmg}!")
            }
            _ => {
                let years = 1 + c.roll(5);
                c.age_days += 365 * years;
                format!("{name} ages {years} years!")
            }
        };
        g.say(msg);
        if g.party[j].hp <= 0 {
            g.party[j].hp = 1;
            g.hurt(j, 1);
        }
    }
    let c = &mut g.party[j];
    if mon.specials & STEAL_GOLD != 0 && c.gold > 0 && c.chance(0.2) {
        let took = c.gold * u64::from(10 + c.roll(21)) / 100;
        c.gold -= took;
        g.say(format!("The {} steals {took} gold from {name} and runs!", mon.name));
        return flee(g, fi);
    }
    if mon.specials & STEAL_ITEM != 0 && c.chance(0.15) {
        let loose: Vec<usize> = (0..c.items.len()).filter(|&s| !c.items[s].equipped).collect();
        if !loose.is_empty() {
            let s = loose[c.roll(loose.len() as u32) as usize];
            let h = c.items.remove(s);
            g.say(format!("The {} steals {name}'s {} and runs!", mon.name, d.items[h.item].name));
            return flee(g, fi);
        }
    }
    if mon.specials & DESTROY != 0 && c.chance(0.1) && !c.items.is_empty() {
        let s = c.roll(c.items.len() as u32) as usize;
        // Weak monsters can't destroy powerful items.
        if d.items[c.items[s].item].level <= mon.level {
            let h = c.items.remove(s);
            g.say(format!("The {} destroys {name}'s {}!", mon.name, d.items[h.item].name));
        }
    }
}

fn flee(g: &mut Game, fi: usize) {
    if let Some(f) = g.fight.as_mut() {
        f.foes[fi].hp = 0;
    }
}

/// Breath hits everyone for the monster's level x 2-4, halved on a resisted save.
fn breath(g: &mut Game, d: &GameData, m: usize) {
    let mon = &d.monsters[m];
    let (slot, kind) = if mon.specials & FIRE_BREATH != 0 { (0, "fire") } else if mon.specials & COLD_BREATH != 0 { (1, "frost") } else { (2, "lightning") };
    g.say(format!("The {} breathes {kind}!", mon.name));
    for j in g.living() {
        let mut dmg = i32::from(mon.level.max(1)) * (2 + g.party[j].roll(3) as i32);
        let res = resists(&g.party[j], d, slot, CON);
        if g.party[j].chance(res) {
            dmg /= 2;
        }
        g.hurt(j, dmg);
    }
}

/// A monster spell at its casting level, resisted by WIS (docs/balance.md).
fn monster_spell(g: &mut Game, d: &GameData, m: usize, j: usize) {
    let mon = &d.monsters[m];
    let classes: Vec<usize> = (0..8).filter(|b| mon.spells >> b & 1 != 0).collect();
    let class = classes[g.party[0].roll(classes.len() as u32) as usize];
    let slot = [0, 1, 2, 3, 6, 5, 6, 6][class];
    let name = g.party[j].name.clone();
    let res = resists(&g.party[j], d, slot, WIS);
    if g.party[j].chance(res) {
        return g.say(format!("{name} resists the {}'s spell.", mon.name));
    }
    let dmg = i32::from(mon.magic_level) * (1 + g.party[j].roll(3) as i32);
    g.say(format!("The {} casts a spell at {name} for {dmg}!", mon.name));
    g.hurt(j, dmg);
}

/// Mage's passive (docs/balance.md): binds last longer and hold harder, with CHA.
fn bind_strength(c: &Character) -> f32 {
    let mage = if c.levels[8] > 0 { 1.5 } else { 1.0 };
    mage * (1.0 + 0.04 * f32::from((c.stats[CHA] - 10).max(0)))
}

/// Charm and bind spells: the targeted kind of monster may join as a companion.
pub fn charm(g: &mut Game, d: &GameData, i: usize, s: usize, p: f32) {
    g.provoke();
    let sp = &d.spells[s];
    let kind = (75..=86).contains(&sp.effect).then(|| sp.effect - 75).filter(|&k| k != 5);
    let Some(f) = g.fight.as_ref() else { return };
    let Some(fi) = f.alive().find(|&fi| kind.is_none_or(|k| d.monsters[f.foes[fi].monster].kind == k)) else {
        return g.say("No monster here answers that charm.");
    };
    if g.party[0].companions.len() >= MAX_COMPANIONS {
        return g.say("You can't hold more companions.");
    }
    let (m, hp, max_hp) = (f.foes[fi].monster, f.foes[fi].hp, f.foes[fi].max_hp);
    let strength = bind_strength(&g.party[i]);
    let p_ok = (0.35 + 0.05 * (p - f32::from(d.monsters[m].level))) * strength;
    if !g.party[i].chance(f64::from(p_ok.clamp(0.05, 0.9))) {
        return g.say(format!("The {} resists.", d.monsters[m].name));
    }
    g.fight.as_mut().unwrap().foes[fi].hp = 0;
    let bound = (100.0 * strength) as u32 + 10 * p as u32;
    g.party[0].companions.push(Companion { monster: m, hp, max_hp, bound });
    g.say(format!("The {} joins you as a companion!", d.monsters[m].name));
}

impl Game {
    /// Companions strike the weakest foe each round.
    pub(crate) fn companions_act(&mut self, d: &GameData, t: &Tuning) {
        for k in 0..self.party[0].companions.len() {
            let Some(f) = self.fight.as_ref() else { return };
            if f.peaceful {
                return;
            }
            let Some(fi) = f.pick(crate::ai::Target::Weakest) else { return };
            let c = &d.monsters[self.party[0].companions[k].monster];
            let foe = &d.monsters[f.foes[fi].monster];
            if self.party[0].chance(hit_chance(&t.combat, i32::from(c.attack), i32::from(foe.defense))) {
                let dmg = 1 + self.party[0].roll(c.level.max(1) as u32 * t.combat.monster_die + 2) as i32;
                let foe_hp = &mut self.fight.as_mut().unwrap().foes[fi].hp;
                *foe_hp -= dmg;
                let killed = *foe_hp <= 0;
                self.say(format!("Your {} hits the {} for {dmg}{}", c.name, foe.name, if killed { " and kills it!" } else { "." }));
                if killed {
                    let m = self.fight.as_ref().unwrap().foes[fi].monster;
                    self.kill(d, t, 0, m);
                }
            }
        }
    }

    /// Each step a bound companion's hold weakens; an unhappy (hurt) one may leave.
    pub(crate) fn companions_step(&mut self, d: &GameData) {
        let lead = &mut self.party[0];
        let mut gone = Vec::new();
        for k in (0..lead.companions.len()).rev() {
            let c = &mut lead.companions[k];
            c.bound = c.bound.saturating_sub(1);
            let unhappy = c.hp * 3 < c.max_hp;
            if c.bound == 0 || (unhappy && lead.rng_chance(0.02)) {
                gone.push(lead.companions.remove(k).monster);
            }
        }
        for m in gone {
            self.say(format!("Your {} wanders off.", d.monsters[m].name));
        }
    }
}

impl Character {
    fn rng_chance(&mut self, p: f64) -> bool {
        self.chance(p)
    }
}

/// Holding & Confinement's stock: tame monsters of the first floors, by price.
pub fn confinement_stock(d: &GameData) -> Vec<(usize, u64)> {
    let mut v: Vec<(usize, u64)> = (0..d.monsters.len())
        .filter(|&m| (1..=4).contains(&d.monsters[m].level) && d.monsters[m].peaceful >= 30)
        .map(|m| (m, confinement_price(d, m)))
        .collect();
    v.sort_by_key(|&(_, p)| p);
    v.truncate(24);
    v
}

pub fn confinement_price(d: &GameData, m: usize) -> u64 {
    let mon = &d.monsters[m];
    100 * u64::from(mon.level.max(1) as u16).pow(2) * u64::from(mon.hp.max(1) as u16)
}

impl Game {
    pub fn buy_companion(&mut self, d: &GameData, m: usize) -> Result<String, String> {
        let price = confinement_price(d, m);
        let lead = &mut self.party[0];
        if lead.companions.len() >= MAX_COMPANIONS {
            return Err("You can't hold more companions.".into());
        }
        if lead.gold + lead.bank < price {
            return Err("You can't afford that.".into());
        }
        lead.pay(price);
        let hp = i32::from(d.monsters[m].hp.max(1));
        let bound = (300.0 * bind_strength(lead)) as u32;
        lead.companions.push(Companion { monster: m, hp, max_hp: hp, bound });
        Ok(format!("The {} is yours, bound for a while.", d.monsters[m].name))
    }

    /// Sells at a third of the price, plus CHA's 1% a point above 10.
    pub fn sell_companion(&mut self, d: &GameData, k: usize) -> String {
        let lead = &mut self.party[0];
        let c = lead.companions.remove(k);
        let price = (confinement_price(d, c.monster) as f64 / 3.0 * (1.0 + f64::from((lead.stats[CHA] - 10).max(0)) / 100.0)) as u64;
        lead.gold += price;
        format!("Sold the {} for {price} gold.", d.monsters[c.monster].name)
    }

    /// Rebinding costs a tenth of the price and renews the bind.
    pub fn rebind(&mut self, d: &GameData, k: usize) -> Result<String, String> {
        let lead = &mut self.party[0];
        let price = confinement_price(d, lead.companions[k].monster) / 10;
        if lead.gold + lead.bank < price {
            return Err(format!("Rebinding costs {price} gold."));
        }
        lead.pay(price);
        let bound = (300.0 * bind_strength(lead)) as u32;
        lead.companions[k].bound = lead.companions[k].bound.max(bound);
        lead.companions[k].hp = lead.companions[k].max_hp;
        Ok(format!("The bindsmen renew your hold on the {}.", d.monsters[lead.companions[k].monster].name))
    }
}
