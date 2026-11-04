//! The party window and the macro editor.

use ddj_rules::{
    Tuning,
    ai::{self, Act, Cond, Role, Rule, Target},
    game::Life,
    magic::{self, Magic},
    mdr::GameData,
};
use eframe::egui::{self, Color32};

use crate::{App, Screen};

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

#[derive(Default)]
pub(crate) struct RuleDraft {
    cond: usize,
    value: u8,
    text: String,
    kind: usize,
    conds: Vec<Cond>,
    act: usize,
    target: usize,
    magic: usize,
    spell: usize,
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
    /// The original's Party Information window: members, hits, spells, status.
    pub(crate) fn party_panel(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning) {
        let in_town = matches!(self.screen, Screen::Town(_));
        let g = self.game.as_mut().unwrap();
        let mut switch = None;
        let mut dismiss = None;
        egui::Grid::new("party").striped(true).show(ui, |ui| {
            ui.strong("Name");
            ui.strong("Hits");
            ui.strong("Spells");
            ui.strong("Status");
            ui.end_row();
            for (i, m) in g.party.iter().enumerate() {
                let label = if i == 0 { format!("{} (you)", m.name) } else { m.name.clone() };
                if ui.selectable_label(self.sel == i, label).clicked() {
                    self.sel = i;
                }
                ui.label(format!("{}/{}", m.hp, m.max_hp));
                ui.label(m.sp.to_string());
                let status = match m.life {
                    Life::Alive if m.status.stoned => "Stoned",
                    Life::Alive if m.status.paralysis > 0 => "Paralysed",
                    Life::Alive if m.status.poison > 0 => "Poisoned",
                    Life::Alive if m.status.disease > 0 => "Diseased",
                    Life::Alive => "OK",
                    _ => "Dead",
                };
                ui.label(status);
                if i > 0 && m.alive() && ui.small_button("Switch to").clicked() {
                    switch = Some(i);
                }
                if ui.small_button("Macros").clicked() {
                    self.editing = Some(i);
                }
                if in_town && i > 0 && ui.small_button("Dismiss").clicked() {
                    dismiss = Some(i);
                }
                ui.end_row();
            }
        });
        if !g.party[0].companions.is_empty() {
            let comps: Vec<String> = g.party[0].companions.iter().map(|c| format!("{} {}/{}", d.monsters[c.monster].name, c.hp, c.max_hp)).collect();
            ui.label(format!("Companions: {}", comps.join(", ")));
        }
        if let Some(i) = switch {
            g.switch_to(i);
            self.sel = 0;
        }
        if let Some(i) = dismiss
            && let Some(c) = g.dismiss(i)
        {
            let _ = c.save(&crate::saves());
            self.sel = 0;
        }
        if in_town {
            self.recruit(ui);
        }
        let _ = t;
    }

    pub(crate) fn macro_editor(&mut self, ctx: &egui::Context, d: &GameData, i: usize) {
        let Some(g) = self.game.as_mut() else { return };
        if i >= g.party.len() {
            self.editing = None;
            return;
        }
        let mut open = true;
        let mut changed = false;
        let c = &mut g.party[i];
        let known = magic::known(c, d);
        egui::Window::new(format!("{}'s macros", c.name)).open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.label("Checked each round from the top; the first rule whose conditions all hold, and that can be done, is used. With none, the default decision tree decides.");
            let mut op = None;
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for (k, r) in c.macros.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut r.on, "").changed();
                        if ui.small_button("Up").clicked() {
                            op = Some((k, -1));
                        }
                        if ui.small_button("Down").clicked() {
                            op = Some((k, 1));
                        }
                        if ui.small_button("Delete").clicked() {
                            op = Some((k, 0));
                        }
                        ui.label(describe(r, d));
                    });
                }
            });
            if let Some((k, dir)) = op {
                match dir {
                    0 => {
                        c.macros.remove(k);
                    }
                    -1 if k > 0 => c.macros.swap(k, k - 1),
                    1 if k + 1 < c.macros.len() => c.macros.swap(k, k + 1),
                    _ => {}
                }
                changed = true;
            }
            ui.separator();
            ui.strong("New rule");
            let r = &mut self.rule;
            ui.horizontal(|ui| {
                ui.label("When");
                egui::ComboBox::from_id_salt("cond").selected_text(CONDS[r.cond]).show_ui(ui, |ui| {
                    for (k, s) in CONDS.iter().enumerate() {
                        ui.selectable_value(&mut r.cond, k, *s);
                    }
                });
                match r.cond {
                    1..=6 | 14 | 15 => {
                        ui.add(egui::DragValue::new(&mut r.value).range(0..=100));
                    }
                    7 => {
                        ui.text_edit_singleline(&mut r.text);
                    }
                    8 => {
                        egui::ComboBox::from_id_salt("kind").selected_text(d.monster_types[r.kind].as_str()).show_ui(ui, |ui| {
                            for (k, s) in d.monster_types.iter().enumerate() {
                                ui.selectable_value(&mut r.kind, k, s.as_str());
                            }
                        });
                    }
                    _ => {}
                }
                if ui.button("Add condition").clicked() {
                    let v = r.value;
                    r.conds.push(match r.cond {
                        1 => Cond::SelfHpBelow(v),
                        2 => Cond::AllyHpBelow(v),
                        3 => Cond::PartyHpBelow(v),
                        4 => Cond::SelfSpAbove(v),
                        5 => Cond::FoesAtLeast(v),
                        6 => Cond::FoeHpBelow(v),
                        7 => Cond::FoeNamed(r.text.clone()),
                        8 => Cond::FoeKind(r.kind),
                        9 => Cond::AllyDead,
                        10 => Cond::AllyAiling,
                        11 => Cond::Lair,
                        12 => Cond::Chest,
                        13 => Cond::Peaceful,
                        14 => Cond::RoundAtLeast(v),
                        15 => Cond::RoundBelow(v),
                        _ => Cond::Always,
                    });
                }
            });
            let conds: Vec<String> = r.conds.iter().map(|c| cond_text(c, d)).collect();
            ui.label(if conds.is_empty() { "Conditions: none yet (always)".into() } else { format!("Conditions: {}", conds.join(" and ")) });
            ui.horizontal(|ui| {
                ui.label("Do");
                egui::ComboBox::from_id_salt("act").selected_text(ACTS[r.act]).show_ui(ui, |ui| {
                    for (k, s) in ACTS.iter().enumerate() {
                        ui.selectable_value(&mut r.act, k, *s);
                    }
                });
                if r.act == 2 {
                    egui::ComboBox::from_id_salt("magic").selected_text(MAGICS[r.magic].0).show_ui(ui, |ui| {
                        for (k, (s, _)) in MAGICS.iter().enumerate() {
                            ui.selectable_value(&mut r.magic, k, *s);
                        }
                    });
                }
                if r.act == 3 {
                    let name = known.get(r.spell).map_or("(no spells known)", |&s| d.spells[s].name.as_str());
                    egui::ComboBox::from_id_salt("spell").selected_text(name).show_ui(ui, |ui| {
                        for (k, &s) in known.iter().enumerate() {
                            ui.selectable_value(&mut r.spell, k, d.spells[s].name.as_str());
                        }
                    });
                }
                if matches!(r.act, 0 | 2 | 3) {
                    egui::ComboBox::from_id_salt("target").selected_text(TARGETS[r.target].0).show_ui(ui, |ui| {
                        for (k, (s, _)) in TARGETS.iter().enumerate() {
                            ui.selectable_value(&mut r.target, k, *s);
                        }
                    });
                }
            });
            let target = TARGETS[r.target].1;
            let act = match r.act {
                0 => Some(Act::Fight(target)),
                1 => Some(Act::Defend),
                2 => Some(Act::CastBest(MAGICS[r.magic].1, target)),
                3 => known.get(r.spell).map(|&s| Act::Cast(s, target)),
                4 => Some(Act::Quaff),
                5 => Some(Act::Open),
                6 => Some(Act::Run),
                _ => Some(Act::Wait),
            };
            ui.horizontal(|ui| {
                for (label, top) in [("Add at the top", true), ("Add at the bottom", false)] {
                    if let Some(act) = act
                        && ui.button(label).clicked()
                    {
                        let when = if r.conds.is_empty() { vec![Cond::Always] } else { std::mem::take(&mut r.conds) };
                        let rule = Rule { when, then: act, on: true };
                        if top { c.macros.insert(0, rule) } else { c.macros.push(rule) }
                        changed = true;
                    }
                }
                if ui.button("Clear conditions").clicked() {
                    r.conds.clear();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Reset to a starting set:");
                for (label, role) in [("Fighter", Role::Fighter), ("Healer", Role::Healer), ("Caster", Role::Caster), ("Thief", Role::Thief)] {
                    if ui.button(label).clicked() {
                        c.macros = ai::preset(role);
                        changed = true;
                    }
                }
            });
            if i > 0 {
                ui.colored_label(Color32::from_rgb(0, 0, 128), "Allies always follow their macros. For you, they run when your order is Auto.");
            }
        });
        if !open {
            self.editing = None;
        }
        if changed {
            self.persist();
        }
    }
}
