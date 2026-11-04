use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use ddj_rules::{
    Tuning,
    game::{self, Action, Character, Game, Life, Side},
    mdr::{GameData, square},
};
use eframe::egui::{self, Color32, Key, Rect, Stroke, pos2, vec2};

enum Screen {
    Import,
    Menu,
    Create(Draft),
    Town(Tab),
    Dungeon,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Guilds,
    Store,
    Morgue,
}

struct Draft {
    name: String,
    race: usize,
    female: bool,
    alignment: u8,
    stats: [i16; 6],
}

struct App {
    tuning: Result<Tuning, String>,
    data: Option<GameData>,
    /// The original monster portraits (MG11.DLL `MONS`), 60x60 tiles ten to a row.
    mons: Option<egui::TextureHandle>,
    import_path: String,
    status: String,
    screen: Screen,
    game: Option<Game>,
    paused: bool,
    in_fight: bool,
    last_round: Instant,
}

fn saves() -> PathBuf {
    PathBuf::from("saves")
}

/// Player patches: every `patches/*.toml` except the built-in `modern.toml`, by name.
fn load_tuning() -> Result<Tuning, String> {
    let mut paths: Vec<_> = fs::read_dir("patches")
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "toml") && !p.ends_with("modern.toml"));
    paths.sort();
    let texts: Vec<String> = paths.iter().filter_map(|p| fs::read_to_string(p).ok()).collect();
    Tuning::load(&texts.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| e.to_string())
}

impl App {
    fn new() -> Self {
        let data = GameData::load(&ddj_extract::data_dir()).ok();
        let screen = if data.is_some() { Screen::Menu } else { Screen::Import };
        Self {
            tuning: load_tuning(),
            data,
            mons: None,
            import_path: String::new(),
            status: String::new(),
            screen,
            game: None,
            paused: false,
            in_fight: false,
            last_round: Instant::now(),
        }
    }

    /// Saves after every change; the file is small.
    fn persist(&mut self) {
        if let Some(g) = &self.game
            && let Err(e) = g.ch.save(&saves())
        {
            self.status = format!("Could not save: {e}");
        }
    }

    fn import(&mut self, ui: &mut egui::Ui) {
        ui.heading("Depths of Dejenol remake");
        ui.label("Point this at your copy of the original game (MORDOR11.ZIP, MORDOR.WIP, or the install folder):");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.import_path);
            if ui.button("Import").clicked() {
                let dir = ddj_extract::data_dir();
                self.status = match ddj_extract::import(self.import_path.trim().as_ref(), &dir).and_then(|_| GameData::load(&dir)) {
                    Ok(d) => {
                        self.data = Some(d);
                        self.screen = Screen::Menu;
                        String::new()
                    }
                    Err(e) => format!("Import failed: {e}"),
                };
            }
        });
    }

    fn menu(&mut self, ui: &mut egui::Ui, d: &GameData) {
        ui.heading("Depths of Dejenol remake");
        ui.label(format!("{} dungeon levels imported.", d.floors.len()));
        ui.add_space(8.0);
        if ui.button("Create a character").clicked() {
            let stats = game::start_stats(d, 0);
            self.screen = Screen::Create(Draft { name: String::new(), race: 0, female: false, alignment: 1, stats });
        }
        ui.add_space(8.0);
        for name in game::saved(&saves()) {
            if ui.button(format!("Play {name}")).clicked() {
                match Character::load(&saves().join(format!("{name}.json"))) {
                    Ok(ch) => {
                        self.game = Some(Game::new(ch));
                        self.screen = Screen::Town(Tab::Guilds);
                    }
                    Err(e) => self.status = format!("Could not load {name}: {e}"),
                }
            }
        }
    }

    fn create(&mut self, ui: &mut egui::Ui, d: &GameData) {
        let Screen::Create(dr) = &mut self.screen else { return };
        ui.heading("Create a character");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut dr.name);
        });
        let race = dr.race;
        egui::ComboBox::from_label("Race").selected_text(d.races[race].name.as_str()).show_ui(ui, |ui| {
            for (i, r) in d.races.iter().enumerate() {
                ui.selectable_value(&mut dr.race, i, r.name.as_str());
            }
        });
        if dr.race != race {
            dr.stats = game::start_stats(d, dr.race);
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut dr.female, false, "Male");
            ui.selectable_value(&mut dr.female, true, "Female");
        });
        ui.horizontal(|ui| {
            for (a, name) in ["Good", "Neutral", "Evil"].into_iter().enumerate() {
                if d.races[dr.race].alignments & (1 << a) != 0 {
                    ui.selectable_value(&mut dr.alignment, a as u8, name);
                }
            }
        });
        let base = game::start_stats(d, dr.race);
        let left = game::FREE_POINTS - dr.stats.iter().zip(base).map(|(s, b)| s - b).sum::<i16>();
        let r = &d.races[dr.race];
        egui::Grid::new("stats").show(ui, |ui| {
            for i in 0..6 {
                ui.label(game::STATS[i]);
                ui.label(dr.stats[i].to_string());
                if ui.add_enabled(dr.stats[i] > r.stat_min[i], egui::Button::new("-")).clicked() {
                    dr.stats[i] -= 1;
                }
                if ui.add_enabled(left > 0 && dr.stats[i] < r.stat_max[i], egui::Button::new("+")).clicked() {
                    dr.stats[i] += 1;
                }
                ui.label(format!("({}-{})", r.stat_min[i], r.stat_max[i]));
                ui.end_row();
            }
        });
        ui.label(format!("{left} points left"));
        let open: Vec<&str> = d
            .guilds
            .iter()
            .filter(|g| g.races & (1 << dr.race) != 0 && g.alignments & (1 << dr.alignment) != 0 && (0..6).all(|i| dr.stats[i] >= g.stat_req[i]))
            .map(|g| g.name.as_str())
            .collect();
        ui.label(format!("Guilds you could join now: {}", open.join(", ")));
        let (create, back) = ui.horizontal(|ui| (ui.button("Create").clicked(), ui.button("Back").clicked())).inner;
        if back {
            self.screen = Screen::Menu;
        } else if create && game::saved(&saves()).contains(&dr.name.trim().to_string()) {
            self.status = "That name is taken.".into();
        } else if create {
            match Character::new(d, &dr.name, dr.race, dr.female, dr.alignment, dr.stats) {
                Ok(ch) => {
                    self.game = Some(Game::new(ch));
                    self.screen = Screen::Town(Tab::Store);
                    self.status = "Welcome to Mordor! The store has the standard starting kit.".into();
                    self.persist();
                }
                Err(e) => self.status = e,
            }
        }
    }

    fn sheet(ui: &mut egui::Ui, g: &Game, d: &GameData, t: &Tuning) {
        let c = &g.ch;
        ui.heading(&c.name);
        let sex = if c.female { "Female" } else { "Male" };
        let align = ["Good", "Neutral", "Evil"][c.alignment as usize];
        ui.label(format!("{sex} {} ({align})", d.races[c.race].name));
        ui.label(format!("{} level {}", d.guilds[c.guild].name, c.level()));
        let (a, df) = c.att_def(d);
        egui::Grid::new("sheet").show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            };
            row(ui, "Hits", format!("{}/{}", c.hp, c.max_hp));
            row(ui, "Att/Def", format!("{a}/{df}"));
            row(ui, "Experience", format!("{}/{}", c.xp[c.guild], c.xp_needed(d, t)));
            row(ui, "Gold", c.gold.to_string());
            row(ui, "Kills", c.kills.to_string());
            for i in 0..6 {
                row(ui, game::STATS[i], c.stats[i].to_string());
            }
        });
        match c.life {
            Life::Dead(p) => ui.colored_label(Color32::RED, format!("Dead on level {} at {},{}", p.floor, p.x + 1, p.y + 1)),
            Life::InMorgue => ui.colored_label(Color32::RED, "Dead, in the morgue"),
            Life::Alive => ui.label(""),
        };
    }

    /// Items with Equip buttons, and Sell when in the store.
    fn inventory(&mut self, ui: &mut egui::Ui, d: &GameData, store: bool) {
        let Some(g) = self.game.as_mut() else { return };
        ui.label(format!("Items ({}/40)", g.ch.items.len()));
        let mut act = None;
        egui::ScrollArea::vertical().id_salt("items").max_height(260.0).show(ui, |ui| {
            for (i, h) in g.ch.items.iter().enumerate() {
                let it = &d.items[h.item];
                ui.horizontal(|ui| {
                    if ui.button(if h.equipped { "Unequip" } else { "Equip" }).clicked() {
                        act = Some((i, false));
                    }
                    if store && ui.button(format!("Sell {}", it.price / 2)).clicked() {
                        act = Some((i, true));
                    }
                    ui.label(format!("{}{} [{}/{}]", if h.equipped { "* " } else { "" }, it.name, it.attack, it.defense));
                });
            }
        });
        match act {
            Some((i, true)) => self.status = g.ch.sell(d, i),
            Some((i, false)) => self.status = g.ch.toggle_equip(d, i).err().unwrap_or_default(),
            None => return,
        }
        self.persist();
    }

    fn town(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning, mut tab: Tab) {
        let Some(g) = self.game.as_mut() else { return };
        ui.heading("The City");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut tab, Tab::Guilds, "Guilds");
            ui.selectable_value(&mut tab, Tab::Store, "Store");
            ui.selectable_value(&mut tab, Tab::Morgue, "Morgue");
            ui.separator();
            if ui.button("Enter the dungeon").clicked() {
                match g.enter_dungeon(d) {
                    Ok(()) => self.screen = Screen::Dungeon,
                    Err(e) => self.status = e,
                }
            }
            if ui.button("Save and quit").clicked() {
                self.screen = Screen::Menu;
            }
        });
        if let Screen::Town(_) = self.screen {
            self.screen = Screen::Town(tab);
        }
        ui.separator();
        let g = self.game.as_mut().unwrap();
        let mut changed = false;
        match tab {
            Tab::Guilds => {
                let (need, gold) = (g.ch.xp_needed(d, t), Tuning::level_gold(g.ch.level()));
                ui.label(format!("The {} guild asks {need} experience and {gold} gold for level {}.", d.guilds[g.ch.guild].name, g.ch.level() + 1));
                if ui.button("Make level").clicked() {
                    self.status = g.ch.make_level(d, t).unwrap_or_else(|e| e);
                    changed = true;
                }
                ui.add_space(6.0);
                ui.label(format!("Joining a new guild costs {} gold.", g.ch.join_cost()));
                egui::Grid::new("guilds").show(ui, |ui| {
                    for (i, gd) in d.guilds.iter().enumerate() {
                        ui.label(&gd.name);
                        let lv = g.ch.levels[i];
                        ui.label(if lv > 0 { format!("level {lv}") } else { String::new() });
                        let label = if i == g.ch.guild { None } else if lv > 0 { Some("Act as") } else { Some("Join") };
                        if let Some(l) = label
                            && ui.button(l).clicked()
                        {
                            self.status = g.ch.join(d, i).unwrap_or_else(|e| e);
                            changed = true;
                        }
                        ui.end_row();
                    }
                });
            }
            Tab::Store => {
                ui.columns(2, |cols| {
                    cols[0].label("For sale [att/def] (power)");
                    egui::ScrollArea::vertical().id_salt("stock").show(&mut cols[0], |ui| {
                        for i in game::stock(d) {
                            let it = &d.items[i];
                            ui.horizontal(|ui| {
                                if ui.button(format!("Buy {}", it.price)).clicked() {
                                    self.status = g.ch.buy(d, i).unwrap_or_else(|e| e);
                                    changed = true;
                                }
                                let usable = it.guilds & (1 << g.ch.guild) != 0;
                                let text = format!("{} [{}/{}]{}", it.name, it.attack, it.defense, if it.power > 0 { format!(" ({})", it.power) } else { String::new() });
                                ui.label(if usable { egui::RichText::new(text).strong() } else { egui::RichText::new(text).weak() });
                            });
                        }
                    });
                });
            }
            Tab::Morgue => match g.ch.life {
                Life::Alive => {
                    ui.label("The morgue workers eye you hopefully.");
                }
                Life::Dead(p) => {
                    ui.label(format!("Your body lies on level {}.", p.floor));
                    if ui.button(format!("Hire rescuers ({} gold)", g.rescue_cost())).clicked() {
                        g.rescue();
                        changed = true;
                    }
                }
                Life::InMorgue => {
                    if ui.button(format!("Raise ({} gold)", g.raise_cost())).clicked() {
                        g.raise(d);
                        changed = true;
                    }
                }
            },
        }
        if changed {
            self.persist();
        }
        if tab == Tab::Store {
            ui.separator();
            self.inventory(ui, d, true);
        }
    }

    fn dungeon(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning) {
        let g = self.game.as_mut().unwrap();
        let before = (g.ch.pos, g.fight.is_some());
        let keys = ui.input(|i| [Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight, Key::ArrowDown, Key::T, Key::F, Key::D, Key::R, Key::Space].map(|k| i.key_pressed(k)));
        let press = |ui: &mut egui::Ui, label: &str, key: bool| ui.button(label).clicked() || key;
        ui.horizontal(|ui| {
            if press(ui, "Left", keys[1]) {
                g.turn(3);
            }
            if press(ui, "Forward", keys[0]) {
                g.step(d);
            }
            if press(ui, "Right", keys[2]) {
                g.turn(1);
            }
            if press(ui, "Turn around", keys[3]) {
                g.turn(2);
            }
            if press(ui, "Take stairs (T)", keys[4]) {
                g.take_stairs(d);
            }
            ui.separator();
            if let Some(f) = g.fight.as_mut() {
                for (label, a, k) in [("Fight (F)", Action::Fight, keys[5]), ("Defend (D)", Action::Defend, keys[6]), ("Run (R)", Action::Run, keys[7])] {
                    if ui.selectable_label(f.action == a, label).clicked() || k {
                        f.action = a;
                    }
                }
                if ui.selectable_label(self.paused, "Pause (Space)").clicked() || keys[8] {
                    self.paused = !self.paused;
                }
            }
        });
        // Timed rounds: the first one comes a full round after the fight starts.
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
        let p = g.ch.pos;
        if p.floor > 0 {
            let flags = d.floors[p.floor - 1].square(p.x, p.y).flags;
            let stairs = match (flags & square::STAIRS_UP != 0, flags & square::STAIRS_DOWN != 0) {
                (true, _) => " - stairs up",
                (_, true) => " - stairs down",
                _ => "",
            };
            ui.label(format!("{},{},{} {}{stairs}", p.x + 1, p.y + 1, p.floor, ["North", "East", "South", "West"][p.facing as usize]));
            self.view(ui, d);
        }
        let g = self.game.as_ref().unwrap();
        let (changed, life) = ((g.ch.pos, g.fight.is_some()) != before, g.ch.life);
        if g.ch.pos.floor == 0 {
            self.screen = Screen::Town(if life == Life::Alive { Tab::Guilds } else { Tab::Morgue });
        }
        if changed {
            self.persist();
        }
    }

    /// The first-person view: up to four squares ahead, walls as shaded panels.
    fn view(&mut self, ui: &mut egui::Ui, d: &GameData) {
        let g = self.game.as_ref().unwrap();
        let p = g.ch.pos;
        let f = &d.floors[p.floor - 1];
        let size = vec2(ui.available_width().min(560.0), 360.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let paint = ui.painter_at(rect);
        paint.rect_filled(rect, 0.0, Color32::from_gray(12));
        let scale = [1.0, 0.62, 0.38, 0.24, 0.15];
        let at = |k: usize| Rect::from_center_size(rect.center(), rect.size() * scale[k]);
        let mut squares = vec![(p.x, p.y)];
        while squares.len() < 4 && side(f, *squares.last().unwrap(), p.facing) == Side::Open {
            match game::ahead(f, squares.last().unwrap().0, squares.last().unwrap().1, p.facing) {
                Some(s) => squares.push(s),
                None => break,
            }
        }
        let line = Stroke::new(1.5, Color32::from_gray(200));
        let shade = |k: usize, s: Side| match s {
            Side::Door => Color32::from_rgb(110 - 20 * k as u8, 70 - 12 * k as u8, 30),
            _ => Color32::from_gray(90 - 18 * k as u8),
        };
        for (k, &sq) in squares.iter().enumerate().rev() {
            let (near, far) = (at(k), at(k + 1));
            let front = side(f, sq, p.facing);
            if front != Side::Open {
                paint.rect(far, 0.0, shade(k + 1, front), line, egui::StrokeKind::Middle);
            }
            for (dir, x0, x1) in [((p.facing + 3) % 4, near.left(), far.left()), ((p.facing + 1) % 4, near.right(), far.right())] {
                let s = side(f, sq, dir);
                if s == Side::Open {
                    // A side passage: show the far wall of the opening.
                    let r = Rect::from_x_y_ranges(x0.min(x1)..=x0.max(x1), far.y_range());
                    paint.rect(r, 0.0, shade(k + 1, Side::Wall), line, egui::StrokeKind::Middle);
                } else {
                    let pts = vec![pos2(x0, near.top()), pos2(x1, far.top()), pos2(x1, far.bottom()), pos2(x0, near.bottom())];
                    paint.add(egui::Shape::convex_polygon(pts, shade(k, s), line));
                }
            }
        }
        if let (Some(fight), Some(tex)) = (&g.fight, &self.mons) {
            let n = fight.foes.iter().filter(|f| f.hp > 0).count().max(1) as f32;
            let w = (rect.width() / n).min(140.0);
            for (i, foe) in fight.foes.iter().filter(|f| f.hp > 0).enumerate() {
                let m = &d.monsters[foe.monster];
                let tile = (m.picture.max(1) - 1) as f32;
                let uv = Rect::from_min_size(pos2((tile % 10.0) * 0.1, (tile / 10.0).floor() * 60.0 / 540.0), vec2(0.1, 60.0 / 540.0));
                let x = rect.center().x - w * n / 2.0 + w * i as f32;
                let r = Rect::from_min_size(pos2(x + 6.0, rect.bottom() - w - 20.0), vec2(w - 12.0, w - 12.0));
                paint.image(tex.id(), r, uv, Color32::WHITE);
                paint.text(pos2(r.center().x, r.bottom() + 8.0), egui::Align2::CENTER_CENTER, &m.name, egui::FontId::proportional(13.0), Color32::WHITE);
            }
        }
    }

    /// Seen squares of the current floor, north up.
    fn automap(ui: &mut egui::Ui, d: &GameData, g: &Game) {
        let p = g.ch.pos;
        let Some(f) = (p.floor > 0).then(|| &d.floors[p.floor - 1]) else { return };
        let Some(seen) = g.ch.seen.get(p.floor - 1) else { return };
        ui.label(format!("Automap (level {})", p.floor));
        let cell = (ui.available_width().min(300.0) / f.width as f32).floor();
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
}

fn side(f: &ddj_rules::mdr::Floor, (x, y): (usize, usize), dir: u8) -> Side {
    game::side(f, x, y, dir)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let tuning = match std::mem::replace(&mut self.tuning, Err(String::new())) {
            Ok(t) => t,
            Err(e) => {
                egui::CentralPanel::default().show(ui, |ui| ui.colored_label(Color32::RED, format!("Bad patch file: {e}")));
                self.tuning = Err(e);
                return;
            }
        };
        let data = self.data.take();
        if self.mons.is_none()
            && data.is_some()
            && let Some((w, h, px)) = fs::read(ddj_extract::data_dir().join("MG11.DLL")).ok().and_then(|b| ddj_extract::mg_bitmap(&b, "MONS"))
        {
            self.mons = Some(ui.ctx().load_texture("mons", egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::NEAREST));
        }
        let playing = matches!(self.screen, Screen::Town(_) | Screen::Dungeon);
        if let (true, Some(d), Some(g)) = (playing, &data, &self.game) {
            egui::Panel::left("sheet").resizable(false).show(ui, |ui| Self::sheet(ui, g, d, &tuning));
            egui::Panel::bottom("log").resizable(true).default_size(140.0).show(ui, |ui| {
                ui.label(&self.status);
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for l in g.log.iter().rev().take(200).rev().filter(|l| !l.is_empty()) {
                        ui.label(l);
                    }
                });
            });
        }
        if let (Screen::Dungeon, Some(d)) = (&self.screen, &data) {
            egui::Panel::right("map").resizable(false).show(ui, |ui| {
                Self::automap(ui, d, self.game.as_ref().unwrap());
                ui.separator();
                self.inventory(ui, d, false);
            });
        }
        egui::CentralPanel::default().show(ui, |ui| match (&self.screen, &data) {
            (Screen::Import, _) | (_, None) => self.import(ui),
            (Screen::Menu, Some(d)) => self.menu(ui, d),
            (Screen::Create(_), Some(d)) => self.create(ui, d),
            (Screen::Town(tab), Some(d)) => self.town(ui, d, &tuning, *tab),
            (Screen::Dungeon, Some(d)) => self.dungeon(ui, d, &tuning),
        });
        if !playing {
            egui::Panel::bottom("status").show(ui, |ui| ui.label(&self.status));
        }
        self.data = data;
        self.tuning = Ok(tuning);
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Depths of Dejenol remake",
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]), ..Default::default() },
        Box::new(|_| Ok(Box::new(App::new()))),
    )
}
