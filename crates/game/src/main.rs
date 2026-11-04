#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, path::PathBuf, time::Instant};

use ddj_rules::{
    Tuning,
    game::{self, Character, Game, Life},
    magic,
    mdr::GameData,
};
use eframe::egui::{self, Color32, Key, Rect, pos2, vec2};

mod art;
mod audio;
mod dungeon;
mod party;
mod town;
use art::Art;

enum Screen {
    Import,
    Menu,
    Create(Draft),
    Town(town::Tab),
    Dungeon,
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
    /// The original pictures, once imported.
    art: Option<Art>,
    styled: bool,
    import_path: String,
    status: String,
    screen: Screen,
    game: Option<Game>,
    paused: bool,
    in_fight: bool,
    last_round: Instant,
    /// The party member shown on the sheet and in the item and spell lists.
    sel: usize,
    /// The party member whose macros are open in the editor.
    editing: Option<usize>,
    rule: party::RuleDraft,
    seer_query: String,
    amount: String,
    audio: Option<audio::Audio>,
    /// Log lines already given their sound.
    heard: usize,
}

fn saves() -> PathBuf {
    ddj_extract::home().join("saves")
}

/// Player patches: every `patches/*.toml` except the built-in `modern.toml`, by name.
fn load_tuning() -> Result<Tuning, String> {
    let mut paths: Vec<_> = fs::read_dir(ddj_extract::home().join("patches"))
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "toml") && !p.ends_with("modern.toml"));
    paths.sort();
    let texts: Vec<String> = paths.iter().filter_map(|p| fs::read_to_string(p).ok()).collect();
    Tuning::load(&texts.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| e.to_string())
}

fn load(name: &str) -> std::io::Result<Character> {
    Character::load(&saves().join(format!("{name}.json")))
}

impl App {
    fn new() -> Self {
        // A path on the command line ("Open with" on the game's ZIP) is imported first.
        if let Some(src) = std::env::args_os().nth(1) {
            let _ = ddj_extract::import(src.as_ref(), &ddj_extract::data_dir());
        }
        let data = GameData::load(&ddj_extract::data_dir()).ok();
        let screen = if data.is_some() { Screen::Menu } else { Screen::Import };
        Self {
            tuning: load_tuning(),
            data,
            art: None,
            styled: false,
            import_path: String::new(),
            status: String::new(),
            screen,
            game: None,
            paused: false,
            in_fight: false,
            last_round: Instant::now(),
            sel: 0,
            editing: None,
            rule: party::RuleDraft::default(),
            seer_query: String::new(),
            amount: String::new(),
            audio: audio::Audio::new(),
            heard: 0,
        }
    }

    /// Saves every party member after each change; the files are small.
    fn persist(&mut self) {
        let Some(g) = &self.game else { return };
        for m in &g.party {
            if let Err(e) = m.save(&saves()) {
                self.status = format!("Could not save {}: {e}", m.name);
            }
        }
    }

    fn import(&mut self, ui: &mut egui::Ui) {
        ui.heading("Depths of Dejenol remake");
        ui.label("This remake needs your own copy of Mordor: The Depths of Dejenol. Drop its ZIP, MORDOR.WIP or install folder on this window, or type the path:");
        ui.label("(The free shareware MORDOR11.ZIP gives floors 1-3; the full game all 15.)");
        let dropped = ui.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()));
        if let Some(p) = &dropped {
            self.import_path = p.display().to_string();
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.import_path).desired_width(480.0));
            if ui.button("Import").clicked() || dropped.is_some() {
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
        if let Some(a) = &self.art {
            art::tile(ui.painter(), &a.stucco, ui.max_rect());
        }
        if let Some(menu) = self.art.as_ref().and_then(|a| a.menu.as_ref()) {
            let r = Rect::from_center_size(ui.max_rect().center(), vec2(600.0, 400.0));
            ui.painter().image(menu.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_center_size(r.center(), vec2(240.0, 380.0))), |ui| self.menu_buttons(ui, d));
        } else {
            self.menu_buttons(ui, d);
        }
    }

    fn menu_buttons(&mut self, ui: &mut egui::Ui, d: &GameData) {
        ui.vertical_centered(|ui| {
            ui.heading(egui::RichText::new("MORDOR").size(32.0).color(Color32::from_rgb(0, 0, 128)));
            ui.label(egui::RichText::new("The Depths of Dejenol").italics().color(Color32::from_rgb(128, 0, 0)));
            ui.label(format!("{} dungeon levels imported", d.floors.len()));
            ui.add_space(8.0);
            if ui.button("Create a character").clicked() {
                let stats = game::start_stats(d, 0);
                self.screen = Screen::Create(Draft { name: String::new(), race: 0, female: false, alignment: 1, stats });
            }
            if let Some(a) = self.audio.as_mut() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut a.music_on, "Music");
                    ui.checkbox(&mut a.sound_on, "Sound");
                });
            }
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for name in game::saved(&saves()) {
                    if ui.button(format!("Play {name}")).clicked() {
                        match load(&name) {
                            Ok(ch) => self.start(ch, d),
                            Err(e) => self.status = format!("Could not load {name}: {e}"),
                        }
                    }
                }
            });
        });
    }

    /// Starts play with a character, bringing its allies along when they can come.
    fn start(&mut self, ch: Character, d: &GameData) {
        let allies = ch.allies.clone();
        let mut g = Game::new(ch);
        for name in allies {
            if let Ok(a) = load(&name)
                && let Err(e) = g.add_ally(a)
            {
                self.status = e;
            }
        }
        g.party[0].allies = g.party[1..].iter().map(|m| m.name.clone()).collect();
        if let Ok(t) = load_tuning() {
            // Loading a game starts everyone in the city, rested (the dungeon repopulates).
            for m in g.party.iter_mut().filter(|m| m.alive()) {
                m.pos = game::Pos::default();
                m.hp = m.max_hp;
                m.sp = m.max_sp(d, &t);
            }
        }
        self.game = Some(g);
        self.heard = 0;
        self.sel = 0;
        self.screen = Screen::Town(town::Tab::Guilds);
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
            if d.races[dr.race].alignments & (1 << dr.alignment) == 0 {
                dr.alignment = (0..3).find(|a| d.races[dr.race].alignments & (1 << a) != 0).unwrap_or(1);
            }
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
        let create = create || (ui.input(|i| i.key_pressed(Key::Enter)) && !dr.name.trim().is_empty());
        if back {
            self.screen = Screen::Menu;
        } else if create && game::saved(&saves()).contains(&dr.name.trim().to_string()) {
            self.status = "That name is taken.".into();
        } else if create {
            match Character::new(d, &dr.name, dr.race, dr.female, dr.alignment, dr.stats) {
                Ok(ch) => {
                    self.start(ch, d);
                    self.screen = Screen::Town(town::Tab::Store);
                    self.status = "Welcome to Mordor! The store has the standard starting kit.".into();
                    self.persist();
                }
                Err(e) => self.status = e,
            }
        }
    }

    fn sheet(ui: &mut egui::Ui, c: &Character, d: &GameData, t: &Tuning) {
        ui.heading(&c.name);
        let sex = if c.female { "Female" } else { "Male" };
        let align = ["Good", "Neutral", "Evil"][c.alignment as usize];
        ui.label(format!("{sex} {} ({align}), age {}", d.races[c.race].name, c.age()));
        ui.label(format!("{} level {}", d.guilds[c.guild].name, c.level()));
        let (a, df) = c.att_def(d, t);
        egui::Grid::new("sheet").show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: String| {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            };
            row(ui, "Hits", format!("{}/{}", c.hp, c.max_hp));
            row(ui, "Spells", format!("{}/{}", c.sp, c.max_sp(d, t)));
            row(ui, "Att/Def", format!("{a}/{df}"));
            row(ui, "Experience", format!("{}/{}", c.xp[c.guild], c.xp_needed(d, t)));
            row(ui, "Gold", c.gold.to_string());
            row(ui, "Bank", c.bank.to_string());
            row(ui, "Kills", c.kills.to_string());
            for i in 0..6 {
                row(ui, game::STATS[i], c.stats[i].to_string());
            }
        });
        let mut notes = Vec::new();
        let st = &c.status;
        for (on, s) in [(st.poison > 0, "Poisoned"), (st.disease > 0, "Diseased"), (st.paralysis > 0, "Paralysed"), (st.stoned, "Stoned")] {
            if on {
                notes.push(s.to_string());
            }
        }
        if c.buffs.xp_mult > 1.0 {
            notes.push(format!("Learning {}x", c.buffs.xp_mult));
        }
        if c.buffs.levitate {
            notes.push("Levitating".into());
        }
        if c.buffs.protection > 0 {
            notes.push(format!("Protected +{}", c.buffs.protection));
        }
        if let Some(m) = c.quest {
            notes.push(format!("Quest: slay a {}", d.monsters[m].name));
        }
        match c.life {
            Life::Dead(p) => notes.push(format!("Dead on level {} at {},{}", p.floor, p.x + 1, p.y + 1)),
            Life::InMorgue => notes.push("Dead, in the morgue".into()),
            Life::Alive => {}
        }
        for n in notes {
            ui.colored_label(Color32::from_rgb(128, 0, 0), n);
        }
        ui.separator();
        egui::CollapsingHeader::new("Guild abilities").show(ui, |ui| {
            for (i, name) in game::ABILITIES.iter().enumerate() {
                ui.label(format!("{name}: {:.0}", c.ability(d, i)));
            }
        });
    }

    /// The selected member's items: Equip, Use, and in the store Sell, Identify and
    /// Uncurse.
    fn inventory(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning, store: bool) {
        let Some(g) = self.game.as_mut() else { return };
        let sel = self.sel.min(g.party.len() - 1);
        let c = &g.party[sel];
        ui.label(format!("{}'s items ({}/40)", c.name, c.items.len()));
        #[derive(Clone, Copy)]
        enum Do {
            Equip,
            Use,
            Sell,
            Identify,
            Uncurse,
        }
        let mut act = None;
        egui::ScrollArea::vertical().id_salt("items").max_height(240.0).show(ui, |ui| {
            for (i, h) in c.items.iter().enumerate() {
                let it = &d.items[h.item];
                ui.horizontal(|ui| {
                    if magic::consumable(d, h.item) {
                        if ui.button("Use").clicked() {
                            act = Some((i, Do::Use));
                        }
                    } else if ui.button(if h.equipped { "Unequip" } else { "Equip" }).clicked() {
                        act = Some((i, Do::Equip));
                    }
                    if store {
                        if ui.button(format!("Sell {}", c.sell_price(d, h.item))).clicked() {
                            act = Some((i, Do::Sell));
                        }
                        if !h.known && ui.button(format!("ID {}", Character::identify_cost(d, h.item))).clicked() {
                            act = Some((i, Do::Identify));
                        }
                        if h.equipped && magic::cursed(d, h.item, c.alignment) && ui.button("Uncurse").clicked() {
                            act = Some((i, Do::Uncurse));
                        }
                    }
                    let ad = if h.known { format!(" [{}/{}]", it.attack, it.defense) } else { String::new() };
                    ui.label(format!("{}{}{ad}", if h.equipped { "* " } else { "" }, magic::item_name(d, h)));
                });
            }
        });
        let Some((i, what)) = act else { return };
        let c = &mut g.party[sel];
        self.status = match what {
            Do::Equip => c.toggle_equip(d, i).err().unwrap_or_default(),
            Do::Sell => c.sell(d, i),
            Do::Identify => c.identify(d, i).unwrap_or_else(|e| e),
            Do::Uncurse => c.uncurse(d, i).unwrap_or_else(|e| e),
            Do::Use => {
                let item = c.items[i].item;
                magic::use_item(g, d, t, sel, item);
                String::new()
            }
        };
        self.persist();
    }

    /// The selected member's spells, castable outside combat (heals, cures, travel);
    /// in combat, choosing one sets the controlled character's order.
    fn spells(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning) {
        let Some(g) = self.game.as_mut() else { return };
        let sel = self.sel.min(g.party.len() - 1);
        let c = &g.party[sel];
        let known = magic::known(c, d);
        ui.label(format!("{}'s spells ({} SP)", c.name, c.sp));
        let mut cast = None;
        egui::ScrollArea::vertical().id_salt("spells").max_height(240.0).show(ui, |ui| {
            for s in known {
                let sp = &d.spells[s];
                let ok = magic::castable(c, d, t, s);
                ui.horizontal(|ui| {
                    if ui.add_enabled(ok, egui::Button::new("Cast")).clicked() {
                        cast = Some(s);
                    }
                    ui.label(format!("{} ({} SP, {}/{})", sp.name, sp.sp, sp.base, sp.per_level));
                });
            }
        });
        let Some(s) = cast else { return };
        let target = if magic::kind(&d.spells[s]).is_some_and(|k| matches!(k, magic::Magic::Damage | magic::Magic::MassDamage | magic::Magic::Sleep)) { ddj_rules::ai::Target::Strongest } else { ddj_rules::ai::Target::LowestAlly };
        if let (Some(f), 0) = (g.fight.as_mut(), sel) {
            f.order = Some(ddj_rules::ai::Act::Cast(s, target));
            self.status = format!("You will cast {} next round.", d.spells[s].name);
        } else if g.fight.is_some() {
            self.status = "Allies cast by their macros in combat.".into();
        } else {
            magic::cast(g, d, t, sel, s, target);
            self.persist();
        }
    }
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
        if !self.styled {
            art::style(ui.ctx());
            self.styled = true;
        }
        if self.art.is_none() && data.is_some() {
            self.art = Art::load(ui.ctx(), &ddj_extract::data_dir());
        }
        let playing = matches!(self.screen, Screen::Town(_) | Screen::Dungeon);
        if let (true, Some(d)) = (playing, &data) {
            let g = self.game.as_ref().unwrap();
            self.sel = self.sel.min(g.party.len() - 1);
            egui::Panel::left("sheet").resizable(false).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| Self::sheet(ui, &self.game.as_ref().unwrap().party[self.sel], d, &tuning));
            });
            egui::Panel::bottom("party").resizable(true).default_size(190.0).show(ui, |ui| {
                ui.columns(2, |cols| {
                    self.party_panel(&mut cols[0], d, &tuning);
                    cols[1].label(&self.status);
                    let g = self.game.as_ref().unwrap();
                    egui::ScrollArea::vertical().id_salt("log").stick_to_bottom(true).show(&mut cols[1], |ui| {
                        for l in g.log.iter().rev().take(200).rev().filter(|l| !l.is_empty()) {
                            ui.label(l);
                        }
                    });
                });
            });
        }
        if let (Screen::Dungeon, Some(d)) = (&self.screen, &data) {
            egui::Panel::right("map").resizable(false).default_size(300.0).show(ui, |ui| {
                dungeon::automap(ui, d, self.game.as_ref().unwrap());
                ui.separator();
                self.inventory(ui, d, &tuning, false);
                ui.separator();
                self.spells(ui, d, &tuning);
            });
        }
        if !playing {
            egui::Panel::bottom("status").show(ui, |ui| ui.label(&self.status));
        }
        egui::CentralPanel::default().show(ui, |ui| match (&self.screen, &data) {
            (Screen::Import, _) | (_, None) => self.import(ui),
            (Screen::Menu, Some(d)) => self.menu(ui, d),
            (Screen::Create(_), Some(d)) => self.create(ui, d),
            (Screen::Town(tab), Some(d)) => self.town(ui, d, &tuning, *tab),
            (Screen::Dungeon, Some(d)) => self.dungeon(ui, d, &tuning),
        });
        if let (Some(i), Some(d)) = (self.editing, &data) {
            self.macro_editor(ui.ctx(), d, i);
        }
        // The original's Sound Canvas music for the menu, the city and the dungeon, and
        // its effects for what just happened (at most a few a frame).
        if let (Some(a), Some(_)) = (self.audio.as_mut(), &data) {
            let dir = ddj_extract::data_dir();
            let track = match self.screen {
                Screen::Town(_) => "CITYSC.MID",
                Screen::Dungeon => "DUNGEOSC.MID",
                _ => "MAINSC.MID",
            };
            a.music(&dir, track);
            if let Some(g) = &self.game {
                self.heard = self.heard.min(g.log.len());
                for w in g.log[self.heard..].iter().filter_map(|l| audio::effect_for(l)).take(3) {
                    a.effect(&dir, w);
                }
                self.heard = g.log.len();
            }
        }
        // A first import sets the data during the frame; keep it.
        self.data = self.data.take().or(data);
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
