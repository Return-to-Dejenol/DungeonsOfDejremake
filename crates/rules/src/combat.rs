//! Encounters and combat rounds as MORDOR.EXE runs them (docs/balance.md, "Original
//! rules"): the party acts (the player's orders for the controlled character, macros for
//! the rest), then the companions, then the monsters. XP comes from damage dealt.

use crate::{
    Tuning,
    ai::{self, Act, Target},
    game::{BACKSTAB, CRITICAL, Character, DEX, Game, Held, Pos},
    mdr::{GameData, Words},
    treasure::Chest,
};

pub struct Foe {
    pub monster: usize,
    pub hp: i32,
    pub max_hp: i32,
    /// Rounds left asleep or dazzled; it doesn't act.
    pub asleep: u16,
}

pub struct Fight {
    pub foes: Vec<Foe>,
    pub round: u32,
    /// The player's order for the controlled character; `None` lets its macros decide.
    pub order: Option<Act>,
    /// Where the party came from, for leaving.
    pub back: Pos,
    pub lair: bool,
    /// Peaceful monsters leave you be until provoked (attacked, or a chest opened).
    pub peaceful: bool,
    pub chest: Option<Chest>,
    /// Who chose Defend this round.
    pub defending: Vec<bool>,
    /// The group's gold, paid when the fight is won or its container opened.
    pub gold: u64,
    /// A member opening the chest mid-fight, and the rounds it still takes.
    pub opening: Option<(usize, u32)>,
}

impl Fight {
    pub fn alive(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.foes.len()).filter(|&i| self.foes[i].hp > 0)
    }

    pub fn over(&self) -> bool {
        self.alive().next().is_none()
    }

    /// The foe a target rule picks.
    pub fn pick(&self, t: Target) -> Option<usize> {
        let alive = self.alive();
        match t {
            Target::Strongest => alive.max_by_key(|&i| self.foes[i].hp),
            Target::Weakest | Target::LowestAlly | Target::Myself => alive.min_by_key(|&i| self.foes[i].hp),
            Target::First => alive.into_iter().next(),
        }
    }
}

/// One swing, MORDOR.EXE's hit-and-damage routine. The hit chance is 50% moved by the
/// attack/defense gap relative to the larger of the two (2-98%); damage grows with the
/// attacker's level and strength, times the multiplier.
pub(crate) fn swing(c: &mut Character, att: i32, def: i32, level: f64, strength: i32, mult: f64) -> i32 {
    let (a, d) = (f64::from(att.max(0)), f64::from(def.max(0)));
    let h = match () {
        _ if a == 0.0 && d == 0.0 => 0.5,
        _ if a > d => 0.5 + 0.5 * (a - d) / a,
        _ => 0.5 - 0.5 * (d - a) / d,
    };
    let h = (h * 100.0).clamp(2.0, 98.0);
    let s = f64::from(strength.max(1));
    let s = (s - s.ln() * (s.ln() - 2.4) * 2.25 + 1.0).round_ties_even();
    let mut m = if mult <= 0.0 { 0.1 } else { mult };
    if m > 1.0 {
        m -= m.ln().powi(2);
    }
    let lvl = ((level + 5.0).ln() + 1.0) / 1.2f64.ln();
    let roll = (c.rnd() * (s + 10.0) / 2.0 + (s + 10.0) / 4.0).floor() / 10.0;
    let mut dmg = (lvl * s / 30.0 * roll * m).floor() as i32;
    // The defense gap takes damage off (or adds it when the attacker is stronger).
    let gap = ((d - a) / 8.0).floor();
    let gap = gap - (c.rnd() * (gap + 1.0)).floor() + 1.0;
    if c.roll(1000) > 0 {
        dmg -= gap as i32;
    }
    if dmg < 1 || f64::from(c.d100()) >= h { 0 } else { dmg }
}

/// A critical-hit or backstab chance in percent (MORDOR.EXE): the guild's level and
/// rate against the monster's level.
fn knack(rate: f64, level: f64, div: f64, monster_level: f64) -> f64 {
    ((level / 999.0 + 1.0).ln() * 100.0 * (rate + 1.0).ln() / div - (monster_level / 999.0 + 1.0).ln() * 100.0) / 2.0
}

/// The size gap's effect on a multiplier: +0.1 a step for the bigger side.
pub(crate) fn size_term(data: &GameData, c: &Character, monster: usize) -> f64 {
    f64::from(data.races[c.race].w(28) - data.monsters[monster].w(64)) / 10.0
}

impl Game {
    /// Rooms hold their lair monster, or a group of a type the room allows.
    pub(crate) fn spawn(&mut self, data: &GameData, area: i16, stud: bool, back: Pos) {
        let floor = self.pos().floor;
        // A few squares name a room past the floor's room list; nothing lives there.
        let Some(a) = data.floors[floor - 1].areas.get(area as usize) else {
            return;
        };
        let level = floor as i16 + i16::from(stud);
        let lair = a.lair != 0;
        let pick = if lair {
            data.monsters.iter().position(|m| m.id == a.lair)
        } else {
            if !self.party[0].chance(0.6) {
                self.cleared.push((floor, area));
                return;
            }
            let fits: Vec<usize> = (0..data.monsters.len()).filter(|&i| a.monster_types >> data.monsters[i].kind & 1 != 0 && data.monsters[i].level == level.min(16)).collect();
            (!fits.is_empty()).then(|| fits[self.party[0].roll(fits.len() as u32) as usize])
        };
        let Some(m) = pick else { return };
        let mon = &data.monsters[m];
        // Bigger parties meet bigger groups.
        let extra = self.party.len() as u32 / 2;
        let n = if lair { 1 } else { 1 + self.party[0].roll(mon.group.max(1) as u32 + extra) };
        let foes = (0..n).map(|_| Foe { monster: m, hp: new_hp(&mut self.party[0], i32::from(mon.hp)), max_hp: 0, asleep: 0 }).map(|f| Foe { max_hp: f.hp, ..f }).collect();
        let peaceful = crate::monsters::peaceful(self, data, m);
        let chest = crate::treasure::chest_for(self, data, m, lair);
        let gold = crate::treasure::gold(&mut self.party[0], mon, n, chest.as_ref().is_some_and(|c| c.magic_lock));
        self.say(format!(
            "You encounter {n} {}{}{}{}!",
            mon.name,
            if n > 1 { "s" } else { "" },
            if peaceful { " (peaceful)" } else { "" },
            if chest.is_some() { " guarding a chest" } else { "" }
        ));
        self.fight = Some(Fight { foes, round: 0, order: None, back, lair, peaceful, chest, defending: vec![false; self.party.len()], gold, opening: None });
    }

    /// One combat round: each able party member acts, then the companions, then the
    /// monsters.
    pub fn round(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.fight.as_mut() else { return };
        f.round += 1;
        f.defending = vec![false; self.party.len()];
        crate::monsters::turn_hostile(self, data);
        for i in 0..self.party.len() {
            if self.fight.as_ref().is_none_or(Fight::over) {
                break;
            }
            if !self.party[i].able() {
                continue;
            }
            let order = if i == 0 { self.fight.as_ref().unwrap().order } else { None };
            let act = order.unwrap_or_else(|| ai::decide(self, data, t, i));
            // Anything but opening cancels an opening under way.
            if act != Act::Open
                && let Some(f) = self.fight.as_mut()
                && f.opening.is_some_and(|(j, _)| j == i)
            {
                f.opening = None;
            }
            self.act(data, t, i, act);
        }
        self.companions_act(data, t);
        let Some(f) = self.fight.as_ref() else { return };
        if f.over() {
            return self.win(data, t);
        }
        if !f.peaceful {
            crate::monsters::attack(self, data, t);
        }
        if let Some(f) = self.fight.as_mut() {
            for foe in &mut f.foes {
                foe.asleep = foe.asleep.saturating_sub(1);
            }
        }
        // Paralysis wears off round by round as well as step by step.
        for m in &mut self.party {
            m.status.paralysis = m.status.paralysis.saturating_sub(1);
        }
    }

    /// Carries out one member's action.
    pub(crate) fn act(&mut self, data: &GameData, t: &Tuning, i: usize, act: Act) {
        match act {
            Act::Fight(target) => self.melee(data, t, i, target),
            Act::Defend => self.defend(i),
            Act::Cast(spell, target) => crate::magic::cast(self, data, t, i, spell, target),
            Act::Use(item) => crate::magic::use_item(self, data, t, i, item),
            Act::Open => crate::treasure::open(self, data, t, i),
            Act::Run if i == 0 => self.leave(data, t),
            Act::CastBest(..) | Act::Quaff => {
                if let Some(a) = ai::resolve(self, data, t, i, act) {
                    self.act(data, t, i, a);
                }
            }
            Act::Run | Act::Wait => {}
        }
    }

    fn defend(&mut self, i: usize) {
        if let Some(f) = self.fight.as_mut() {
            f.defending[i] = true;
        }
    }

    /// Walking away: the monsters get a free attack, 50% + 5% per point their DEX beats
    /// the leader's (5% at least), as the original's leaving combat.
    fn leave(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.fight.as_ref() else { return };
        let m = f.alive().next().map(|fi| f.foes[fi].monster);
        let (peaceful, back) = (f.peaceful, f.back);
        if let Some(m) = m
            && !peaceful
        {
            let p = (50 + 5 * (i32::from(data.monsters[m].stats[5]) - i32::from(self.party[0].stats[DEX]))).max(5);
            if self.party[0].d100() <= p {
                self.say("The monsters strike as you turn to go!");
                crate::monsters::attack(self, data, t);
            }
        }
        if self.fight.take().is_some() {
            self.set_pos(data, back);
            self.say("You leave the way you came.");
        }
    }

    /// Provokes peaceful monsters.
    pub(crate) fn provoke(&mut self) {
        if let Some(f) = self.fight.as_mut()
            && f.peaceful
        {
            f.peaceful = false;
            self.say("The monsters turn hostile!");
        }
    }

    /// The original's melee: swings + the weapon's swings (w9) at one foe. A miss, or a
    /// hit that doesn't kill, ends the turn; a kill moves on to the next foe.
    fn melee(&mut self, data: &GameData, t: &Tuning, i: usize, target: Target) {
        self.provoke();
        let Some(mut fi) = self.fight.as_ref().and_then(|f| f.pick(target)) else {
            return;
        };
        let name = self.party[i].name.clone();
        let Some(w) = self.party[i].weapon(data) else {
            self.say(format!("{name} has no weapon, and defends."));
            return self.defend(i);
        };
        let weapon = &data.items[w];
        let (att, _) = self.party[i].att_def(data, t);
        let (fighting, flevel) = self.party[i].fighting(data);
        let swings = (self.party[i].swings(data) + i32::from(weapon.power)).max(1);
        for _ in 0..swings {
            let m = self.fight.as_ref().unwrap().foes[fi].monster;
            let mon = &data.monsters[m];
            if mon.flags & 32 != 0 || mon.flags & 16 != 0 && weapon.level < mon.level / 2 {
                self.say(format!("{name}'s weapon has no effect on the {}!", mon.name));
                return self.defend(i);
            }
            let c = &mut self.party[i];
            let mut mult = fighting * f64::from(weapon.damage_mult);
            let flags = weapon.long(7);
            let wl = (f64::from(weapon.level) + 1.0).ln();
            let mlevel = f64::from(mon.power_level);
            let (rc, lc) = c.best_rate(data, CRITICAL);
            let mut crit = knack(rc, lc, 3.0, mlevel);
            if flags & 16 != 0 {
                crit = (crit + wl * 10f64.ln()).max(wl * 10f64.ln());
            }
            let crit = if crit < 1.0 && (lc > 0.0 || flags & 16 != 0) { 1.0 } else { crit.min(50.0) };
            let (rb, lb) = c.best_rate(data, BACKSTAB);
            let mut stab = knack(rb, lb, 2.0, mlevel);
            if flags & 32 != 0 {
                stab = (stab + wl * 100f64.ln()).max(wl * 100f64.ln());
            }
            let stab = if stab < 2.0 && (lb > 0.0 || flags & 32 != 0) { 2.0 } else { stab.min(75.0) };
            let (critical, backstab) = (f64::from(c.roll(100)) < crit, f64::from(c.roll(100)) < stab);
            if critical {
                // DEX passive (docs/balance.md): +3% critical damage a point above 15.
                mult += 5.0 * (1.0 + 0.03 * f64::from((c.stats[DEX] - 15).max(0)));
            }
            if backstab {
                mult += 2.0;
            }
            if mon.flags & 2 != 0 && !c.buffs.see_invisible {
                mult -= mult / 2.0;
            }
            mult += size_term(data, c, m);
            let dmg = swing(c, att, i32::from(mon.defense), flevel, c.melee_rating(), mult);
            if dmg == 0 {
                self.say(format!("{name} misses the {}.", mon.name));
                return;
            }
            let verb = match (critical, backstab) {
                (true, true) => "severs",
                (false, true) => "backstabs",
                (true, false) => "critically hits",
                _ => "hits",
            };
            if !self.damage_foe(data, t, i, fi, dmg, verb, 1.0) {
                return;
            }
            match self.fight.as_ref().and_then(|f| f.pick(Target::First)) {
                Some(next) => fi = next,
                None => return,
            }
        }
    }

    /// Applies damage to a foe, awards the dealer's XP and handles the kill. `share` is
    /// the XP rate (companions earn a quarter for their master).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn damage_foe(&mut self, data: &GameData, t: &Tuning, i: usize, fi: usize, dmg: i32, verb: &str, share: f64) -> bool {
        let Some(f) = self.fight.as_mut() else {
            return false;
        };
        let foe = &mut f.foes[fi];
        let (m, dealt) = (foe.monster, dmg.min(foe.hp));
        foe.hp -= dmg;
        let killed = foe.hp <= 0;
        let xp = self.award(data, t, i, m, dealt, share);
        let name = self.party[i].name.clone();
        let mon = &data.monsters[m].name;
        self.say(format!("{name} {verb} the {mon} for {dmg}{} (+{xp} XP)", if killed { " and kills it!" } else { "." }));
        if killed {
            self.kill(data, i, m);
        }
        killed
    }

    /// XP for damage dealt (MORDOR.EXE): d (0.9 - ln(d+1)/10) (5 + level/10)
    /// (2 - ln d/10) / (1 + 3 L/999), less 4% per other party member, then the remake's
    /// depth and catch-up bonuses.
    pub(crate) fn award(&mut self, data: &GameData, t: &Tuning, i: usize, m: usize, dealt: i32, share: f64) -> u64 {
        let d = f64::from(dealt.max(1));
        let lcur = f64::from(self.party[i].level());
        let base = d * (0.9 - (d + 1.0).ln() / 10.0) * (5.0 + f64::from(data.monsters[m].power_level) / 10.0) * (2.0 - d.ln() / 10.0) * share / (1.0 + 3.0 * lcur / 999.0);
        let base = (base * (1.0 - 0.04 * (self.party.len() as f64 - 1.0))).max(0.0).round_ties_even() as u64;
        let best = self.party.iter().map(Character::level).max().unwrap_or(1);
        let xp = t.xp_award(base, self.pos().floor as u32, best - self.party[i].level());
        self.party[i].add_xp(data, t, xp)
    }

    /// A kill: the kill count and the guild's quest.
    pub(crate) fn kill(&mut self, data: &GameData, killer: usize, monster: usize) {
        self.party[killer].kills += 1;
        let name = self.party[killer].name.clone();
        self.hall.check(data, crate::hall::CREATURE, &name, i64::from(data.monsters[monster].id));
        for j in self.living() {
            if self.party[j].quest == Some(monster) {
                self.party[j].quest = None;
                let name = self.party[j].name.clone();
                self.say(format!("{name} has completed the guild's quest!"));
            }
        }
    }

    /// Gold split evenly among the living (the original's divvying up).
    pub(crate) fn share_gold(&mut self, gold: u64) {
        let living = self.living();
        for &j in &living {
            self.party[j].gold += gold / living.len().max(1) as u64;
        }
        self.say(format!("The party finds {gold} gold."));
    }

    fn win(&mut self, _data: &GameData, _t: &Tuning) {
        let Some(f) = self.fight.take() else { return };
        let Pos { x, y, floor, .. } = self.pos();
        let area = _data.floors[floor - 1].square(x, y).area;
        self.cleared.push((floor, area));
        self.say("The room is clear.");
        match f.chest {
            // A container keeps the gold until it is opened.
            Some(mut chest) => {
                chest.gold = f.gold;
                crate::treasure::leave_chest(self, chest);
            }
            None => self.share_gold(f.gold),
        }
    }

    /// Gives an item to the first living member with room; what they know of it follows
    /// the original's identification roll.
    pub(crate) fn give(&mut self, data: &GameData, item: Option<usize>) {
        let Some(item) = item else { return };
        let Some(j) = self.living().into_iter().find(|&j| self.party[j].items.len() < 40) else {
            return self.say("An item was found, but nobody had room to carry it.");
        };
        let known = crate::treasure::identifies(&self.party[j], data, item);
        let held: Held = crate::magic::new_held(data, item, known);
        let name = self.party[j].name.clone();
        self.say(format!("{name} finds {}!", crate::magic::item_name(data, &held)));
        self.party[j].items.push(held);
    }
}

/// A new monster's hits: the table's, give or take a quarter (MORDOR.EXE).
fn new_hp(c: &mut Character, hp: i32) -> i32 {
    let v = (c.rnd() * f64::from(hp) * 0.25).floor() as i32 + 1;
    (if c.chance(0.5) { hp + v } else { hp - v }).max(1)
}
