//! Encounters and combat rounds: the party acts (the player's orders for the controlled
//! character, macros for the rest), then the monsters. Start values are in
//! `patches/modern.toml`; see docs/balance.md.

use crate::{
    Tuning,
    ai::{self, Act, Target},
    game::{CRITICAL, DEX, Game, Held, Pos, STR},
    mdr::GameData,
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
    /// Where the party came from, for running away.
    pub back: Pos,
    pub lair: bool,
    /// Peaceful monsters leave you be until provoked (attacked, or a chest opened).
    pub peaceful: bool,
    pub chest: Option<Chest>,
    /// Who chose Defend this round.
    pub defending: Vec<bool>,
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

pub(crate) fn hit_chance(c: &crate::Combat, att: i32, def: i32) -> f64 {
    // A ratio scales from floor 1 (A/D near 20) to floor 15 (several hundred).
    let a = f64::from(att.max(1)) * c.hit_bias;
    (a / (a + f64::from(def.max(1)))).clamp(0.05, 0.95)
}

impl Game {
    /// Rooms hold their lair monster, or a group of a type the room allows.
    pub(crate) fn spawn(&mut self, data: &GameData, area: i16, stud: bool, back: Pos) {
        let floor = self.pos().floor;
        // A few squares name a room past the floor's room list; nothing lives there.
        let Some(a) = data.floors[floor - 1].areas.get(area as usize) else { return };
        let level = floor as i16 + i16::from(stud);
        let lair = a.lair != 0;
        let pick = if lair {
            data.monsters.iter().position(|m| m.id == a.lair)
        } else {
            if !self.party[0].chance(0.6) {
                self.cleared.push((floor, area));
                return;
            }
            let fits: Vec<usize> = (0..data.monsters.len())
                .filter(|&i| a.monster_types >> data.monsters[i].kind & 1 != 0 && data.monsters[i].level == level.min(16))
                .collect();
            (!fits.is_empty()).then(|| fits[self.party[0].roll(fits.len() as u32) as usize])
        };
        let Some(m) = pick else { return };
        let mon = &data.monsters[m];
        // Bigger parties meet bigger groups.
        let extra = self.party.len() as u32 / 2;
        let n = if lair { 1 } else { 1 + self.party[0].roll(mon.group.max(1) as u32 + extra) };
        let foes = (0..n)
            .map(|_| {
                let hp = (i32::from(mon.hp) * (75 + self.party[0].roll(51) as i32) / 100).max(1);
                Foe { monster: m, hp, max_hp: hp, asleep: 0 }
            })
            .collect();
        let peaceful = crate::monsters::peaceful(self, data, m);
        let chest = crate::treasure::chest_for(self, data, m, lair);
        self.say(format!(
            "You encounter {n} {}{}{}{}!",
            mon.name,
            if n > 1 { "s" } else { "" },
            if peaceful { " (peaceful)" } else { "" },
            if chest.is_some() { " guarding a chest" } else { "" }
        ));
        self.fight = Some(Fight { foes, round: 0, order: None, back, lair, peaceful, chest, defending: vec![false; self.party.len()] });
    }

    /// One combat round: each able party member acts, then the monsters.
    pub fn round(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.fight.as_mut() else { return };
        f.round += 1;
        f.defending = vec![false; self.party.len()];
        for i in 0..self.party.len() {
            if self.fight.as_ref().is_none_or(Fight::over) {
                break;
            }
            if !self.party[i].able() {
                continue;
            }
            let order = if i == 0 { self.fight.as_ref().unwrap().order } else { None };
            let act = order.unwrap_or_else(|| ai::decide(self, data, t, i));
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
            Act::Defend => {
                if let Some(f) = self.fight.as_mut() {
                    f.defending[i] = true;
                }
            }
            Act::Cast(spell, target) => crate::magic::cast(self, data, t, i, spell, target),
            Act::Use(item) => crate::magic::use_item(self, data, t, i, item),
            Act::Open => crate::treasure::open(self, data, t, i),
            Act::Run if i == 0 => {
                if self.party[0].chance(0.6) {
                    let back = self.fight.take().unwrap().back;
                    self.set_pos(data, back);
                    self.say("You flee back the way you came.");
                } else {
                    self.say("You fail to get away!");
                }
            }
            Act::CastBest(..) | Act::Quaff => {
                if let Some(a) = ai::resolve(self, data, t, i, act) {
                    self.act(data, t, i, a);
                }
            }
            Act::Run | Act::Wait => {}
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

    fn melee(&mut self, data: &GameData, t: &Tuning, i: usize, target: Target) {
        self.provoke();
        let c = &t.combat;
        let Some(fi) = self.fight.as_ref().and_then(|f| f.pick(target)) else { return };
        let m = &data.monsters[self.fight.as_ref().unwrap().foes[fi].monster];
        let (att, _) = self.party[i].att_def(data, t);
        // Multiple swings: a second swing at swings-rating / 30 (Ninja ~ 1 in 4 at level 10).
        let swing_odds = f64::from(self.party[i].ability(data, 3)) / 30.0;
        let swings = 1 + usize::from(self.party[i].chance(swing_odds));
        for _ in 0..swings {
            if self.fight.as_ref().is_none_or(|f| f.foes[fi].hp <= 0) {
                return;
            }
            let ch = &mut self.party[i];
            if !ch.chance(hit_chance(c, att, i32::from(m.defense))) {
                let name = ch.name.clone();
                self.say(format!("{name} misses the {}.", m.name));
                continue;
            }
            let (power, mult) = ch.weapon(data).map_or((2, 1.0), |w| (data.items[w].power, data.items[w].damage_mult));
            let rating = (2 * i32::from(ch.stats[STR]) + i32::from(ch.stats[DEX])) / 3;
            let mut dmg = ((1 + ch.roll(power.max(1) as u32 * c.weapon_die) as i32 + (rating - 10) / 2).max(1) as f32 * mult) as i32;
            // Critical hits: 2% per point of the rating; DEX above 15 adds 3% damage per point.
            let crit = ch.chance(f64::from(ch.ability(data, CRITICAL)) * 0.02);
            if crit {
                dmg = (f64::from(dmg) * (2.0 + 0.03 * f64::from(ch.stats[DEX] - 15).max(0.0))) as i32;
            }
            let name = ch.name.clone();
            self.damage_foe(data, t, i, fi, dmg, if crit { "critically hits" } else { "hits" });
            let _ = name;
        }
    }

    /// Applies damage to a foe and handles the kill.
    pub(crate) fn damage_foe(&mut self, data: &GameData, t: &Tuning, i: usize, fi: usize, dmg: i32, verb: &str) -> bool {
        let Some(f) = self.fight.as_mut() else { return false };
        let foe = &mut f.foes[fi];
        let m = &data.monsters[foe.monster];
        foe.hp -= dmg;
        let killed = foe.hp <= 0;
        let name = self.party[i].name.clone();
        self.say(format!("{name} {verb} the {} for {dmg}{}", m.name, if killed { " and kills it!" } else { "." }));
        if killed {
            self.kill(data, t, i, foe_monster(self, fi));
        }
        killed
    }

    /// Experience and gold for a kill, shared by the living: XP split evenly, members
    /// behind the party's best level get the catch-up bonus.
    pub(crate) fn kill(&mut self, data: &GameData, t: &Tuning, killer: usize, monster: usize) {
        let c = &t.combat;
        let m = &data.monsters[monster];
        self.party[killer].kills += 1;
        let living = self.living();
        let floor = self.pos().floor as u32;
        let base = (f64::from(m.hp) * f64::from(m.level.max(1)) * c.xp_per_hp_level + f64::from(m.attack) / 2.0) as u64 / living.len() as u64;
        let best = living.iter().map(|&j| self.party[j].level()).max().unwrap_or(1);
        let gold = u64::from(m.level.max(1) as u32 * (c.gold_min + self.party[killer].roll(c.gold_max - c.gold_min + 1)));
        let mut gained = Vec::new();
        for &j in &living {
            let xp = t.xp_award(base, floor, best - self.party[j].level());
            let xp = self.party[j].add_xp(data, t, xp);
            self.party[j].gold += gold / living.len() as u64;
            gained.push(xp);
            if self.party[j].quest == Some(monster) {
                self.party[j].quest = None;
                let name = self.party[j].name.clone();
                self.say(format!("{name} has completed the guild's quest!"));
            }
        }
        let xp = gained.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("/");
        self.say(format!("The party gains {xp} experience and {gold} gold."));
    }

    fn win(&mut self, data: &GameData, t: &Tuning) {
        let lair = self.fight.as_ref().is_some_and(|f| f.lair);
        let chest = self.fight.as_mut().and_then(|f| f.chest.take());
        self.fight = None;
        let Pos { x, y, floor, .. } = self.pos();
        let area = data.floors[floor - 1].square(x, y).area;
        self.cleared.push((floor, area));
        self.say("The room is clear.");
        match chest {
            // Items mostly come in chests (the original); the party opens it now.
            Some(chest) => crate::treasure::leave_chest(self, chest),
            None if self.party[0].chance(t.combat.item_chance * if lair { 3.0 } else { 0.3 }) => {
                let item = crate::treasure::random_item(self, data, floor);
                self.give(data, item);
            }
            None => {}
        }
    }

    /// Gives an item to the first living member with room.
    pub(crate) fn give(&mut self, data: &GameData, item: Option<usize>) {
        let Some(item) = item else { return };
        let Some(j) = self.living().into_iter().find(|&j| self.party[j].items.len() < 40) else {
            return self.say("Nobody has room to carry it.");
        };
        // A sharp mind may know what it found (INT + WIS); otherwise the store can tell.
        let wits = f64::from(self.party[j].stats[crate::game::INT] + self.party[j].stats[crate::game::WIS]) / 50.0;
        let known = self.party[j].chance(wits);
        let held: Held = crate::magic::new_held(data, item, known);
        let name = self.party[j].name.clone();
        self.say(format!("{name} finds {}!", crate::magic::item_name(data, &held)));
        self.party[j].items.push(held);
    }
}

fn foe_monster(g: &Game, fi: usize) -> usize {
    g.fight.as_ref().map_or(0, |f| f.foes[fi].monster)
}
