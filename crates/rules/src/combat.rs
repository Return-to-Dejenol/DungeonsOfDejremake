//! Encounters and combat rounds as MORDOR.EXE runs them (docs/balance.md, "Original
//! rules"): the party acts (the player's orders for the controlled character, macros for
//! the rest), then the companions, then the monsters. XP comes from damage dealt.

use crate::{
    Tuning,
    ai::{self, Act, Target},
    game::{BACKSTAB, CHA, CHAR_HIT, CRITICAL, Character, DEX, Game, Held, MONSTER_HIT, Pos},
    mdr::{GameData, Words},
    treasure::Chest,
};

pub struct Foe {
    pub monster: usize,
    pub hp: i32,
    pub max_hp: i32,
    /// Its group in the encounter; the original fights a group one monster at a time.
    pub group: u8,
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
    /// Per group, how many of a peaceful group offer to join (0 none).
    pub offers: Vec<u8>,
}

/// A room's monsters (fn0F88): when it was last stocked, in ailment ticks, and the
/// encounter still there, wounded, until killed.
pub struct Room {
    pub born: u64,
    pub fight: Option<Fight>,
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

    /// A group's front monster, the first still alive.
    pub fn front(&self, group: u8) -> Option<usize> {
        self.alive().find(|&i| self.foes[i].group == group)
    }

    /// The living groups, starting from `first` and wrapping around.
    pub fn groups_from(&self, first: u8) -> Vec<u8> {
        let mut v: Vec<u8> = self.alive().map(|i| self.foes[i].group).collect();
        v.sort_unstable();
        v.dedup();
        let k = v.iter().position(|&g| g == first).unwrap_or(0);
        v.rotate_left(k);
        v
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

/// Ticks before an emptied room refills (sub0A91): 1200 + 300 x floor / 2, less 5 for
/// each room of the floor without monsters, 120 at least.
fn refill(floor: usize, rooms: usize, held: usize) -> u64 {
    (1200.0 + 300.0 * floor as f64 / 2.0 - 5.0 * rooms.saturating_sub(held) as f64).max(120.0) as u64
}

impl Game {
    /// Entering a room (sub0A91): its monsters are still there, wounded, until killed;
    /// an emptied room refills after `refill` ticks, and one whose monsters are twice
    /// that old gets new ones. Hostile monsters may strike as the party comes in.
    pub(crate) fn enter_room(&mut self, data: &GameData, t: &Tuning, area: i16, stud: bool, back: Pos) {
        let floor = self.pos().floor;
        // A few squares name a room past the floor's room list; nothing lives there.
        let Some(a) = data.floors[floor - 1].areas.get(area as usize) else {
            return;
        };
        let held = self.rooms.iter().filter(|(k, r)| k.0 == floor && r.fight.is_some()).count();
        let span = refill(floor, data.floors[floor - 1].areas.len(), held);
        let room = self.rooms.remove(&(floor, area));
        let age = room.as_ref().map_or(u64::MAX, |r| self.ticks - r.born);
        match room {
            Some(Room { fight: Some(mut f), born }) if age <= 2 * span => {
                f.back = back;
                f.defending = vec![false; self.party.len()];
                self.fight = Some(f);
                self.rooms.insert((floor, area), Room { born, fight: None });
            }
            Some(r @ Room { fight: None, .. }) if age <= span => {
                self.rooms.insert((floor, area), r);
                return;
            }
            _ => {
                self.rooms.insert((floor, area), Room { born: self.ticks, fight: None });
                if a.monster_types == 0 {
                    return;
                }
                let groups = self.mongen(data, a, area, stud);
                if groups.is_empty() {
                    return;
                }
                self.muster(data, &groups, a.lair, back);
            }
        }
        // The monsters' free attack on the party's coming in (sub0A64, mode 1).
        if self.fight.as_ref().is_some_and(|f| !f.peaceful) {
            self.free_attacks(data, t, 999);
        }
    }

    /// Each hostile group strikes its target when d100 < 50 + 5 x (its DEX - the
    /// target's), 5 at least and `cap` at most (sub0A64: entering, or leaving at 98).
    fn free_attacks(&mut self, data: &GameData, t: &Tuning, cap: i32) {
        let Some(f) = self.fight.as_ref() else { return };
        for gr in f.groups_from(0) {
            let Some(m) = self.fight.as_ref().and_then(|f| f.front(gr)).map(|fi| self.fight.as_ref().unwrap().foes[fi].monster) else { continue };
            let Some(j) = crate::monsters::target(self) else { return };
            let p = (50 + 5 * (i32::from(data.monsters[m].stats[5]) - i32::from(self.party[j].stats[DEX]))).clamp(5, cap);
            if self.party[0].d100() < p {
                crate::monsters::attack(self, data, t, gr, Some(j));
            }
        }
    }

    /// MORDOR.EXE's MONGEN (fn0D4D): the room's lair monster 75% of the time; else a type
    /// the room allows (any type 1% of the time outside the floor-1 starter rooms),
    /// weighted by the spawn weights (w26) of its monsters within the window, then one of
    /// those by weight. Then up to w4 groups, each a follower of the one before (w48
    /// type, w49 how, w62/w63 kin), of half to all of its group size (w32).
    pub(crate) fn mongen(&mut self, data: &GameData, a: &crate::mdr::Area, area: i16, stud: bool) -> Vec<(usize, u32)> {
        let floor = self.pos().floor as i16;
        let top = floor + i16::from(stud);
        let starter = floor == 1 && area < 18;
        let r = &mut self.party[0];
        let ms = &data.monsters;
        let in_window = |i: usize, kind: usize, w: i16| ms[i].kind as usize == kind && (top - w..=top).contains(&ms[i].level);
        let weight = |kind: usize, w: i16| (0..ms.len()).filter(|&i| in_window(i, kind, w)).map(|i| ms[i].weight.max(0) as u32).sum::<u32>();
        let mut first = if a.lair != 0 && r.rnd() * 100.0 <= 75.0 { ms.iter().position(|m| m.id == a.lair) } else { None };
        for _ in 0..100 {
            if first.is_some() {
                break;
            }
            // The window: 1 level below half the time (the original's 2- and 3-level
            // branches can't be reached), more by floor / 5; on a stud square 1 two times
            // in three.
            let mut w = i16::from(r.d100() < 50) + (f64::from(floor) / 5.0).round_ties_even() as i16;
            if stud && r.rnd() * 75.0 < 50.0 {
                w = 1;
            }
            w = w.min(floor);
            let types = data.monster_types.len();
            let allowed: Vec<usize> = (0..types).filter(|&k| a.monster_types >> k & 1 != 0 && ms.iter().any(|m| m.kind as usize == k)).collect();
            let kind = if r.d100() < 2 && !starter || allowed.is_empty() {
                r.roll(types as u32) as usize
            } else {
                let ws: Vec<(usize, i16)> = allowed.iter().map(|&k| (k, weight(k, w).min(i16::MAX as u32) as i16)).collect();
                pick(r, &ws).unwrap_or(allowed[r.roll(allowed.len() as u32) as usize])
            };
            // An empty window widens, three times at most.
            let Some(w) = (w..=w + 3).find(|&w| weight(kind, w) > 0) else { continue };
            let fits: Vec<(usize, i16)> = (0..ms.len()).filter(|&i| in_window(i, kind, w)).map(|i| (i, ms[i].weight)).collect();
            let Some(m) = pick(r, &fits) else { continue };
            // The floor-1 starter rooms hold only the weakest (and Slaves).
            if starter && ms[m].power_level > 2 && m != 4 {
                continue;
            }
            first = Some(m);
        }
        let Some(first) = first else { return Vec::new() };
        let w4 = ms[first].w(4).max(1);
        let mut n = w4;
        if f64::from(r.roll(100)) < f64::from(n) / 8.0 * 100.0 {
            n -= (r.rnd() * (f64::from(w4) / 2.0 + 1.0)) as i16;
        }
        let mut chain = vec![first];
        let mut kin = ms[first].w(63);
        while chain.len() < n.clamp(1, 4) as usize {
            let prev = *chain.last().unwrap();
            let p = &ms[prev];
            let next = match p.w(49) {
                3 => Some(prev),
                4 | 6 => Some(4),
                5 => Some(5),
                m if m > 7 => Some((m - 7) as usize),
                mode => {
                    let (down, up) = match mode {
                        0 if p.level > 1 => (3, -1),
                        0 | 1 => (3, 0),
                        _ => (0, 0),
                    };
                    let fit = |i: usize, kin: i16| ms[i].kind == p.w(48) && (p.level - down..=p.level + up).contains(&ms[i].level) && ms[i].w(32) != 1 && i != prev && (kin <= 0 || ms[i].w(62) == kin);
                    let fits = |kin: i16| (0..ms.len()).filter(|&i| fit(i, kin)).map(|i| (i, ms[i].weight)).collect::<Vec<_>>();
                    pick(r, &fits(kin)).or_else(|| pick(r, &fits(0)))
                }
            };
            let Some(next) = next.filter(|&i| i < ms.len()) else { break };
            kin = ms[next].w(63);
            chain.push(next);
        }
        chain
            .into_iter()
            .map(|m| {
                let g = f64::from(ms[m].w(32).max(1));
                (m, ((g - r.rnd() * (g / 2.0)).round_ties_even() as u32).max(1))
            })
            .collect()
    }

    /// The encounter from MONGEN's groups: their hits, peace, container and gold, and the
    /// peaceful groups' offers to join (d3000 <= 10 CHA - 10 (level found - 1)).
    fn muster(&mut self, data: &GameData, groups: &[(usize, u32)], lair: i16, back: Pos) {
        let first = groups[0].0;
        let mut foes = Vec::new();
        for (gi, &(m, n)) in groups.iter().enumerate() {
            for _ in 0..n {
                let hp = new_hp(&mut self.party[0], i32::from(data.monsters[m].hp));
                foes.push(Foe { monster: m, hp, max_hp: hp, group: gi as u8 });
            }
        }
        let peaceful = crate::monsters::peaceful(self, data, first);
        let is_lair = lair != 0 && data.monsters[first].id == lair;
        let chest = crate::treasure::chest_for(self, data, first, is_lair);
        let gold = crate::treasure::gold(&mut self.party[0], data, groups, chest.as_ref().is_some_and(|c| c.magic_lock));
        let lead = &self.party[0];
        let (cha, align, level) = (i32::from(lead.stats[CHA]), lead.alignment, lead.level());
        let mut offers = vec![0; groups.len()];
        for (gi, &(m, n)) in groups.iter().enumerate() {
            let mon = &data.monsters[m];
            let kin = mon.w(31) == 1 || mon.w(31) == i16::from(align);
            if peaceful && kin && u32::try_from(mon.power_level).unwrap_or(0) <= level && self.party[0].roll(3000) as i32 + 1 <= (10 * cha - 10 * (i32::from(mon.level) - 1)).max(0) {
                offers[gi] = (self.party[0].roll(n.min(4)) + 1) as u8;
                // MONSMOD.monShowMonsters: "2 Dwarves offer to 'j'oin."
                let k = usize::from(offers[gi]);
                let offer = format!("{k} {}{}", crate::monsters::plural(data, m, k, true), data.t(if k == 1 { 0xc1ae_c013 } else { 0x57df_86ca }));
                self.said[MONSTER_HIT].clone_from(&offer);
                self.say(offer);
            }
        }
        self.fight = Some(Fight { foes, round: 0, order: None, back, lair: is_lair, peaceful, chest, defending: vec![false; self.party.len()], gold, opening: None, offers });
    }

    /// The encounter, as it stands, goes back to its room (fleeing, walking past peaceful
    /// monsters, or the party falling).
    pub(crate) fn shelve(&mut self, data: &GameData) {
        let Some(f) = self.fight.take() else { return };
        let Pos { x, y, floor, .. } = self.pos();
        if floor > 0 && !f.over() {
            let area = data.floors[floor - 1].square(x, y).area;
            let born = self.rooms.get(&(floor, area)).map_or(self.ticks, |r| r.born);
            self.rooms.insert((floor, area), Room { born, fight: Some(f) });
        }
    }

    /// One combat round: each able party member acts, then the companions, then the
    /// monsters.
    pub fn round(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.fight.as_mut() else { return };
        f.round += 1;
        f.defending = vec![false; self.party.len()];
        (self.said[CHAR_HIT], self.said[MONSTER_HIT]) = Default::default();
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
        // Each group attacks its own target (MULTIMOD.mdrmulMonsterAttack).
        if !f.peaceful {
            for gr in f.groups_from(0) {
                if self.fight.as_ref().is_some_and(|f| f.front(gr).is_some()) {
                    crate::monsters::attack(self, data, t, gr, None);
                }
            }
        }
        // A round is a tick of the ailment timer (docs/balance.md).
        self.ailments(data);
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

    /// Walking away: each hostile group gets a free attack (`free_attacks`, 98 at most);
    /// the monsters stay in their room.
    fn leave(&mut self, data: &GameData, t: &Tuning) {
        let Some(f) = self.fight.as_ref() else { return };
        let (peaceful, back) = (f.peaceful, f.back);
        if !peaceful {
            self.free_attacks(data, t, 98);
        }
        if self.fight.is_some() {
            self.shelve(data);
            self.ailments(data);
            self.set_pos(data, back);
            crate::ai::tend(self, data, t);
        }
    }

    /// Provokes peaceful monsters.
    pub(crate) fn provoke(&mut self) {
        if let Some(f) = self.fight.as_mut()
            && f.peaceful
        {
            f.peaceful = false;
        }
    }

    /// The original's melee: swings + the weapon's swings (w9) at one foe. A miss, or a
    /// hit that doesn't kill, ends the turn; a kill moves on to the next foe.
    fn melee(&mut self, data: &GameData, t: &Tuning, i: usize, target: Target) {
        self.provoke();
        // A group is fought at its front monster.
        let Some(mut fi) = self.fight.as_ref().and_then(|f| f.pick(target).and_then(|p| f.front(f.foes[p].group))) else {
            return;
        };
        let name = self.party[i].name.clone();
        let Some(w) = self.party[i].weapon(data) else {
            self.fought(i, data.t(0xa7e9_50ed).into(), format!("{name} has no weapon, and defends."));
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
                self.fought(i, data.t(0xfb73_3c80).into(), format!("{name}'s weapon has no effect on the {}!", mon.name));
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
                mult += 5.0 * (1.0 + 0.03 * f64::from((c.stat(data, DEX) - 15).max(0)));
            }
            if backstab {
                mult += 2.0;
            }
            if mon.flags & 2 != 0 && !c.sees_invisible(data) {
                mult -= mult / 2.0;
            }
            mult += size_term(data, c, m);
            let dmg = swing(c, att, i32::from(mon.defense), flevel, c.melee_rating(data), mult);
            if dmg == 0 {
                self.fought(i, data.t(0x0f2f_e6dc).into(), format!("{name} misses the {}.", mon.name));
                return;
            }
            // The label's words for a hit and for a kill (sub1019).
            let (verb, you) = match (critical, backstab) {
                (true, true) => ("severs", [0xb6db_bf47, 0x4921_99cb]),
                (false, true) => ("backstabs", [0x44f6_b1dd, 0x5403_ca7d]),
                (true, false) => ("critically hits", [0x9a6c_1c4c, 0x2989_57e6]),
                _ => ("hits", [0x7c13_0ece, 0xdb82_b334]),
            };
            if !self.damage_foe(data, t, i, fi, dmg, verb, Some(you), 1.0) {
                return;
            }
            match self.fight.as_ref().and_then(|f| f.pick(Target::First)) {
                Some(next) => fi = next,
                None => return,
            }
        }
    }

    /// What member i did in a fight: the original's "You" words in the controlled
    /// character's label, and the member's own line in Messages.
    pub(crate) fn fought(&mut self, i: usize, you: String, line: String) {
        if i == 0 {
            self.said[CHAR_HIT] = you;
        }
        self.say(line);
    }

    /// Applies damage to a foe, awards the dealer's XP and handles the kill. `you` are
    /// the label's words for a hit and for a kill; `share` is the XP rate (companions
    /// earn a quarter for their master).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn damage_foe(&mut self, data: &GameData, t: &Tuning, i: usize, fi: usize, dmg: i32, verb: &str, you: Option<[u32; 2]>, share: f64) -> bool {
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
        let line = format!("{name} {verb} the {mon} for {dmg}{} (+{xp} XP)", if killed { " and kills it!" } else { "." });
        match you {
            Some([_, kill]) if killed => self.fought(i, data.t(kill).into(), line),
            Some([hit, _]) => self.fought(i, format!("{} {dmg}{}", data.t(hit), data.t(0x6434_a9bd)), line),
            None => self.say(line),
        }
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
                let line = if self.party.len() == 1 {
                    data.t(0xaafc_760f).into()
                } else {
                    format!("{}{}{}{}", self.quoted(j), data.t(0xafaf_3048), data.t(if self.party[j].female { 0xf4b4_8d8c } else { 0xfdd2_b0cb }), data.t(0x92a3_3eda))
                };
                self.say(line);
            }
        }
    }

    /// Gold split evenly among the living (the original's divvying up), each told its
    /// share (MONSMOD.monGetGold).
    pub(crate) fn share_gold(&mut self, data: &GameData, gold: u64) {
        let living = self.living();
        let share = gold / living.len().max(1) as u64;
        for &j in &living {
            self.party[j].gold += share;
            if share > 0 {
                self.tell(j, CHAR_HIT, format!("{}{share}{}", data.t(0x4982_d859), data.t(0xfde9_4394)), None);
            }
        }
        if share == 0 {
            self.said[CHAR_HIT] = data.t(0x66b0_fe6a).into();
        }
    }

    fn win(&mut self, _data: &GameData, _t: &Tuning) {
        let Some(f) = self.fight.take() else { return };
        // Paralysis ends with the fight (timAilment_Timer).
        self.ailments(_data);
        match f.chest {
            // A container keeps the gold until it is opened.
            Some(mut chest) => {
                chest.gold = f.gold;
                crate::treasure::leave_chest(self, chest);
            }
            None => self.share_gold(_data, f.gold),
        }
        crate::ai::tend(self, _data, _t);
    }

    /// Gives an item to the first living member with room; what they know of it follows
    /// the original's identification roll.
    pub(crate) fn give(&mut self, data: &GameData, item: Option<usize>, chest: bool) {
        let Some(item) = item else { return };
        let Some(j) = self.living().into_iter().find(|&j| self.party[j].items.len() < 40) else {
            return self.say(data.t(0x2ec8_5095));
        };
        // fn129A's identification level, and w13 charges give or take up to half.
        let mut held: Held = crate::magic::new_held(data, item, false);
        held.id = crate::treasure::identifies(&self.party[j], data, item);
        held.align = crate::magic::roll_align(&mut self.party[j], data, item);
        let full = data.items[item].w(13);
        if full > 1 && data.items[item].id < crate::magic::POTION_OF_EXPERIENCE {
            let r = &mut self.party[j];
            let by = (r.rnd() * f64::from(full) / 2.0) as i16;
            held.charges = if r.roll(2) == 0 { full + by } else { full - by };
        }
        // MONSMOD.monOpenChest: "CHEST: Slot #3, Dagger", by name in a party.
        let found = format!("{}{}{}{}{}", data.t(if chest { 0x3e05_53f6 } else { 0xd5cb_7ed2 }).to_uppercase(), data.t(0xdc80_56b6), self.party[j].items.len() + 1, data.t(0x89d0_00f1), crate::magic::item_name(data, &held));
        let line = if self.party.len() == 1 { found } else { format!("{}{}{found}", self.quoted(j), data.t(0x9918_0f02)) };
        self.say(line);
        self.party[j].items.push(held);
        // A "sticks on by itself" curse (w40 = 2) attaches itself (monOpenChest).
        let slot = self.party[j].items.len() - 1;
        if data.items[item].w(40) == 2 && self.party[j].toggle_equip(data, slot).is_ok() {
            let line = if j == 0 { data.t(0xb969_69a3).into() } else { format!("{}\"{}\"!!", data.t(0xb328_a64b), self.party[j].name) };
            self.say(line);
        }
    }
}

/// A pick weighted by the table's weights; none when they sum to 0.
fn pick(r: &mut Character, v: &[(usize, i16)]) -> Option<usize> {
    let total: u32 = v.iter().map(|&(_, w)| w.max(0) as u32).sum();
    let mut x = r.roll(total.max(1));
    v.iter().find(|&&(_, w)| {
        let w = w.max(0) as u32;
        let hit = x < w;
        x = x.saturating_sub(w);
        hit
    }).map(|&(i, _)| i)
}

/// A new monster's hits: the table's, give or take a quarter (MORDOR.EXE).
fn new_hp(c: &mut Character, hp: i32) -> i32 {
    let v = (c.rnd() * f64::from(hp) * 0.25).floor() as i32 + 1;
    (if c.chance(0.5) { hp + v } else { hp - v }).max(1)
}
