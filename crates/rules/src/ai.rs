//! Party AI. Each character carries macros: ordered rules of conditions and an action,
//! first match wins, so a player can script an ally ("when an ally is under 40%, cast
//! the best heal on the weakest ally"). When no rule fits, a decision tree by role
//! decides: raise and heal first, cure, protect the weak, spend spells where they pay,
//! and gang up on the foe closest to dying.

use serde::{Deserialize, Serialize};

use crate::{
    Tuning,
    game::Game,
    magic::{self, Magic},
    mdr::GameData,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// The foe with the fewest hits left.
    Weakest,
    Strongest,
    First,
    /// The ally (or self) with the lowest share of hits.
    LowestAlly,
    Myself,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Act {
    Fight(Target),
    Defend,
    /// A particular spell (index into `GameData::spells`).
    Cast(usize, Target),
    /// The best spell of a kind the character can cast now.
    CastBest(Magic, Target),
    /// Uses the first carried item of this kind (index into `GameData::items`).
    Use(usize),
    /// Uses the best carried healing potion.
    Quaff,
    /// Opens (or disarms) the room's chest.
    Open,
    Run,
    Wait,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cond {
    Always,
    /// Percent of max hits.
    SelfHpBelow(u8),
    AllyHpBelow(u8),
    /// The party's average.
    PartyHpBelow(u8),
    SelfSpAbove(u8),
    FoesAtLeast(u8),
    /// The weakest foe's share of its hits.
    FoeHpBelow(u8),
    /// Any foe whose name contains this text.
    FoeNamed(String),
    /// Any foe of this monster type (index into `GameData::monster_types`).
    FoeKind(usize),
    AllyDead,
    /// Poisoned, diseased or paralysed.
    AllyAiling,
    Lair,
    Chest,
    Peaceful,
    RoundAtLeast(u8),
    RoundBelow(u8),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub when: Vec<Cond>,
    pub then: Act,
    #[serde(default = "yes")]
    pub on: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    Fighter,
    Healer,
    Caster,
    Thief,
}

impl Role {
    pub fn of(guild: usize) -> Self {
        match guild {
            2 | 11 => Role::Healer,
            8..=10 => Role::Caster,
            3 | 4 | 6 | 7 => Role::Thief,
            _ => Role::Fighter,
        }
    }
}

fn rule(when: Vec<Cond>, then: Act) -> Rule {
    Rule { when, then, on: true }
}

/// The most rules a character keeps, and conditions a rule has.
pub const MAX_RULES: usize = 12;
pub const MAX_CONDS: usize = 3;

/// Starting macros for a role; the player can edit them. Spells are the character's
/// best known of each kind (`best`), and a kind it knows none of is left out.
pub fn preset(role: Role, best: impl Fn(Magic) -> Option<usize>) -> Vec<Rule> {
    use Cond::*;
    let mut v = vec![rule(vec![SelfHpBelow(20)], Act::Quaff)];
    let spells: &[(Vec<Cond>, Magic, Target)] = &match role {
        Role::Healer => vec![
            (vec![AllyDead], Magic::Raise, Target::LowestAlly),
            (vec![PartyHpBelow(60)], Magic::MassHeal, Target::LowestAlly),
            (vec![AllyHpBelow(50)], Magic::Heal, Target::LowestAlly),
            (vec![AllyAiling], Magic::Cure, Target::LowestAlly),
        ],
        Role::Caster => vec![
            (vec![FoesAtLeast(3), SelfSpAbove(25)], Magic::Sleep, Target::Strongest),
            (vec![FoesAtLeast(2), SelfSpAbove(20)], Magic::MassDamage, Target::Strongest),
            (vec![SelfSpAbove(30)], Magic::Damage, Target::Strongest),
        ],
        _ => Vec::new(),
    };
    v.extend(spells.iter().filter_map(|(when, k, t)| best(*k).map(|s| rule(when.clone(), Act::Cast(s, *t)))));
    if role == Role::Thief {
        v.push(rule(vec![Chest, Peaceful], Act::Open));
    }
    v.push(rule(vec![SelfHpBelow(15)], Act::Defend));
    v.push(rule(vec![Always], Act::Fight(Target::Weakest)));
    v
}

fn pct(a: i32, b: i32) -> u8 {
    (100 * a.max(0) / b.max(1)).min(100) as u8
}

fn holds(g: &Game, d: &GameData, t: &Tuning, i: usize, c: &Cond) -> bool {
    let me = &g.party[i];
    let f = g.fight.as_ref();
    let foes = || f.into_iter().flat_map(|f| f.alive().map(|fi| &f.foes[fi]));
    let living = || g.party.iter().filter(|m| m.alive());
    match c {
        Cond::Always => true,
        Cond::SelfHpBelow(p) => pct(me.hp, me.max_hp) < *p,
        Cond::AllyHpBelow(p) => living().any(|m| pct(m.hp, m.max_hp) < *p),
        Cond::PartyHpBelow(p) => {
            let n = living().count().max(1) as u32;
            (living().map(|m| u32::from(pct(m.hp, m.max_hp))).sum::<u32>() / n) < u32::from(*p)
        }
        Cond::SelfSpAbove(p) => pct(me.sp, me.max_sp(d, t)) > *p,
        Cond::FoesAtLeast(n) => foes().count() >= usize::from(*n),
        Cond::FoeHpBelow(p) => foes().any(|x| pct(x.hp, x.max_hp) < *p),
        Cond::FoeNamed(s) => foes().any(|x| d.monsters[x.monster].name.to_lowercase().contains(&s.to_lowercase())),
        Cond::FoeKind(k) => foes().any(|x| d.monsters[x.monster].kind as usize == *k),
        Cond::AllyDead => g.party.iter().any(|m| !m.alive() && matches!(m.life, crate::game::Life::Dead(_))),
        Cond::AllyAiling => living().any(|m| m.status.poison > 0 || m.status.disease > 0 || m.status.paralysis > 0),
        Cond::Lair => f.is_some_and(|f| f.lair),
        Cond::Chest => f.is_some_and(|f| f.chest.is_some()),
        Cond::Peaceful => f.is_some_and(|f| f.peaceful),
        Cond::RoundAtLeast(n) => f.is_some_and(|f| f.round >= u32::from(*n)),
        Cond::RoundBelow(n) => f.is_some_and(|f| f.round < u32::from(*n)),
    }
}

/// Turns an action into one that can be done now, or `None`. Out of a fight only spells
/// and items are; the original's combat gates apply to both (`magic::refusal`).
pub(crate) fn resolve(g: &Game, d: &GameData, t: &Tuning, i: usize, act: Act) -> Option<Act> {
    let me = &g.party[i];
    let f = g.fight.as_ref();
    let anti = g.floor(d).is_some_and(|fl| fl.square(g.pos().x, g.pos().y).flags & crate::mdr::square::ANTI_MAGIC != 0);
    if anti && matches!(act, Act::Cast(..) | Act::CastBest(..)) {
        return None;
    }
    let gated = |item: usize| magic::item_spell(d, item).is_none_or(|s| magic::refusal(g, &d.spells[s]).is_none());
    match act {
        // Melee needs a weapon (MORDOR.EXE); without one a member defends.
        Act::Fight(_) => f.filter(|f| !f.over() && me.weapon(d).is_some()).map(|_| act),
        Act::CastBest(kind, target) => magic::best(g, d, t, i, kind).map(|s| Act::Cast(s, target)),
        Act::Cast(s, _) => (magic::castable(me, d, t, s) && magic::refusal(g, &d.spells[s]).is_none()).then_some(act),
        Act::Use(item) => (me.items.iter().any(|h| h.item == item && !h.equipped) && gated(item)).then_some(act),
        Act::Quaff => magic::healing_potion(me, d).map(|slot| me.items[slot].item).filter(|&it| gated(it)).map(Act::Use),
        Act::Open => f.filter(|f| f.chest.is_some()).map(|_| act),
        Act::Run => (i == 0 && f.is_some()).then_some(act),
        Act::Defend | Act::Wait => f.map(|_| act),
    }
}

/// Between fights the party tends its wounds (docs/balance.md): each able member's macros
/// and default tree, out of combat, for heals, cures and raises only, a few at most.
pub fn tend(g: &mut Game, d: &GameData, t: &Tuning) {
    let mends = |g: &Game, act: Act| {
        let s = match act {
            Act::Cast(s, _) => Some(s),
            Act::Use(item) => magic::item_spell(d, item),
            _ => None,
        };
        s.and_then(|s| magic::kind(&d.spells[s])).is_some_and(|k| matches!(k, Magic::Heal | Magic::MassHeal | Magic::Cure | Magic::Raise)) && g.fight.is_none()
    };
    for i in 0..g.party.len() {
        for _ in 0..6 {
            let act = decide(g, d, t, i);
            if !g.party[i].able() || !mends(g, act) {
                break;
            }
            g.act(d, t, i, act);
        }
    }
}

/// What member `i` does this round.
pub fn decide(g: &Game, d: &GameData, t: &Tuning, i: usize) -> Act {
    for r in g.party[i].macros.iter().filter(|r| r.on) {
        if r.when.iter().all(|c| holds(g, d, t, i, c))
            && let Some(act) = resolve(g, d, t, i, r.then)
        {
            return act;
        }
    }
    fallback(g, d, t, i)
}

/// The default tree, used when no macro applies.
fn fallback(g: &Game, d: &GameData, t: &Tuning, i: usize) -> Act {
    use Cond::*;
    let try_ = |act: Act| resolve(g, d, t, i, act);
    let Some(f) = g.fight.as_ref() else {
        // Between fights: raise, then mend.
        let calm: [(Vec<Cond>, Act); 4] = [
            (vec![AllyDead], Act::CastBest(Magic::Raise, Target::LowestAlly)),
            (vec![PartyHpBelow(70)], Act::CastBest(Magic::MassHeal, Target::LowestAlly)),
            (vec![AllyHpBelow(75)], Act::CastBest(Magic::Heal, Target::LowestAlly)),
            (vec![AllyAiling], Act::CastBest(Magic::Cure, Target::LowestAlly)),
        ];
        return calm.into_iter().find_map(|(when, act)| when.iter().all(|c| holds(g, d, t, i, c)).then(|| try_(act)).flatten()).unwrap_or(Act::Wait);
    };
    let me = &g.party[i];
    let tree: [(Vec<Cond>, Act); 9] = [
        // Peaceful monsters are left alone unless the controlled character starts it.
        (vec![Peaceful], Act::Wait),
        (vec![AllyDead], Act::CastBest(Magic::Raise, Target::LowestAlly)),
        (vec![PartyHpBelow(55)], Act::CastBest(Magic::MassHeal, Target::LowestAlly)),
        (vec![AllyHpBelow(45)], Act::CastBest(Magic::Heal, Target::LowestAlly)),
        (vec![AllyAiling], Act::CastBest(Magic::Cure, Target::LowestAlly)),
        (vec![SelfHpBelow(25)], Act::Quaff),
        (vec![FoesAtLeast(2), SelfSpAbove(25)], Act::CastBest(Magic::MassDamage, Target::Strongest)),
        // A foe that would take several rounds of swords is worth a spell.
        (vec![SelfSpAbove(40)], Act::CastBest(Magic::Damage, Target::Strongest)),
        (vec![SelfHpBelow(15)], Act::Defend),
    ];
    for (when, act) in tree {
        if when.iter().all(|c| holds(g, d, t, i, c))
            && let Some(a) = try_(act)
        {
            if act == Act::CastBest(Magic::Damage, Target::Strongest) {
                let toughest = f.pick(Target::Strongest).map_or(0, |fi| f.foes[fi].hp);
                if toughest < 3 * magic::melee_estimate(me, d) {
                    continue;
                }
            }
            return a;
        }
    }
    if f.peaceful {
        Act::Wait
    } else {
        try_(Act::Fight(Target::Weakest)).unwrap_or(Act::Defend)
    }
}
