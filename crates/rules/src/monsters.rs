//! Monsters' side of combat as MORDOR.EXE runs it: peaceful and hostile groups, the
//! group's attacks and special attacks (poison, breath, theft...), monster spells, and
//! companions won by charming or bought in Holding & Confinement.

use serde::{Deserialize, Serialize};

use crate::{
    Tuning,
    combat::{size_term, swing},
    game::{CHA, CON, Character, DEX, Game, THIEVING, WIS},
    mdr::{GameData, Monster, Words, square},
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
    /// The original's binding level (up to 12), raised by the bindsmen.
    #[serde(default)]
    pub bind: u8,
}

pub const MAX_COMPANIONS: usize = 4;

const POISON: u16 = 1;
const DISEASE: u16 = 2;
const PARALYZE: u16 = 4;
const FIRE_BREATH: u16 = 8;
const COLD_BREATH: u16 = 16;
const ACID: u16 = 32;
const ELECTROCUTE: u16 = 64;
const DRAIN: u16 = 128;
const STONE: u16 = 256;
const AGE: u16 = 512;
const CRITICAL: u16 = 1024;
const BACKSTAB: u16 = 2048;
const DESTROY: u16 = 4096;
const STEAL: u16 = 8192;
const SEES_INVISIBLE: i16 = 1;

// Resistance slots, in the race order.
const FIRE: usize = 0;
const COLD: usize = 1;
const ELECTRIC: usize = 2;
const DISEASE_RES: usize = 4;
const POISON_RES: usize = 5;
const STONE_RES: usize = 7;
const PARALYSIS_RES: usize = 8;
const DRAIN_RES: usize = 9;
const ACID_RES: usize = 10;

/// Resistance in percent for slot `r`: the race's, or 90 from an item or spell that
/// grants it (the original lets an item's 90 replace a higher race value; the higher
/// counts here).
pub(crate) fn resist(c: &Character, d: &GameData, slot: usize) -> i32 {
    let race = i32::from(d.races[c.race].resist.get(slot).copied().unwrap_or(0));
    let item = c.items.iter().any(|h| h.equipped && d.items[h.item].long(24) >> slot & 1 != 0);
    if item || c.buffs.resist & (1 << slot) != 0 { race.max(90) } else { race }
}

/// Whether a new group is peaceful: a good group meeting a good leader, or a neutral
/// group 40% of the time (MORDOR.EXE). Quest monsters always fight.
pub fn peaceful(g: &mut Game, d: &GameData, m: usize) -> bool {
    if g.party.iter().any(|c| c.quest == Some(m)) {
        return false;
    }
    match d.monsters[m].w(31) {
        0 => g.party[0].alignment == 0,
        1 => g.party[0].chance(0.4),
        _ => false,
    }
}

/// Each round a peaceful group may turn: a chance by the two alignments, less
/// 7.5 x ln CHA of the leader.
pub(crate) fn turn_hostile(g: &mut Game, d: &GameData) {
    let Some(f) = g.fight.as_ref().filter(|f| f.peaceful) else {
        return;
    };
    let Some(m) = f.alive().next().map(|fi| f.foes[fi].monster) else {
        return;
    };
    let base = match (d.monsters[m].w(31), g.party[0].alignment) {
        (0, 1) => 15,
        (0, 2) => 100,
        (0, _) => 0,
        (1, 1) => 15,
        (1, _) => 25,
        (_, 2) => 25,
        _ => 35,
    };
    let chance = base - (7.5 * f64::from(g.party[0].stats[CHA].max(1)).ln()).round_ties_even() as i32;
    if g.party[0].d100() <= chance {
        g.provoke();
    }
}

/// The odds of breath, acid and electrocution: 50%, or 80% (90% for acid and shocks)
/// against a target above 1.25 x the monster's level, 20% (10%) below 0.75 x.
fn odds(level: u32, mlevel: i16, high: i32) -> i32 {
    let (l, m) = (f64::from(level), f64::from(mlevel));
    if l > 1.25 * m {
        high
    } else if l < 0.75 * m {
        100 - high
    } else {
        50
    }
}

/// Breath, acid and electric damage, N-based (MORDOR.EXE): N grows with the monster's
/// level and shrinks with the target's.
fn element_hit(c: &mut Character, x: f64, level: f64, per: f64, quarter: bool) -> i32 {
    let n = (((x.max(1.0)).ln() + x / 15.0) / 1.08f64.ln() - level / per + 1.0).floor().max(0.0);
    let bonus = if quarter { n / 4.0 } else { n / 2.0 };
    (((c.rnd() * n + 1.0) / 2.0).floor() + bonus) as i32
}

/// The target member for a group's round: 100/50/25/12/6 by party position.
fn target(g: &mut Game) -> Option<usize> {
    let living: Vec<usize> = g.living().into_iter().filter(|&j| !g.party[j].status.stoned).collect();
    let weights: Vec<u32> = (0..living.len()).map(|k| 100 >> k.min(6)).collect();
    let mut r = g.party[0].roll(weights.iter().sum::<u32>().max(1));
    for (k, w) in weights.iter().enumerate() {
        if r < *w {
            return Some(living[k]);
        }
        r -= w;
    }
    living.first().copied()
}

/// The group's round: breath or acid first, then up to five of its members attack one
/// character (or its companions), then the special attacks of those that hit.
pub fn attack(g: &mut Game, d: &GameData, t: &Tuning) {
    let Some(f) = g.fight.as_ref() else { return };
    let awake: Vec<usize> = f.alive().filter(|&fi| f.foes[fi].asleep == 0).take(5).collect();
    let Some(&first) = awake.first() else { return };
    let m = f.foes[first].monster;
    let mon = &d.monsters[m];
    let opener = f.opening.map(|(j, _)| j);
    let defending = f.defending.clone();
    let Some(j) = target(g) else { return };
    let (lcur, top) = (g.party[j].level(), *g.party[j].levels.iter().max().unwrap_or(&1));
    if mon.specials & (FIRE_BREATH | COLD_BREATH) != 0 && g.party[j].d100() <= odds(top, mon.power_level, 80) {
        let (slot, kind) = if mon.specials & FIRE_BREATH != 0 { (FIRE, "fire") } else { (COLD, "frost") };
        let ml = f64::from(mon.power_level);
        let x = (ml * (ml + 1.0).ln()).round_ties_even();
        let dmg = (f64::from(element_hit(&mut g.party[j], x, f64::from(lcur), 20.0, true)) * 0.7 / 2.0).round_ties_even() as i32;
        g.say(format!("The {} breathes {kind}!", mon.name));
        let dmg = reduce(dmg, resist(&g.party[j], d, slot));
        companion_damage(g, d, None, dmg / 2);
        if hurt(g, j, dmg) {
            return;
        }
    }
    if mon.specials & ACID != 0 && g.party[j].d100() <= odds(lcur, mon.power_level, 90) {
        let dmg = (f64::from(element_hit(&mut g.party[j], f64::from(mon.power_level) + 1.0, f64::from(lcur), 10.0, false)) * 0.7).round_ties_even() as i32;
        let dmg = reduce(dmg, resist(&g.party[j], d, ACID_RES));
        let name = g.party[j].name.clone();
        g.say(format!("The {}'s acid burns {name} for {dmg}!", mon.name));
        if hurt(g, j, dmg) {
            return;
        }
    }
    let (mut total, mut last_hit) = (0, false);
    for _ in &awake {
        if g.fight.is_none() || !g.party[j].alive() {
            return;
        }
        if mon.spells != 0 && monster_spell(g, d, m, j) {
            return;
        }
        let comps = g.party[j].companions.len();
        if comps > 0 && g.party[j].d100() >= 100 / (comps as i32 + 1) {
            let k = g.party[j].roll(comps as u32) as usize;
            last_hit = hit_companion(g, d, m, j, k);
            continue;
        }
        let c = &g.party[j];
        let mut mult = f64::from(mon.single(46));
        if mult <= 0.0 {
            mult = 1.0;
        }
        if opener == Some(j) {
            mult += 0.75;
        }
        let (_, mut def) = c.att_def(d, t);
        if defending.get(j).copied().unwrap_or(false) {
            def += def / 2;
        }
        mult += knack_bonus(g, mon, lcur, 2.0);
        mult -= size_term(d, &g.party[j], m);
        let dmg = swing(&mut g.party[j], i32::from(mon.attack), def, f64::from(mon.power_level), i32::from(mon.stats[0]), mult);
        last_hit = dmg > 0;
        total += dmg;
    }
    let name = g.party[j].name.clone();
    if total > 0 {
        g.say(format!("The {}{} hit {name} for {total}.", mon.name, if awake.len() > 1 { "s" } else { "" }));
        if hurt(g, j, total) {
            return;
        }
        if last_hit {
            specials(g, d, m, j);
        }
        if mon.specials & STONE != 0 && g.party[j].alive() {
            stone(g, d, m, j);
        }
    } else {
        g.say(format!("The {} miss {name}.", mon.name));
    }
    if mon.specials & ELECTROCUTE != 0 && total > 0 && g.party[j].alive() && g.party[j].d100() <= odds(lcur, mon.power_level, 90) {
        let dmg = (f64::from(element_hit(&mut g.party[j], f64::from(mon.power_level) + 1.0, f64::from(lcur), 10.0, false)) * 0.7).round_ties_even() as i32;
        let dmg = reduce(dmg, resist(&g.party[j], d, ELECTRIC));
        g.say(format!("{name} is electrocuted for {dmg}!"));
        if hurt(g, j, dmg) {
            return;
        }
    }
    if g.party[j].alive() {
        thieves(g, d, m, j, total);
    }
}

/// A monster's critical hit (+`crit`: 2, or 4 for a companion) and backstab (+1) to its
/// multiplier (MORDOR.EXE).
fn knack_bonus(g: &mut Game, mon: &Monster, lcur: u32, crit: f64) -> f64 {
    let (ml, lv, l) = (f64::from(mon.power_level), f64::from(mon.level), f64::from(lcur));
    let c = &mut g.party[0];
    let mut mult = 0.0;
    let chance = |div: f64| (ml / div + 1.0).ln() * 100.0 * (lv + 1.0).ln() - (l / 999.0 + 1.0).ln() * 100.0;
    if mon.specials & CRITICAL != 0 && f64::from(c.d100()) <= chance(650.0).clamp(2.0, 50.0) {
        mult += crit;
    }
    if mon.specials & BACKSTAB != 0 && f64::from(c.d100()) <= chance(500.0).clamp(4.0, 75.0) {
        mult += 1.0;
    }
    mult
}

/// Takes half the resistance off: `dmg x (1 - res / 200)`.
fn reduce(dmg: i32, res: i32) -> i32 {
    (f64::from(dmg) * (1.0 - f64::from(res.clamp(0, 100)) / 200.0)) as i32
}

fn hurt(g: &mut Game, j: usize, dmg: i32) -> bool {
    dmg > 0 && g.hurt(j, dmg)
}

/// Damage to every companion of `owner` (all members' when `None`).
fn companion_damage(g: &mut Game, d: &GameData, owner: Option<usize>, dmg: i32) {
    for j in owner.map_or_else(|| (0..g.party.len()).collect(), |j| vec![j]) {
        for k in (0..g.party[j].companions.len()).rev() {
            let c = &mut g.party[j].companions[k];
            c.hp -= dmg;
            if c.hp <= 0 {
                let m = g.party[j].companions.remove(k).monster;
                g.say(format!("The {} dies.", d.monsters[m].name));
            }
        }
    }
}

/// One attacker against companion `k` of member `j`; true on a hit.
fn hit_companion(g: &mut Game, d: &GameData, m: usize, j: usize, k: usize) -> bool {
    let mon = &d.monsters[m];
    let comp = &d.monsters[g.party[j].companions[k].monster];
    let dmg = swing(&mut g.party[j], i32::from(mon.attack), i32::from(comp.defense), f64::from(mon.power_level), i32::from(mon.stats[0]), f64::from(mon.single(46)).max(1.0));
    if dmg == 0 {
        return false;
    }
    let c = &mut g.party[j].companions[k];
    c.hp -= dmg;
    let name = comp.name.clone();
    if c.hp <= 0 {
        g.party[j].companions.remove(k);
        g.say(format!("The {} kills your {name}.", mon.name));
    } else {
        g.say(format!("The {} hits your {name} for {dmg}.", mon.name));
    }
    true
}

/// Poison, disease, paralysis, aging and draining, after a hit that hurt (MORDOR.EXE's
/// rolls against resistance and CON).
fn specials(g: &mut Game, d: &GameData, m: usize, j: usize) {
    let mon = &d.monsters[m];
    let (w7, w37) = (mon.level.max(1), mon.power_level);
    let name = g.party[j].name.clone();
    let con = f64::from(g.party[j].stats[CON]);
    let body = (con * 2.5 - 2.0 * f64::from(w7)) as i32;
    let c = &mut g.party[j];
    let mut said = Vec::new();
    if mon.specials & AGE != 0 && c.d100() > body {
        let years = (f64::from(w37) / 50.0).round_ties_even().clamp(1.0, 5.0) as u32;
        let days = (c.roll(1825) + 1).min(years * 365);
        c.age_days += days;
        said.push(format!("{name} ages {days} days!"));
    }
    if mon.specials & DRAIN != 0 {
        for _ in 0..(c.rnd() * (f64::from(w7) / 2.0 + 0.5)) as u32 + 1 {
            if c.d100() > resist(c, d, DRAIN_RES) && c.d100() > body {
                // The original never drains STR; here every stat can go.
                match c.roll(7) as usize {
                    6 => c.max_hp = (c.max_hp - 2).max(1),
                    s if c.stats[s] > d.races[c.race].stat_min[s] => c.stats[s] -= 1,
                    _ => {}
                }
                said.push(format!("{name} is drained!"));
            }
        }
    }
    let lvl = w7 as u16;
    if mon.specials & POISON != 0 && c.d100() > resist(c, d, POISON_RES) {
        c.status.poison = c.status.poison.max(lvl);
        said.push(format!("{name} is poisoned!"));
    }
    if mon.specials & DISEASE != 0 && c.d100() > resist(c, d, DISEASE_RES) {
        c.status.disease = c.status.disease.max(lvl);
        said.push(format!("{name} is diseased!"));
    }
    if mon.specials & PARALYZE != 0 && c.d100() > (resist(c, d, PARALYSIS_RES) + i32::from(c.stats[CON])).min(95) {
        c.status.paralysis = c.status.paralysis.max(3 + lvl / 2);
        said.push(format!("{name} is paralysed!"));
    }
    for s in said {
        g.say(s);
    }
}

/// Stoning: past the stone resistance, CON x ln CON / ln 2.5 - level (95 at most) saves.
fn stone(g: &mut Game, d: &GameData, m: usize, j: usize) {
    let c = &mut g.party[j];
    let v = f64::from(c.stats[CON].max(1));
    let save = (v * v.ln() / 2.5f64.ln() - f64::from(d.monsters[m].level)).min(95.0);
    if c.d100() > resist(c, d, STONE_RES) && f64::from(c.d100()) > save {
        c.status.stoned = true;
        let name = c.name.clone();
        g.say(format!("{name} is turned to stone!"));
    }
}

/// Item destruction and theft need no hit (MORDOR.EXE). DEX protects items; the
/// victim's thieving and wounds protect its purse.
fn thieves(g: &mut Game, d: &GameData, m: usize, j: usize, dmg: i32) {
    let mon = &d.monsters[m];
    let name = g.party[j].name.clone();
    let floor = g.pos().floor as f64;
    let c = &mut g.party[j];
    if mon.specials & DESTROY != 0 && !c.items.is_empty() {
        let protect = (45 - i32::from(mon.level) + 2 * i32::from(c.stats[DEX]) - 2 * i32::from(mon.stats[5])).clamp(25, 90);
        let s = c.roll(c.items.len() as u32) as usize;
        let it = &d.items[c.items[s].item];
        if c.d100() > protect && it.level - 2 <= mon.level && !(500..512).contains(&it.w(10)) {
            let h = c.items.remove(s);
            g.say(format!("The {} destroys {name}'s {}!", mon.name, d.items[h.item].name));
        }
    }
    let c = &mut g.party[j];
    if mon.specials & STEAL == 0 {
        return;
    }
    let (r, l) = c.best_rate(d, THIEVING);
    let mut x = f64::from(c.roll(100)) + (c.rnd() * (l * r) + 0.75 * l).floor();
    if g.fight.as_ref().is_some_and(|f| f.defending.get(j).copied().unwrap_or(false)) {
        x *= 1.5;
    }
    let c = &mut g.party[j];
    x -= 100.0 * f64::from(dmg) / f64::from(c.max_hp.max(1));
    let x = x - 2.0 * f64::from(mon.power_level);
    if x >= 50.0 {
        return;
    }
    let took = ((c.rnd() * ((floor + f64::from(mon.attack)) * 3353.452 * f64::from(mon.power_level) / f64::from(c.level().max(1)))) as u64).min(c.gold);
    c.gold -= took;
    let mut msg = format!("The {} steals {took} gold from {name}", mon.name);
    let loose: Vec<usize> = (0..c.items.len()).filter(|&s| !c.items[s].equipped).collect();
    let item = x <= 10.0 && !loose.is_empty();
    if item {
        let s = loose[c.roll(loose.len() as u32) as usize];
        let h = c.items.remove(s);
        msg += &format!(" and {}", d.items[h.item].name);
    }
    let runs = item || took > 0 && c.d100() > 30;
    g.say(msg + if runs { " and runs!" } else { "!" });
    if runs && let Some(f) = g.fight.as_mut() {
        f.foes.iter_mut().filter(|f| f.monster == m && f.hp > 0).take(1).for_each(|f| f.hp = 0);
        f.gold += took;
    }
}

/// A monster spell instead of an attack, as MORDOR.EXE's casters: one of its classes,
/// at a level near the monster's, damage by caster level (power level / 4) less the
/// matching resistance (and WIS, the remake's passive). False if it doesn't cast.
fn monster_spell(g: &mut Game, d: &GameData, m: usize, j: usize) -> bool {
    let mon = &d.monsters[m];
    let sq = g.floor(d).map_or(0, |f| f.square(g.pos().x, g.pos().y).flags);
    if sq & (square::ANTI_MAGIC | square::EXTINGUISH) != 0 {
        return false;
    }
    let c = &mut g.party[j];
    let classes: Vec<i16> = (0..16).filter(|b| mon.spells >> b & 1 != 0).collect();
    if classes.is_empty() || c.chance(0.1) || !d.monster_types.get(mon.kind as usize).is_some_and(|t| t.contains("Mage")) && c.chance(0.5) {
        return false;
    }
    let class = classes[c.roll(classes.len() as u32) as usize];
    let lv = i32::from(mon.level);
    let lo = (f64::from(lv) - f64::from(lv) / 3.0).round_ties_even().max(1.0) as i32;
    let fits: Vec<usize> = (0..d.spells.len())
        .filter(|&s| d.spells[s].class == class && (lo..=lv + 1).contains(&i32::from(d.spells[s].level)) && d.spells[s].base > 0 && d.spells[s].per_level > 0)
        .collect();
    let Some(&s) = fits.get(c.roll(fits.len().max(1) as u32) as usize) else {
        return false;
    };
    let sp = &d.spells[s];
    let l = (f64::from(mon.power_level) / 4.0).floor().clamp(1.0, 255.0);
    let base = f64::from(sp.base) * (1.0 + (l + 1.0).ln() / 5f64.ln()) + (l / 10.0 * f64::from(sp.per_level)).floor();
    let raw = (base - ((c.rnd() * (base - (l - 1.0) / (l * l)) / 2.0).floor() + 1.0)).floor();
    g.say(format!("The {} casts {}!", mon.name, sp.name));
    let n = usize::try_from(sp.targets.max(1)).unwrap_or(1);
    let hit: Vec<usize> = std::iter::once(j).chain(g.living().into_iter().filter(|&k| k != j)).take(n).collect();
    for k in hit {
        let c = &g.party[k];
        let slot = usize::try_from(sp.resist).unwrap_or(99);
        let res = if slot < 12 { resist(c, d, slot) } else { 0 } + 2 * i32::from((c.stats[WIS] - 15).max(0));
        let dmg = (raw * (1.0 - f64::from(res.min(100)) / 100.0)).round_ties_even() as i32;
        if dmg > 0 {
            let name = c.name.clone();
            g.say(format!("{name} takes {dmg}."));
            hurt(g, k, dmg);
        }
    }
    true
}

/// Mage's passive (docs/balance.md): binds last longer and hold harder, with CHA.
fn bind_strength(c: &Character) -> f32 {
    let mage = if c.levels[8] > 0 { 1.5 } else { 1.0 };
    mage * (1.0 + 0.04 * f32::from((c.stats[CHA] - 10).max(0)))
}

/// Steps a binding level holds (a stand-in: the original's leaving rule is not read).
fn bind_steps(c: &Character, bind: u8) -> u32 {
    (30.0 * f32::from(bind) * bind_strength(c)) as u32
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
    if d.monsters[m].flags & 8 != 0 {
        return g.say(format!("The {} can't be charmed.", d.monsters[m].name));
    }
    let strength = bind_strength(&g.party[i]);
    let p_ok = (0.35 + 0.05 * (p - f32::from(d.monsters[m].level))) * strength;
    if !g.party[i].chance(f64::from(p_ok.clamp(0.05, 0.9))) {
        return g.say(format!("The {} resists.", d.monsters[m].name));
    }
    g.fight.as_mut().unwrap().foes[fi].hp = 0;
    let bind = (p as u8).clamp(1, 10);
    let bound = bind_steps(&g.party[i], bind);
    g.party[0].companions.push(Companion { monster: m, hp, max_hp, bound, bind });
    g.say(format!("The {} joins you as a companion!", d.monsters[m].name));
}

impl Game {
    /// Each companion swings once at the weakest foe (MORDOR.EXE): its own attack
    /// against the foe's defense, critical +4 and backstab +1; its master earns a
    /// quarter of the XP.
    pub(crate) fn companions_act(&mut self, d: &GameData, t: &Tuning) {
        for k in 0..self.party[0].companions.len() {
            let Some(f) = self.fight.as_ref() else { return };
            if f.peaceful || k >= self.party[0].companions.len() {
                return;
            }
            let Some(fi) = f.pick(crate::ai::Target::Weakest) else {
                return;
            };
            let c = &d.monsters[self.party[0].companions[k].monster];
            let foe = &d.monsters[f.foes[fi].monster];
            let mut mult = f64::from(c.single(46)).max(1.0) + knack_bonus(self, c, foe.power_level.max(0) as u32, 4.0);
            if foe.flags & 2 != 0 && c.flags & SEES_INVISIBLE == 0 {
                mult /= 2.0;
            }
            let dmg = swing(&mut self.party[0], i32::from(c.attack), i32::from(foe.defense), f64::from(c.power_level), i32::from(c.stats[0]), mult);
            if dmg > 0 {
                let verb = format!("'s {} hits", c.name);
                self.damage_foe(d, t, 0, fi, dmg, &verb, 0.25);
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
            if c.bound == 0 || (unhappy && lead.chance(0.02)) {
                gone.push(lead.companions.remove(k).monster);
            }
        }
        for m in gone {
            self.say(format!("Your {} wanders off.", d.monsters[m].name));
        }
    }
}

/// A companion's base value (MORDOR.EXE): its attack, defense and hits, its specials
/// and spells, and its resistances.
pub fn companion_value(d: &GameData, m: usize) -> f64 {
    let mon = &d.monsters[m];
    if mon.hp <= 1 {
        return 1.0;
    }
    let l = f64::from(mon.power_level);
    let mut v =
        (500.0 * f64::from(mon.attack) + 500.0 * f64::from(mon.defense) + 250.0 * f64::from(mon.hp)) * (f64::from(mon.stats[0] + mon.stats[5]).max(2.0)).ln() * (l + 1.0).ln();
    for (bits, c) in [(FIRE_BREATH | COLD_BREATH, 3250.0), (ACID | ELECTROCUTE, 2050.0), (STONE | CRITICAL, 1500.0), (BACKSTAB, 500.0)] {
        v += f64::from((mon.specials & bits).count_ones()) * (c * l + 1.0);
    }
    for (b, c) in [5250.0, 3250.0, 4250.0, 3250.0, 5250.0, 5250.0, 4250.0].into_iter().enumerate() {
        if mon.spells >> b & 1 != 0 {
            v += c * l + 1.0;
        }
    }
    v + mon.resist[..11].iter().filter(|&&r| r > 0).map(|&r| 500.0 * f64::from(r)).sum::<f64>()
}

/// Holding & Confinement's prices with `s` in stock (MORDOR.EXE): buy 1.5 x value /
/// (ln s + 1.15), sell 0.9 x value / (s + 1.15).
pub fn companion_buy_price(d: &GameData, m: usize, s: u8) -> u64 {
    (1.5 * companion_value(d, m).max(10.0) / (f64::from(s.clamp(1, 9)).ln() + 1.15)).round_ties_even() as u64
}

impl Game {
    pub fn buy_companion(&mut self, d: &GameData, m: usize) -> Result<String, String> {
        let s = self.store.companions.get(m).copied().unwrap_or(0);
        if s == 0 {
            return Err("None left.".into());
        }
        let price = crate::store::cha_buy(&self.party[0], companion_buy_price(d, m, s));
        let lead = &mut self.party[0];
        if lead.companions.len() >= MAX_COMPANIONS {
            return Err("You can't hold more companions.".into());
        }
        if lead.gold + lead.bank < price {
            return Err("You can't afford that.".into());
        }
        lead.pay(price);
        let hp = i32::from(d.monsters[m].hp.max(1));
        let bind = (5 + d.monsters[m].level).clamp(1, 10) as u8;
        let bound = bind_steps(lead, bind);
        lead.companions.push(Companion { monster: m, hp, max_hp: hp, bound, bind });
        if d.monsters[m].level > 0 {
            self.store.companions[m] -= 1;
        }
        Ok(format!("The {} is yours for {price} gold, bound at level {bind}.", d.monsters[m].name))
    }

    pub fn sell_companion(&mut self, d: &GameData, k: usize) -> String {
        let lead = &mut self.party[0];
        let c = lead.companions.remove(k);
        let s = self.store.companions.get(c.monster).copied().unwrap_or(0);
        let price = crate::store::cha_sell(lead, (0.9 * companion_value(d, c.monster) / (f64::from(s.min(9)) + 1.15)).round_ties_even() as u64);
        lead.gold += price;
        if let Some(n) = self.store.companions.get_mut(c.monster)
            && *n < 9
        {
            *n += 1;
        }
        format!("Sold the {} for {price} gold.", d.monsters[c.monster].name)
    }

    /// The bindsmen raise the binding level by 4 (12 at most) for
    /// power level x 1237 x new level x old level / 4, and renew the hold.
    pub fn rebind(&mut self, d: &GameData, k: usize) -> Result<String, String> {
        let lead = &mut self.party[0];
        let b = lead.companions[k].bind.max(1);
        if b >= 12 {
            return Err("It is bound as tightly as it can be.".into());
        }
        let new = (b + 4).min(12);
        let price = (f64::from(d.monsters[lead.companions[k].monster].power_level) * 1237.0 * f64::from(new) * f64::from(b) / 4.0).round_ties_even() as u64;
        if lead.gold + lead.bank < price {
            return Err(format!("Binding costs {price} gold."));
        }
        lead.pay(price);
        let bound = bind_steps(lead, new);
        let c = &mut lead.companions[k];
        (c.bind, c.bound, c.hp) = (new, c.bound.max(bound), c.max_hp);
        Ok(format!("The bindsmen bind your {} to level {new} for {price} gold.", d.monsters[c.monster].name))
    }
}
