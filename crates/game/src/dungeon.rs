//! The Dungeon window (frmDungeonMain): the small maze view, the monster box with its
//! group labels, the movement arrows, the action buttons, the trap box and timed rounds.

use std::time::Duration;

use web_time::Instant;

use ddj_rules::{
    Tuning,
    ai::{Act, Target},
    game::{self, Life},
    mdr::{GameData, square},
    monsters, treasure,
};
use eframe::egui::{self, Color32, Key};

use crate::{App, Screen, art, win::{Desk, View}};

impl App {
    pub(crate) fn dungeon(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) {
        const F: &str = "frmDungeonMain";
        let typing = ctx.egui_wants_keyboard_input() || self.dialog.is_some();
        let mut keys = ctx.input(|i| [Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight, Key::ArrowDown, Key::T, Key::F, Key::D, Key::R, Key::Space, Key::A, Key::O, Key::J].map(|k| i.key_pressed(k) && !typing));
        let (mut ev, open) = {
            let g = self.game.as_ref().unwrap();
            let p = g.pos();
            let flags = if p.floor > 0 { d.floors[p.floor - 1].square(p.x, p.y).flags } else { 0 };
            let mut v = View::default();
            let here = g.party.iter().filter(|m| m.pos.floor == p.floor && m.pos.x == p.x && m.pos.y == p.y).count();
            v.text("lblRoom", format!("{},{},{} {} [ {here} ]", p.x + 1, p.y + 1, p.floor, d.t(game::DIRECTIONS[p.facing as usize])));
            // What just happened on the square, else its stairs or chute (sub0835's lines).
            let note = [(square::STAIRS_UP, 0xe21e_1250), (square::STAIRS_DOWN, 0x4228_c865), (square::CHUTE, 0x98e9_7440)].into_iter().find(|(b, _)| flags & b != 0).map_or("", |(_, w)| d.t(w));
            v.text("lblSquareInfo", if g.said[game::SQUARE].is_empty() { note } else { &g.said[game::SQUARE] });
            // The buffer buttons, loaded side by side at run time.
            let b0 = desk.rect(F, "cmdBuffer(0)");
            for i in 0..10u16 {
                if i > 0 {
                    v.clone_rect("cmdBuffer(0)", i, [b0[0] + b0[2] * i32::from(i), b0[1], b0[2], b0[3]]);
                }
                v.text(&format!("cmdBuffer({i})"), (i + 1).to_string());
            }
            // The container's trap as the party reads it, and the best disarmer's odds.
            let chest = g.fight.as_ref().and_then(|f| f.chest.clone()).or(g.chest.clone());
            v.show("picTrap", chest.is_some()).show("lblTrap", chest.is_some()).show("lblDisarm", chest.is_some());
            if let Some(c) = &chest {
                let (_, pr) = treasure::disarm_chance(g, d, c);
                let odds = if pr > 0.5 { "+" } else if pr > 0.35 { "*" } else { "-" };
                v.text("lblTrap", if c.magic_lock { d.t(0xcd46_ca10) } else { c.guess.name(d) }).text("lblDisarm", format!("{odds} {}", "*".repeat(c.sure as usize)));
            }
            let open = chest.is_some() && g.fight.as_ref().is_none_or(|f| f.chest.is_none());
            v.enable("cmdAction(3)", open);
            // The monster groups: up to four lines, "1. 2 Zombies".
            if let Some(f) = &g.fight {
                let mut groups: Vec<(u8, usize, usize)> = Vec::new();
                for foe in f.foes.iter().filter(|f| f.hp > 0) {
                    match groups.iter_mut().find(|x| x.0 == foe.group) {
                        Some(x) => x.2 += 1,
                        None => groups.push((foe.group, foe.monster, 1)),
                    }
                }
                v.show("picMonGroups", true).show("lblCurrentGroup", true);
                for (i, (_, m, n)) in groups.iter().take(4).enumerate() {
                    let k = format!("lblMonster({i})");
                    v.show(&k, true).text(&k, format!("{}. {n} {}", i + 1, monsters::plural(d, *m, *n, g.lead().buffs.see_invisible)));
                    if f.peaceful {
                        v.fore(&k, Color32::from_rgb(0, 128, 0));
                    }
                }
            }
            v.show("lblCharacterHit", true).text("lblCharacterHit", g.said[game::CHAR_HIT].clone());
            v.show("lblMonsterHit", true).text("lblMonsterHit", g.said[game::MONSTER_HIT].clone());
            v.show("lblDead", g.lead().life != Life::Alive);
            let fog = flags & square::FOG != 0;
            let art_ref = self.art.as_ref();
            v.paint("picMaze2", move |p, r, _| {
                if fog || p_floor_zero(g) {
                    p.rect_filled(r, 0.0, Color32::from_gray(128));
                } else {
                    art::dungeon_view(p, r, art_ref, d, g);
                }
            });
            v.paint("picMonsters", move |p, r, s| art::monsters(p, r, s, art_ref, d, g));
            v.swipe("picMaze2").swipe("picMonsters");
            (desk.window(ctx, F, v, false), open)
        };
        // The phone shell's touch pad: the same buttons, finger-sized, and Defend and Run.
        if desk.shell {
            let fight = self.game.as_ref().is_some_and(|g| g.fight.is_some());
            let mut v = View::default();
            v.enable("cmdAction(3)", open).enable("cmdDefend", fight).enable("cmdRun", fight);
            ev.clicked = desk.window(ctx, crate::stats::PAD, v, false).clicked.or(ev.clicked);
        }
        // Touch: a swipe across the maze steps (up), turns (left, right) or turns about (down).
        if let Some((_, dir)) = ev.swiped.take() {
            ev.clicked = Some(format!("cmdMove({})", [2, 1, 3, 0][usize::from(dir)]));
        }
        keys[6] |= ev.click("cmdDefend");
        keys[7] |= ev.click("cmdRun");
        let g = self.game.as_mut().unwrap();
        let before = (g.pos(), g.fight.is_some(), g.party.iter().map(|m| m.hp).sum::<i32>());
        match ev.index("cmdMove") {
            Some(0) => g.turn(3),
            Some(1) => g.turn(1),
            Some(2) => g.step(d, t),
            Some(3) => g.turn(2),
            _ => {}
        }
        if keys[1] {
            g.turn(3);
        }
        if keys[2] {
            g.turn(1);
        }
        if keys[0] {
            g.step(d, t);
        }
        if keys[3] {
            g.turn(2);
        }
        let on_up = g.pos().floor > 0 && d.floors[g.pos().floor - 1].square(g.pos().x, g.pos().y).flags & square::STAIRS_UP != 0;
        match ev.index("cmdAction") {
            Some(0) => {
                if let Some(f) = g.fight.as_mut() {
                    f.order = Some(Act::Fight(Target::Weakest));
                }
            }
            Some(1) => {
                desk.state("frmCharStat3", "tabItemTab").tab = 1;
                desk.active = Some("frmCharStat3".into());
            }
            Some(2) => g.take_stairs(d, t),
            Some(3) => {
                if let Some(c) = g.chest.clone() {
                    let (who, _) = treasure::disarm_chance(g, d, &c);
                    treasure::open(g, d, t, who);
                }
            }
            Some(6) => desk.active = Some("frmAutoMap".into()),
            Some(4 | 5) => g.log.push("Carrying characters isn't in the remake yet.".into()),
            Some(7) if on_up && g.pos().floor == 1 => g.take_stairs(d, t),
            Some(7) => g.log.push("You must be on the stairs up on level 1 to exit.".into()),
            _ => {}
        }
        if ev.index("cmdBuffer").is_some() {
            g.log.push("Buffers aren't in the remake yet.".into());
        }
        if keys[4] {
            g.take_stairs(d, t);
        }
        if keys[10]
            && g.fight.is_none()
            && let Some(c) = g.chest.clone()
        {
            let (who, _) = treasure::disarm_chance(g, d, &c);
            treasure::open(g, d, t, who);
        }
        if let Some(f) = g.fight.as_mut() {
            for (k, order) in [(keys[5], Some(Act::Fight(Target::Weakest))), (keys[6], Some(Act::Defend)), (keys[7], Some(Act::Run)), (keys[9], None), (keys[10], Some(Act::Open))] {
                if k {
                    f.order = order;
                }
            }
        }
        // J takes up a peaceful group's offer to join.
        if keys[11] {
            self.game.as_mut().unwrap().accept_offer(d);
        }
        if keys[8] {
            self.paused = !self.paused;
        }
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
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let after = (g.pos(), g.fight.is_some(), g.party.iter().map(|m| m.hp).sum::<i32>());
        let wiped = g.living().is_empty();
        if g.pos().floor == 0 || wiped {
            let dead = g.party.iter().any(|m| m.life != Life::Alive);
            if wiped {
                for m in &mut g.party {
                    m.pos = game::Pos::default();
                }
            }
            self.screen = Screen::Town;
            self.building = dead.then_some("frmCityMorgue");
        }
        if after != before || ev.clicked.is_some() {
            self.persist();
        }
    }
}

fn p_floor_zero(g: &game::Game) -> bool {
    g.pos().floor == 0
}
