//! The dungeon screen: movement, the view, combat orders, chests and the automap.

use std::time::Duration;

use web_time::Instant;

use ddj_rules::{
    Tuning,
    ai::{Act, Target},
    game::{self, Game, Life, Side},
    mdr::{GameData, square},
    treasure,
};
use eframe::egui::{self, Color32, Key, Rect, Stroke, pos2, vec2};

use crate::{App, Screen, art, town::Tab};

impl App {
    pub(crate) fn dungeon(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning) {
        let typing = ui.ctx().egui_wants_keyboard_input();
        let keys = ui.input(|i| [Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight, Key::ArrowDown, Key::T, Key::F, Key::D, Key::R, Key::Space, Key::A, Key::O].map(|k| i.key_pressed(k) && !typing));
        let g = self.game.as_mut().unwrap();
        let before = (g.pos(), g.fight.is_some(), g.party.iter().map(|m| m.hp).sum::<i32>());
        let press = |ui: &mut egui::Ui, label: &str, key: bool| ui.button(label).clicked() || key;
        ui.horizontal(|ui| {
            if press(ui, "Left", keys[1]) {
                g.turn(3);
            }
            if press(ui, "Forward", keys[0]) {
                g.step(d, t);
            }
            if press(ui, "Right", keys[2]) {
                g.turn(1);
            }
            if press(ui, "Turn around", keys[3]) {
                g.turn(2);
            }
            if press(ui, "Take stairs (T)", keys[4]) {
                g.take_stairs(d, t);
            }
        });
        if let Some(f) = g.fight.as_mut() {
            ui.horizontal(|ui| {
                ui.label(if f.peaceful { "Peaceful - walk on, or attack:" } else { "Your order:" });
                let orders = [("Auto (A)", None, keys[9]), ("Fight (F)", Some(Act::Fight(Target::Weakest)), keys[5]), ("Defend (D)", Some(Act::Defend), keys[6]), ("Run (R)", Some(Act::Run), keys[7])];
                for (label, order, k) in orders {
                    if ui.selectable_label(f.order == order, label).clicked() || k {
                        f.order = order;
                    }
                }
                if f.chest.is_some() && (ui.selectable_label(f.order == Some(Act::Open), "Open chest (O)").clicked() || keys[10]) {
                    f.order = Some(Act::Open);
                }
                if let Some(Act::Cast(s, _)) = f.order {
                    ui.label(format!("Casting {}", d.spells[s].name));
                }
                if ui.selectable_label(self.paused, "Pause (Space)").clicked() || keys[8] {
                    self.paused = !self.paused;
                }
            });
        }
        self.chest_panel(ui, d, t, keys[10]);
        // Timed rounds: the first comes a full round after the fight starts.
        let g = self.game.as_mut().unwrap();
        if g.fight.is_some() != self.in_fight {
            self.in_fight = g.fight.is_some();
            self.last_round = Instant::now();
        }
        let round = Duration::from_secs_f64(t.combat.round_seconds);
        if self.in_fight && !self.paused && self.last_round.elapsed() >= round {
            g.round(d, t);
            self.last_round = Instant::now();
        }
        if self.in_fight {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let p = g.pos();
        if p.floor > 0 {
            let flags = d.floors[p.floor - 1].square(p.x, p.y).flags;
            let mut notes = Vec::new();
            for (bit, s) in [(square::STAIRS_UP, "stairs up"), (square::STAIRS_DOWN, "stairs down"), (square::WATER, "water"), (square::QUICKSAND, "quicksand"), (square::FOG, "fog"), (square::ANTI_MAGIC, "anti-magic"), (square::CHUTE, "a chute")] {
                if flags & bit != 0 {
                    notes.push(s);
                }
            }
            let fog = flags & square::FOG != 0;
            let here = if notes.is_empty() { String::new() } else { format!(" - {}", notes.join(", ")) };
            let n = g.party.iter().filter(|m| m.pos.floor == p.floor).count();
            ui.label(format!("{},{},{} {} [{n}]{here}", p.x + 1, p.y + 1, p.floor, ["North", "East", "South", "West"][p.facing as usize]));
            if fog {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().min(560.0), 360.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, Color32::from_gray(70));
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Thick fog", egui::FontId::proportional(20.0), Color32::WHITE);
            } else {
                art::dungeon_view(ui, self.art.as_ref(), d, self.game.as_ref().unwrap());
            }
            self.foes(ui, d);
        }
        let g = self.game.as_ref().unwrap();
        let after = (g.pos(), g.fight.is_some(), g.party.iter().map(|m| m.hp).sum::<i32>());
        let wiped = g.living().is_empty();
        if g.pos().floor == 0 || wiped {
            let dead = g.party.iter().any(|m| m.life != Life::Alive);
            self.screen = Screen::Town(if dead { Tab::Morgue } else { Tab::Guilds });
            if wiped {
                for m in &mut self.game.as_mut().unwrap().party {
                    m.pos = game::Pos::default();
                }
            }
        }
        if after != before {
            self.persist();
        }
    }

    /// Foes with their hits, for the fight.
    fn foes(&self, ui: &mut egui::Ui, d: &GameData) {
        let Some(f) = self.game.as_ref().and_then(|g| g.fight.as_ref()) else { return };
        ui.horizontal_wrapped(|ui| {
            for fi in f.alive() {
                let foe = &f.foes[fi];
                let sleep = if foe.asleep > 0 { " (asleep)" } else { "" };
                ui.label(format!("{} {}/{}{sleep}", d.monsters[foe.monster].name, foe.hp, foe.max_hp));
            }
        });
    }

    /// The container in the room: the trap the party thinks it is, how sure (*), and
    /// the best disarmer's odds (+ good, * even, - poor), as the original showed.
    fn chest_panel(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning, key: bool) {
        let g = self.game.as_mut().unwrap();
        let in_fight = g.fight.as_ref().is_some_and(|f| f.chest.is_some());
        let Some(c) = g.fight.as_ref().and_then(|f| f.chest.clone()).or(g.chest.clone()) else { return };
        let (who, p) = treasure::disarm_chance(g, d, &c);
        let odds = if p > 0.65 { "+++" } else if p > 0.5 { "+" } else if p > 0.35 { "*" } else { "-" };
        let lock = if c.magic_lock { ", magically locked" } else { "" };
        let open = ui
            .horizontal(|ui| {
                ui.label(format!("{}: {} {} {odds}{lock} ({} would try)", if c.chest { "Chest" } else { "Box" }, c.guess.name(), "*".repeat(c.sure as usize), g.party[who].name));
                !in_fight && (ui.button("Open (O)").clicked() || key)
            })
            .inner;
        if open {
            treasure::open(g, d, t, who);
            self.persist();
        }
    }
}

/// Seen squares of the current floor, north up.
pub(crate) fn automap(ui: &mut egui::Ui, d: &GameData, g: &Game) {
    let p = g.pos();
    let Some(f) = (p.floor > 0).then(|| &d.floors[p.floor - 1]) else { return };
    let Some(seen) = g.lead().seen.get(p.floor - 1) else { return };
    ui.label(format!("Automap (level {})", p.floor));
    let cell = (ui.available_width().min(360.0) / f.width as f32).floor().max(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(cell * f.width as f32, cell * f.height as f32), egui::Sense::hover());
    let paint = ui.painter_at(rect);
    paint.rect_filled(rect, 0.0, Color32::BLACK);
    let corner = |x: usize, y: usize| pos2(rect.left() + x as f32 * cell, rect.bottom() - y as f32 * cell);
    for y in 0..f.height {
        for x in 0..f.width {
            if !seen.get(y * f.width + x).copied().unwrap_or(false) {
                continue;
            }
            let r = Rect::from_two_pos(corner(x, y), corner(x + 1, y + 1));
            let fl = f.square(x, y).flags;
            let fill = if fl & square::STAIRS_UP != 0 {
                Color32::DARK_GREEN
            } else if fl & square::STAIRS_DOWN != 0 {
                Color32::DARK_RED
            } else if fl & square::WATER != 0 {
                Color32::from_rgb(20, 40, 110)
            } else {
                Color32::from_gray(45)
            };
            paint.rect_filled(r, 0.0, fill);
            for (dir, a, b) in [(0, corner(x, y + 1), corner(x + 1, y + 1)), (1, corner(x + 1, y), corner(x + 1, y + 1)), (2, corner(x, y), corner(x + 1, y)), (3, corner(x, y), corner(x, y + 1))] {
                match game::side(f, x, y, dir) {
                    Side::Wall => paint.line_segment([a, b], Stroke::new(1.0, Color32::WHITE)),
                    Side::Door => paint.line_segment([a, b], Stroke::new(1.5, Color32::YELLOW)),
                    Side::Open => continue,
                };
            }
        }
    }
    let c = Rect::from_two_pos(corner(p.x, p.y), corner(p.x + 1, p.y + 1)).center();
    let dir = [vec2(0.0, -1.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(-1.0, 0.0)][p.facing as usize] * cell * 0.4;
    let side = vec2(-dir.y, dir.x);
    paint.add(egui::Shape::convex_polygon(vec![c + dir, c - dir + side, c - dir - side], Color32::LIGHT_BLUE, Stroke::NONE));
}
