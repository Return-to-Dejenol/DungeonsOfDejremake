//! The remake's party macros, and its use of the original's "Pickup Character" window
//! (frmInteract) to add allies and to ask the Seer. The macro editor is a window of the
//! remake's own, built in the original's style.

use ddj_rules::{
    ai::{self, Act, Cond, Role, Rule, Target},
    game::{self, Character},
    magic::{self, Magic},
    mdr::GameData,
};
use eframe::egui;

use crate::{
    App, load, saves,
    win::{Desk, View},
};

const CONDS: [&str; 16] = [
    "Always",
    "my hits below %",
    "an ally's hits below %",
    "the party's hits below %",
    "my spells above %",
    "foes at least",
    "a foe's hits below %",
    "a foe named",
    "a foe of type",
    "an ally is dead",
    "an ally is ailing",
    "in a lair",
    "a chest is here",
    "monsters are peaceful",
    "round at least",
    "round below",
];
const ACTS: [&str; 7] = ["Fight", "Defend", "Cast", "Drink a healing potion", "Open the chest", "Run", "Wait"];
static TARGETS: [(&str, Target); 5] = [("the weakest foe", Target::Weakest), ("the strongest foe", Target::Strongest), ("the first foe", Target::First), ("the weakest ally", Target::LowestAlly), ("myself", Target::Myself)];
const MAGICS: [(&str, Magic); 8] = [("heal", Magic::Heal), ("mass heal", Magic::MassHeal), ("damage spell", Magic::Damage), ("group damage spell", Magic::MassDamage), ("cure", Magic::Cure), ("raise", Magic::Raise), ("sleep spell", Magic::Sleep), ("protection", Magic::Protect)];

/// The conditions gathered for the next rule.
#[derive(Default)]
pub(crate) struct RuleDraft {
    conds: Vec<Cond>,
}

fn cond_text(c: &Cond, d: &GameData) -> String {
    match c {
        Cond::Always => "always".into(),
        Cond::SelfHpBelow(p) => format!("my hits < {p}%"),
        Cond::AllyHpBelow(p) => format!("an ally's hits < {p}%"),
        Cond::PartyHpBelow(p) => format!("party hits < {p}%"),
        Cond::SelfSpAbove(p) => format!("my spells > {p}%"),
        Cond::FoesAtLeast(n) => format!("{n}+ foes"),
        Cond::FoeHpBelow(p) => format!("a foe's hits < {p}%"),
        Cond::FoeNamed(s) => format!("a foe named \"{s}\""),
        Cond::FoeKind(k) => format!("a foe is {}", d.monster_types.get(*k).map_or("?", String::as_str)),
        Cond::AllyDead => "an ally is dead".into(),
        Cond::AllyAiling => "an ally is ailing".into(),
        Cond::Lair => "in a lair".into(),
        Cond::Chest => "a chest is here".into(),
        Cond::Peaceful => "peaceful".into(),
        Cond::RoundAtLeast(n) => format!("round {n}+"),
        Cond::RoundBelow(n) => format!("before round {n}"),
    }
}

fn target_text(t: Target) -> &'static str {
    TARGETS.iter().find(|(_, x)| *x == t).map_or("?", |(s, _)| s)
}

fn act_text(a: &Act, d: &GameData) -> String {
    match a {
        Act::Fight(t) => format!("fight {}", target_text(*t)),
        Act::Defend => "defend".into(),
        Act::Cast(s, t) => format!("cast {} on {}", d.spells[*s].name, target_text(*t)),
        Act::CastBest(m, t) => format!("cast the best {} on {}", MAGICS.iter().find(|(_, x)| x == m).map_or("?", |(s, _)| s), target_text(*t)),
        Act::Use(i) => format!("use {}", d.items[*i].name),
        Act::Quaff => "drink a healing potion".into(),
        Act::Open => "open the chest".into(),
        Act::Run => "run".into(),
        Act::Wait => "wait".into(),
    }
}

pub(crate) fn describe(r: &Rule, d: &GameData) -> String {
    let when: Vec<String> = r.when.iter().filter(|c| **c != Cond::Always).map(|c| cond_text(c, d)).collect();
    if when.is_empty() { format!("Always: {}", act_text(&r.then, d)) } else { format!("If {}: {}", when.join(", "), act_text(&r.then, d)) }
}

/// The targets an action can take: foes to fight or harm, allies to help.
fn targets(act: usize, spell: Option<usize>, d: &GameData) -> &'static [(&'static str, Target)] {
    match (act, spell.and_then(|s| magic::kind(&d.spells[s]))) {
        (0, _) | (2, Some(Magic::Damage | Magic::MassDamage | Magic::Sleep)) => &TARGETS[..3],
        (2, Some(_)) => &TARGETS[3..],
        _ => &TARGETS,
    }
}

impl App {
    /// The macro editor for member `i`: the rules top to bottom (checked each round; the
    /// first that holds and can be done is used), then the parts of a new rule.
    pub(crate) fn macros(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, i: usize) -> bool {
        const F: &str = "dlgMacros";
        let Some(g) = self.game.as_mut() else { return true };
        if i >= g.party.len() {
            return true;
        }
        let c = &mut g.party[i];
        let known = magic::known(c, d);
        // Each choice starts on its first entry.
        for k in ["cmbCond", "cmbAct", "cmbArg", "cmbTarget", "cmbKind"] {
            let st = desk.state(F, k);
            st.sel = st.sel.or(Some(0));
        }
        let cond = desk.state(F, "cmbCond").sel.unwrap_or(0);
        let act = desk.state(F, "cmbAct").sel.unwrap_or(0);
        let spell = desk.state(F, "cmbArg").sel.and_then(|a| known.get(a).copied());
        let aims = targets(act, spell, d);
        let target = desk.state(F, "cmbTarget");
        target.sel = target.sel.filter(|&t| t < aims.len()).or(Some(0));
        let mut v = View::default();
        v.caption = Some(format!("{}'s Macros", c.name));
        v.items("lstRules", c.macros.iter().map(|r| format!("{} {}", if r.on { "[x]" } else { "[ ]" }, describe(r, d))).collect());
        v.items("cmbCond", CONDS.iter().map(|s| s.to_string()).collect());
        v.items("cmbKind", d.monster_types.clone()).show("cmbKind", cond == 8).show("txtValue", !matches!(cond, 0 | 8..=13));
        v.items("cmbAct", ACTS.iter().map(|s| s.to_string()).collect());
        v.show("cmbArg", act == 2).items("cmbArg", known.iter().map(|&s| d.spells[s].name.clone()).collect()).show("cmbTarget", matches!(act, 0 | 2));
        v.items("cmbTarget", aims.iter().map(|t| t.0.to_string()).collect());
        let conds: Vec<String> = self.rule.conds.iter().map(|c| cond_text(c, d)).collect();
        v.text("lblConds", if conds.is_empty() { "Conditions: none yet (always)".into() } else { format!("Conditions: {}", conds.join(" and ")) });
        let ev = desk.window(ctx, F, v, true);
        let picked = desk.state(F, "lstRules").sel.filter(|&k| k < c.macros.len());
        let mut changed = true;
        match (ev.clicked.as_deref(), picked) {
            (Some("cmdUp"), Some(k)) if k > 0 => {
                c.macros.swap(k, k - 1);
                desk.state(F, "lstRules").sel = Some(k - 1);
            }
            (Some("cmdDown"), Some(k)) if k + 1 < c.macros.len() => {
                c.macros.swap(k, k + 1);
                desk.state(F, "lstRules").sel = Some(k + 1);
            }
            (Some("cmdToggle"), Some(k)) => c.macros[k].on = !c.macros[k].on,
            (Some("cmdDelete"), Some(k)) => {
                c.macros.remove(k);
            }
            (Some("cmdAddCond"), _) if self.rule.conds.len() >= ai::MAX_CONDS => {
                self.say(format!("A rule has at most {} conditions.", ai::MAX_CONDS));
                changed = false;
            }
            (Some("cmdAddCond"), _) => {
                let text = desk.state(F, "txtValue").text.trim().to_string();
                let kind = desk.state(F, "cmbKind").sel.unwrap_or(0);
                // Percentages 1-99, foes 1-40, rounds 1-50, names of 1-20 letters.
                let (lo, hi) = match cond {
                    5 => (1, 40),
                    14 | 15 => (1, 50),
                    _ => (1, 99),
                };
                let n = text.parse::<u8>().ok().filter(|n| (lo..=hi).contains(n));
                let bad = match cond {
                    7 => (text.is_empty() || text.chars().count() > 20).then(|| "Type a name of 1 to 20 letters.".to_string()),
                    1..=6 | 14 | 15 if n.is_none() => Some(format!("Type a number from {lo} to {hi}.")),
                    _ => None,
                };
                changed = false;
                if let Some(msg) = bad {
                    self.say(msg);
                    return false;
                }
                let n = n.unwrap_or(0);
                self.rule.conds.push(match cond {
                    1 => Cond::SelfHpBelow(n),
                    2 => Cond::AllyHpBelow(n),
                    3 => Cond::PartyHpBelow(n),
                    4 => Cond::SelfSpAbove(n),
                    5 => Cond::FoesAtLeast(n),
                    6 => Cond::FoeHpBelow(n),
                    7 => Cond::FoeNamed(text),
                    8 => Cond::FoeKind(kind),
                    9 => Cond::AllyDead,
                    10 => Cond::AllyAiling,
                    11 => Cond::Lair,
                    12 => Cond::Chest,
                    13 => Cond::Peaceful,
                    14 => Cond::RoundAtLeast(n),
                    15 => Cond::RoundBelow(n),
                    _ => Cond::Always,
                });
            }
            (Some("cmdClear"), _) => {
                self.rule.conds.clear();
                changed = false;
            }
            (Some("cmdTop" | "cmdBottom"), _) if c.macros.len() >= ai::MAX_RULES => {
                let msg = format!("{} has the most rules, {}; delete one first.", c.name, ai::MAX_RULES);
                self.say(msg);
                return false;
            }
            (Some("cmdTop" | "cmdBottom"), _) if act == 2 && spell.is_none() => {
                let msg = format!("{} knows no spells.", c.name);
                self.say(msg);
                return false;
            }
            (Some(b @ ("cmdTop" | "cmdBottom")), _) => {
                let target = aims[desk.state(F, "cmbTarget").sel.unwrap_or(0).min(aims.len() - 1)].1;
                let then = match act {
                    0 => Some(Act::Fight(target)),
                    1 => Some(Act::Defend),
                    2 => spell.map(|s| Act::Cast(s, target)),
                    3 => Some(Act::Quaff),
                    4 => Some(Act::Open),
                    5 => Some(Act::Run),
                    _ => Some(Act::Wait),
                };
                if let Some(then) = then {
                    let when = if self.rule.conds.is_empty() { vec![Cond::Always] } else { std::mem::take(&mut self.rule.conds) };
                    let rule = Rule { when, then, on: true };
                    if b == "cmdTop" { c.macros.insert(0, rule) } else { c.macros.push(rule) }
                }
            }
            _ if ev.index("cmdPreset").is_some() => {
                let role = [Role::Fighter, Role::Healer, Role::Caster, Role::Thief][usize::from(ev.index("cmdPreset").unwrap_or(0)).min(3)];
                c.macros = ai::preset(role, |k| magic::best_known(c, d, k));
            }
            _ => changed = false,
        }
        if changed {
            self.persist();
        }
        ev.click("cmdClose") || ev.closed
    }

    /// "Pickup Character": add a saved character to the party (the toolbar's A).
    pub(crate) fn recruit(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        let Some(g) = self.game.as_mut() else { return true };
        let names: Vec<String> = game::saved(&saves()).into_iter().filter(|n| !g.party.iter().any(|m| &m.name == n)).collect();
        let mut v = View::default();
        v.caption = Some(d.t(0xb5bc_a695).into());
        v.text("lblQuestion", d.t(0x2113_adde)).show("txtQuestion", false).show("cmbCharacter", true).items("cmbCharacter", names.clone());
        let ev = desk.window(ctx, "frmInteract", v, true);
        if ev.click("cmdOK")
            && let Some(n) = desk.state("frmInteract", "cmbCharacter").sel.and_then(|i| names.get(i)).cloned()
        {
            let msg = load(&n).map_err(|e| e.to_string()).and_then(|c: Character| g.add_ally(d, c)).err().unwrap_or_default();
            desk.state("frmInteract", "cmbCharacter").sel = None;
            self.say(msg);
            self.persist();
        }
        ev.click("cmdOK") || ev.click("cmdCancel") || ev.closed
    }

    /// The Seer's question for its Character (0), Monster (2) or Item (3) button.
    pub(crate) fn seer(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, button: u16) -> bool {
        let (caption, question) = match button {
            0 => (0xf3de_8418, 0x2113_adde),
            2 => (0xd2c4_c9d9, 0xd869_f803),
            _ => (0xd8fc_e04e, 0x9c88_32e8),
        };
        let mut v = View::default();
        v.caption = Some(d.t(caption).into());
        v.text("lblQuestion", d.t(question));
        let ev = desk.window(ctx, "frmInteract", v, true);
        if ev.click("cmdOK") || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            let q = std::mem::take(&mut desk.state("frmInteract", "txtQuestion").text);
            if let Some(g) = self.game.as_mut() {
                let (title, text) = g.seer(d, button, q.trim());
                self.boxes.push((d.t(title).into(), text));
                self.persist();
            }
            return true;
        }
        ev.click("cmdCancel") || ev.closed
    }
}

/// The macro editor's window.
pub(crate) fn add_windows(desk: &mut Desk) {
    desk.synth(
        "dlgMacros",
        "Macros",
        440,
        306,
        &[
            ("ListBox", "lstRules", None, [8, 8, 424, 122], ""),
            ("CommandButton", "cmdUp", None, [8, 134, 70, 22], "&Up"),
            ("CommandButton", "cmdDown", None, [82, 134, 70, 22], "&Down"),
            ("CommandButton", "cmdToggle", None, [156, 134, 70, 22], "&On/Off"),
            ("CommandButton", "cmdDelete", None, [230, 134, 70, 22], "De&lete"),
            ("Label", "lblWhen", None, [8, 168, 40, 14], "When"),
            ("ComboBox", "cmbCond", None, [50, 164, 170, 21], ""),
            ("TextBox", "txtValue", None, [226, 164, 60, 21], ""),
            ("ComboBox", "cmbKind", None, [226, 164, 60, 21], ""),
            ("CommandButton", "cmdAddCond", None, [292, 164, 140, 22], "Add &Condition"),
            ("Label", "lblConds", None, [8, 190, 424, 28], ""),
            ("Label", "lblDo", None, [8, 226, 40, 14], "Do"),
            ("ComboBox", "cmbAct", None, [50, 222, 130, 21], ""),
            ("ComboBox", "cmbArg", None, [186, 222, 120, 21], ""),
            ("ComboBox", "cmbTarget", None, [312, 222, 120, 21], ""),
            ("CommandButton", "cmdTop", None, [8, 250, 120, 22], "Add at &Top"),
            ("CommandButton", "cmdBottom", None, [134, 250, 120, 22], "Add at &Bottom"),
            ("CommandButton", "cmdClear", None, [260, 250, 90, 22], "Cl&ear"),
            ("Label", "lblReset", None, [8, 282, 60, 14], "Reset to:"),
            ("CommandButton", "cmdPreset", Some(0), [66, 278, 64, 22], "Fighter"),
            ("CommandButton", "cmdPreset", Some(1), [134, 278, 64, 22], "Healer"),
            ("CommandButton", "cmdPreset", Some(2), [202, 278, 64, 22], "Caster"),
            ("CommandButton", "cmdPreset", Some(3), [270, 278, 64, 22], "Thief"),
            ("CommandButton", "cmdClose", None, [352, 278, 80, 22], "Close"),
        ],
    );
}
