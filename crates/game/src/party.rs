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
const ACTS: [&str; 8] = ["Fight", "Defend", "Cast the best", "Cast", "Drink a healing potion", "Open the chest", "Run", "Wait"];
const TARGETS: [(&str, Target); 5] = [("the weakest foe", Target::Weakest), ("the strongest foe", Target::Strongest), ("the first foe", Target::First), ("the weakest ally", Target::LowestAlly), ("myself", Target::Myself)];
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
    let when: Vec<String> = r.when.iter().map(|c| cond_text(c, d)).collect();
    format!("When {}: {}", when.join(" and "), act_text(&r.then, d))
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
        let cond = desk.state(F, "cmbCond").sel.unwrap_or(0);
        let act = desk.state(F, "cmbAct").sel.unwrap_or(0);
        let mut v = View::default();
        v.caption = Some(format!("{}'s Macros", c.name));
        v.items("lstRules", c.macros.iter().map(|r| format!("{} {}", if r.on { "[x]" } else { "[ ]" }, describe(r, d))).collect());
        v.items("cmbCond", CONDS.iter().map(|s| s.to_string()).collect());
        v.items("cmbKind", d.monster_types.clone()).show("cmbKind", cond == 8).show("txtValue", cond != 8);
        v.items("cmbAct", ACTS.iter().map(|s| s.to_string()).collect());
        let args: Vec<String> = match act {
            2 => MAGICS.iter().map(|m| m.0.to_string()).collect(),
            3 => known.iter().map(|&s| d.spells[s].name.clone()).collect(),
            _ => Vec::new(),
        };
        v.show("cmbArg", !args.is_empty()).items("cmbArg", args).show("cmbTarget", matches!(act, 0 | 2 | 3));
        v.items("cmbTarget", TARGETS.iter().map(|t| t.0.to_string()).collect());
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
            (Some("cmdAddCond"), _) => {
                let text = desk.state(F, "txtValue").text.trim().to_string();
                let n = text.parse::<u8>().unwrap_or(0).min(100);
                let kind = desk.state(F, "cmbKind").sel.unwrap_or(0);
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
                changed = false;
            }
            (Some("cmdClear"), _) => {
                self.rule.conds.clear();
                changed = false;
            }
            (Some(b @ ("cmdTop" | "cmdBottom")), _) => {
                let target = TARGETS[desk.state(F, "cmbTarget").sel.unwrap_or(0).min(TARGETS.len() - 1)].1;
                let arg = desk.state(F, "cmbArg").sel.unwrap_or(0);
                let then = match act {
                    0 => Some(Act::Fight(target)),
                    1 => Some(Act::Defend),
                    2 => Some(Act::CastBest(MAGICS[arg.min(MAGICS.len() - 1)].1, target)),
                    3 => known.get(arg).map(|&s| Act::Cast(s, target)),
                    4 => Some(Act::Quaff),
                    5 => Some(Act::Open),
                    6 => Some(Act::Run),
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
                c.macros = ai::preset(role);
            }
            _ => changed = false,
        }
        if changed {
            self.persist();
        }
        ev.click("cmdClose") || ev.closed
    }

    /// "Pickup Character": add a saved character to the party (the toolbar's A).
    pub(crate) fn recruit(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let Some(g) = self.game.as_mut() else { return true };
        let names: Vec<String> = game::saved(&saves()).into_iter().filter(|n| !g.party.iter().any(|m| &m.name == n)).collect();
        let mut v = View::default();
        v.text("lblQuestion", "Which character joins the party?").show("txtQuestion", false).show("cmbCharacter", true).items("cmbCharacter", names.clone());
        let ev = desk.window(ctx, "frmInteract", v, true);
        if ev.click("cmdOK")
            && let Some(n) = desk.state("frmInteract", "cmbCharacter").sel.and_then(|i| names.get(i)).cloned()
        {
            let msg = match load(&n).map_err(|e| e.to_string()).and_then(|c: Character| g.add_ally(c)) {
                Ok(()) => format!("{n} joins the party."),
                Err(e) => e,
            };
            desk.state("frmInteract", "cmbCharacter").sel = None;
            self.say(msg);
            self.persist();
        }
        ev.click("cmdOK") || ev.click("cmdCancel") || ev.closed
    }

    /// The Seer's question: a character's or monster's name.
    pub(crate) fn seer(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        let mut v = View::default();
        v.caption = Some("The Seer".into());
        v.text("lblQuestion", "Who or what do you seek? (200 gold a try)");
        let ev = desk.window(ctx, "frmInteract", v, true);
        if ev.click("cmdOK") || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            let q = std::mem::take(&mut desk.state("frmInteract", "txtQuestion").text);
            if let Some(g) = self.game.as_mut() {
                let msg = g.seer(d, q.trim());
                self.say(msg);
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
            ("ComboBox", "cmbAct", None, [50, 222, 140, 21], ""),
            ("ComboBox", "cmbArg", None, [196, 222, 130, 21], ""),
            ("ComboBox", "cmbTarget", None, [332, 222, 100, 21], ""),
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
