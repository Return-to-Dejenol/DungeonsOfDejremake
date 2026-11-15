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

/// What a group looks like (MONGEN's gv1E38 @24 and @20): the monster it passes for, and
/// whether only its type is known (met on a fog square).
#[derive(Clone, Copy, Debug)]
pub struct Shown {
    pub as_m: usize,
    pub typed: bool,
}

pub struct Fight {
    pub foes: Vec<Foe>,
    /// Per group, what it looks like; a group without one is what it is.
    pub shown: Vec<Shown>,
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
        let mut new = false;
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
                new = true;
            }
        }
        self.record_groups(data, new);
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

    /// MORDOR.EXE's MONGEN (fn0D4D), step for step: the room's lair monster 75% of the
    /// time; else a type the room allows (any type 1% of the time outside the floor-1
    /// starter rooms) and a level window, then one of the type's monsters in it by spawn
    /// weight (w26); then up to w4 groups, each picked by the follower rules of the last
    /// monster picked (w48 type, w49 how, w62/w63 kin), of half to all of its group size
    /// (w32). Monster 0 (GOBLIE) is the file's template: the type ranges start at 1
    /// (sub06D7). Each `continue 'retry` is the original's GoTo back to the start.
    pub(crate) fn mongen(&mut self, data: &GameData, a: &crate::mdr::Area, area: i16, stud: bool) -> Vec<(usize, u32)> {
        let floor = self.pos().floor as i16;
        let top = floor + i16::from(stud);
        let starter = floor == 1 && area < 18;
        let r = &mut self.party[0];
        let ms = &data.monsters;
        let types = data.monster_types.len();
        // A type's monsters, kept together in the file (sub06D7).
        let range = |k: usize| {
            let first = (1..ms.len()).find(|&i| ms[i].kind as usize == k)?;
            Some((first, (first..ms.len()).take_while(|&i| ms[i].kind as usize == k).last()?))
        };
        let weight = |i: usize| f64::from(ms[i].weight);
        let allowed: Vec<usize> = (0..types).filter(|&k| a.monster_types >> k & 1 != 0 && range(k).is_some()).collect();
        'retry: for _ in 0..10_000 {
            let mut first = None;
            if a.lair != 0 && r.rnd() * 100.0 <= 75.0 {
                first = ms.iter().position(|m| m.id == a.lair);
            }
            if first.is_none() {
                let random = (r.d100() < 2 && !starter || allowed.is_empty()).then(|| r.roll(types as u32) as usize);
                // The window: 1 level below half the time (the original's 2- and 3-level
                // branches can't be reached), more by floor / 5; on a stud square 1 two
                // times in three.
                let mut w = i16::from(r.d100() < 50) + (f64::from(floor) / 5.0).round_ties_even() as i16;
                if stud && r.rnd() * 75.0 < 50.0 {
                    w = 1;
                }
                w = w.min(floor);
                let kind = match random {
                    Some(k) => k,
                    None => {
                        let tw: Vec<(f64, usize)> = allowed
                            .iter()
                            .map(|&k| {
                                let (f, l) = range(k).unwrap();
                                let from = (f..=l).find(|&i| ms[i].level >= top - w).unwrap_or(l + 1);
                                let v: Vec<usize> = (from..=l).take_while(|&i| ms[i].level <= top).collect();
                                (v.iter().map(|&i| weight(i)).sum(), v.len())
                            })
                            .collect();
                        let total: f64 = tw.iter().map(|t| t.0).sum();
                        if total == 0.0 || tw.iter().all(|t| t.1 == 0) {
                            // m02EE((Rnd * (UBound - 1))): its unused slots hold type 0.
                            let i = (r.rnd() * (types as f64 - 2.0)).round_ties_even() as usize;
                            allowed.get(i).copied().unwrap_or(0)
                        } else {
                            // The running sum starts at the first weight and adds each
                            // type's after testing it, so the first counts twice.
                            let x = r.rnd() * 101.0;
                            let mut cum = tw[0].0;
                            let mut pick = None;
                            for (k, t) in tw.iter().enumerate() {
                                if x <= cum / total * 100.0 {
                                    pick = Some(allowed[k]);
                                    break;
                                }
                                cum += t.0;
                            }
                            pick.unwrap_or(*allowed.last().unwrap())
                        }
                    }
                };
                let Some((f, l)) = range(kind) else { continue 'retry };
                let end = (f..=l).take_while(|&i| ms[i].level <= top).last().unwrap_or(f);
                let in_window = |i: usize, w: i16| (top - w..=top).contains(&ms[i].level);
                // An empty window widens up to 3 levels, else MONGEN starts over.
                let total = loop {
                    let total: f64 = (f..=end).filter(|&i| in_window(i, w)).map(weight).sum();
                    if total != 0.0 {
                        break total;
                    }
                    w += 1;
                    if w > 3 {
                        continue 'retry;
                    }
                };
                let p = |i: usize| if i <= end && in_window(i, w) { weight(i) / total * 100.0 } else { 0.0 };
                let x = r.rnd() * 100.0;
                let mut cum = p(f);
                for k in f + 1..=end + 1 {
                    if x <= cum || k > ms.len() - 1 {
                        first = Some(k - 1);
                        break;
                    }
                    cum += p(k);
                }
                let Some(m) = first else { continue 'retry };
                // The floor-1 starter rooms hold only the weakest (and Slaves).
                if starter && ms[m].power_level > 2 && m != 4 {
                    continue 'retry;
                }
            }
            let Some(first) = first else { continue 'retry };
            let w4 = ms[first].w(4);
            let mut n = w4;
            if f64::from(r.roll(100)) < f64::from(n) / 8.0 * 100.0 {
                n = (f64::from(n) - (r.rnd() * (f64::from(w4) / 2.0 + 1.0)).floor()).round_ties_even() as i16;
            }
            // The follower rules come from the last monster picked; a fixed follower (w49
            // 3-6, 8+) leaves them as they were.
            let (mut src, mut kin, mut level) = (first, ms[first].w(63), ms[first].level);
            let (mut down, mut up) = (0i16, 0i16);
            let mut groups = Vec::new();
            for g in 0..n.clamp(1, 4) {
                let m = if g == 0 {
                    first
                } else {
                    let s = &ms[src];
                    match s.w(49) {
                        3 => src,
                        4 | 6 => 4,
                        5 => 5,
                        m if m > 7 => (m - 7) as usize,
                        mode => {
                            match mode {
                                0 => (down, up) = (3, -1),
                                1 => (down, up) = (3, 0),
                                2 => (down, up) = (0, 0),
                                _ => {}
                            }
                            if level + up < 1 {
                                up = 0;
                            }
                            let Some((f, l)) = range(s.w(48).max(0) as usize) else { continue 'retry };
                            let span: Vec<usize> = (f..=l).take_while(|&i| ms[i].level <= level + up).collect();
                            let end = span.last().copied().unwrap_or(f);
                            let fits = |i: usize, kin: i16| i <= end && ms[i].level >= level - down && ms[i].w(32) != 1 && i != src && (kin <= 0 || ms[i].w(62) == kin);
                            let total: f64 = span.iter().filter(|&&i| fits(i, kin)).map(|&i| weight(i)).sum();
                            if total == 0.0 {
                                continue 'retry;
                            }
                            // The shares stay as worked out with this kin.
                            let k0 = kin;
                            let p = |i: usize| if fits(i, k0) { weight(i) / total * 100.0 } else { 0.0 };
                            // From the top of the window down; the first monster is never
                            // picked again, and a wrong kin is retried, kin dropped after
                            // 150 tries and the encounter after 300.
                            let mut tries = 0;
                            let mut chosen = None;
                            for _ in 0..300 {
                                let x = r.rnd() * 99.0;
                                let mut cum = p(end);
                                for k in (f + 1..=end + 1).rev() {
                                    if x > cum {
                                        cum += p(k - 1);
                                    }
                                    if x <= cum && k - 1 != first {
                                        if kin > 0 && ms[k - 1].w(62) != kin {
                                            tries += 1;
                                            if tries > 150 {
                                                kin = 0;
                                            }
                                            if tries > 300 {
                                                return Vec::new();
                                            }
                                            cum += p(k);
                                        } else {
                                            chosen = Some(k - 1);
                                            break;
                                        }
                                    }
                                }
                                if chosen.is_some() {
                                    break;
                                }
                            }
                            let Some(c) = chosen else { break };
                            (src, kin, level) = (c, ms[c].w(63), ms[c].level);
                            c
                        }
                    }
                };
                if m >= ms.len() {
                    break;
                }
                let size = f64::from(ms[m].w(32));
                groups.push((m, ((size - r.rnd() * (size / 2.0)).round_ties_even() as u32).max(1)));
            }
            return groups;
        }
        Vec::new()
    }

    /// How a new group of monster m looks (MONGEN): the leader's ID skill against
    /// ln(power) x (level + 1) gives a chance C = 50 + 10 x the difference, 5 to 95; past
    /// d100 > C it passes for a monster of its kin (w62) up to CInt(Rnd x (ln level +
    /// 2.5)) + 1 places away in the file, above it when d100 < C, within its type. On a
    /// fog square only its type is known.
    pub(crate) fn disguise(&mut self, d: &GameData, m: usize) -> Shown {
        let ms = &d.monsters;
        let fog = self.floor(d).is_some_and(|f| f.square(self.pos().x, self.pos().y).flags & crate::mdr::square::FOG != 0);
        let skill = crate::treasure::id_skill(&self.party[0], d);
        let c = (50.0 + (skill - f64::from(ms[m].power_level.max(1)).ln() * f64::from(ms[m].level + 1)) * 10.0).clamp(5.0, 95.0);
        let r = &mut self.party[0];
        let mut as_m = m;
        let kin: Vec<usize> = (0..ms.len()).filter(|&i| ms[i].w(62) == ms[m].w(62)).collect();
        let pos = kin.iter().position(|&i| i == m).unwrap_or(0);
        let range = (1..ms.len()).filter(|&i| ms[i].kind == ms[m].kind);
        let (lo, hi) = (range.clone().min().unwrap_or(m), range.max().unwrap_or(m));
        if f64::from(r.d100()) > c && kin.len() > 1 {
            let spread = (r.rnd() * (f64::from(ms[m].level.max(1)).ln() + 2.5)).round_ties_even();
            for tries in 1.. {
                let up = f64::from(r.d100()) < c;
                let off = (r.rnd() * (spread + 1.5)).floor() as usize;
                let at = if up { kin.get(pos + off).copied() } else { pos.checked_sub(off).map(|p| kin[p]) };
                if let Some(a) = at.filter(|&a| (lo..=hi).contains(&a)) {
                    as_m = a;
                    break;
                }
                if tries > 100 {
                    break;
                }
            }
        }
        Shown { as_m, typed: fog }
    }

    /// Group `group`'s name for n of them, as member j sees it (fn0E23).
    pub fn group_name(&self, d: &GameData, group: u8, n: usize, j: usize) -> String {
        let Some(f) = self.fight.as_ref() else { return String::new() };
        let Some(m) = f.foes.iter().find(|x| x.group == group).map(|x| x.monster) else { return String::new() };
        let shown = f.shown.get(usize::from(group)).copied().unwrap_or(Shown { as_m: m, typed: false });
        crate::monsters::plural_as(d, m, shown, n, self.party[j].sees_invisible(d))
    }

    /// Foe fi's name as its group looks, seen by everyone.
    fn foe_name(&self, d: &GameData, fi: usize) -> String {
        let Some(f) = self.fight.as_ref() else { return String::new() };
        let m = f.foes[fi].monster;
        let shown = f.shown.get(usize::from(f.foes[fi].group)).copied().unwrap_or(Shown { as_m: m, typed: false });
        crate::monsters::plural_as(d, m, shown, 1, true)
    }

    /// Each tick of the fight (timAilment_Timer), every living member may see through each
    /// group: one known only by type is identified at its ID skill x 1.25 + 20 (skill + 20
    /// in fog), 5 to 80; a disguised one at skill + 20 (x 0.75 + 20 in fog), 10 to 90.
    /// An invisible monster needs See Invisible.
    pub(crate) fn see_through(&mut self, d: &GameData) {
        let fog = self.floor(d).is_some_and(|f| f.square(self.pos().x, self.pos().y).flags & crate::mdr::square::FOG != 0);
        for j in self.living() {
            let skill = crate::treasure::id_skill(&self.party[j], d);
            let Some(f) = self.fight.as_ref() else { return };
            let groups = f.groups_from(0);
            for gr in groups {
                let Some(f) = self.fight.as_ref() else { return };
                let (Some(fi), Some(&s)) = (f.front(gr), f.shown.get(usize::from(gr))) else { continue };
                let m = f.foes[fi].monster;
                if d.monsters[m].flags & 2 != 0 && !self.party[j].sees_invisible(d) || !s.typed && s.as_m == m {
                    continue;
                }
                // Its type first, then through its disguise, each with its own roll.
                let typed = (if fog { skill + 20.0 } else { skill * 1.25 + 20.0 }).round_ties_even().clamp(5.0, 80.0);
                let veiled = (if fog { skill * 0.75 + 20.0 } else { skill + 20.0 }).round_ties_even().clamp(10.0, 90.0);
                for (on, chance, after) in [(s.typed, typed, Shown { typed: false, ..s }), (s.as_m != m, veiled, Shown { as_m: m, typed: false })] {
                    if !on || f64::from(self.party[j].d100()) >= chance {
                        continue;
                    }
                    let f = self.fight.as_mut().unwrap();
                    let now = &mut f.shown[usize::from(gr)];
                    *now = Shown { as_m: after.as_m, typed: now.typed && after.typed };
                    let n = f.foes.iter().filter(|x| x.group == gr && x.hp > 0).count();
                    let what = format!("{}{}{}.", usize::from(gr) + 1, d.t(0x95d8_e191), self.group_name(d, gr, n, j));
                    let line = if j == 0 { format!("{}{what}", d.t(0x9541_689d)) } else { format!("{}{}{what}", self.quoted(j), d.t(0x33d5_6506)) };
                    self.say(line);
                    // Seen through its disguise, it goes in the Library (sub06FE).
                    if after.as_m != s.as_m {
                        self.record_monster(d, m, Some(j), None, Some(n as u32), false);
                    }
                }
            }
        }
    }

    /// The encounter from MONGEN's groups: their hits, peace, container and gold, and the
    /// peaceful groups' offers to join (d3000 <= 10 CHA - 10 (level found - 1)).
    fn muster(&mut self, data: &GameData, groups: &[(usize, u32)], lair: i16, back: Pos) {
        let first = groups[0].0;
        let shown: Vec<Shown> = groups.iter().map(|&(m, _)| self.disguise(data, m)).collect();
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
                let offer = format!("{k} {}{}", crate::monsters::plural_as(data, m, shown[gi], k, true), data.t(if k == 1 { 0xc1ae_c013 } else { 0x57df_86ca }));
                self.said[MONSTER_HIT].clone_from(&offer);
                self.say(offer);
            }
        }
        self.fight = Some(Fight { foes, shown, round: 0, order: None, back, lair: is_lair, peaceful, chest, defending: vec![false; self.party.len()], gold, opening: None, offers });
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
        // Each member acts, then its companions (sub155A).
        for i in 0..self.party.len() {
            if self.fight.as_ref().is_none_or(Fight::over) {
                break;
            }
            if !self.party[i].able() {
                self.companions_act(data, t, i);
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
            self.companions_act(data, t, i);
        }
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
            // Weapon-resistant monsters ignore weapons below half their level, and the
            // member defends; immune ones ignore all, with no defending (sub1019).
            if mon.flags & 32 != 0 || mon.flags & 16 != 0 && weapon.level < mon.level / 2 {
                self.fought(i, data.t(0xfb73_3c80).into(), format!("{name}'s weapon has no effect on the {}!", self.foe_name(data, fi)));
                return if mon.flags & 32 == 0 { self.defend(i) };
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
                self.fought(i, data.t(0x0f2f_e6dc).into(), format!("{name} misses the {}.", self.foe_name(data, fi)));
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
        let mon = self.foe_name(data, fi);
        let line = format!("{name} {verb} the {mon} for {dmg}{} (+{xp} XP)", if killed { " and kills it!" } else { "." });
        match you {
            // A kill is told in the original's words, in Messages too ("You Killed One!").
            Some([_, kill]) if killed => self.tell(i, CHAR_HIT, data.t(kill).into(), None),
            Some([hit, _]) => self.fought(i, format!("{} {dmg}{}", data.t(hit), data.t(0x6434_a9bd)), line),
            None if verb.is_empty() => {}
            None => self.say(line),
        }
        if killed {
            self.kill(data, i, m);
            // Asmodeus: every member's guild hears of it.
            if data.monsters[m].id == 277 {
                let solo = self.party.len() == 1;
                for j in 0..self.party.len() {
                    let line = format!("\"{}\"{}", self.party[j].name, data.t(if solo { 0xf9c7_c3a3 } else { 0x02dd_656b }));
                    let g = self.party[j].guild;
                    self.hall.note(g, line);
                }
            }
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

    /// Gold split evenly over the whole party, the dead included (MONSMOD.monGetGold),
    /// each told its share.
    pub(crate) fn share_gold(&mut self, data: &GameData, gold: u64) {
        let all: Vec<usize> = (0..self.party.len()).collect();
        let share = gold / all.len().max(1) as u64;
        for &j in &all {
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
                chest.disguised = f.shown.first().is_some_and(|s| s.as_m != chest.monster);
                crate::treasure::leave_chest(self, chest);
            }
            None => self.share_gold(_data, f.gold),
        }
        crate::ai::tend(self, _data, _t);
    }

    /// Gives an item to the first living member with room; what they know of it follows
    /// the original's identification roll. The Library records the find as from monster
    /// `from` unless it is invisible to the finder (monOpenChest).
    pub(crate) fn give(&mut self, data: &GameData, item: Option<usize>, chest: bool, from: Option<usize>) {
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
        let from = from.filter(|&m| data.monsters[m].flags & 2 == 0 || self.party[j].sees_invisible(data));
        self.record_item(item, Some(j), held.id, Some(from));
        self.party[j].items.push(held);
        // A "sticks on by itself" curse (w40 = 2) attaches itself (monOpenChest).
        let slot = self.party[j].items.len() - 1;
        if data.items[item].w(40) == 2 && self.party[j].toggle_equip(data, slot).is_ok() {
            let line = if j == 0 { data.t(0xb969_69a3).into() } else { format!("{}\"{}\"!!", data.t(0xb328_a64b), self.party[j].name) };
            self.say(line);
        }
    }
}

/// A new monster's hits: the table's, give or take a quarter (MORDOR.EXE).
fn new_hp(c: &mut Character, hp: i32) -> i32 {
    let v = (c.rnd() * f64::from(hp) * 0.25).floor() as i32 + 1;
    (if c.chance(0.5) { hp + v } else { hp - v }).max(1)
}
