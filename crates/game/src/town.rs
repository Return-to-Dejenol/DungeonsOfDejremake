//! The city: the original's eight buildings and their services.

use ddj_rules::{
    Tuning,
    game::{self, Character, Life},
    monsters,
    mdr::GameData,
};
use eframe::egui::{self, Color32, Key, Stroke, vec2};

use crate::{App, Screen, art, load, saves};

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Guilds,
    Store,
    Morgue,
    Bank,
    Seer,
    Confinement,
}

impl App {
    pub(crate) fn town(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning, mut tab: Tab) {
        if let Some(a) = &self.art {
            art::tile(ui.painter(), &a.stucco, ui.max_rect());
        }
        let places = [("General Store", Some(Tab::Store)), ("Morgue", Some(Tab::Morgue)), ("Guilds", Some(Tab::Guilds)), ("Dungeon", None), ("Confinement", Some(Tab::Confinement)), ("Seer", Some(Tab::Seer)), ("Bank", Some(Tab::Bank)), ("Exit", None)];
        let mut picked = None;
        egui::Grid::new("city").spacing(vec2(8.0, 4.0)).show(ui, |ui| {
            for (i, (label, _)) in places.iter().enumerate() {
                let pic = self.art.as_ref().and_then(|a| a.city.iter().find(|(l, _)| l == label));
                let clicked = ui
                    .vertical_centered(|ui| {
                        let img = pic.map(|(_, t)| ui.add(egui::Button::image(egui::Image::new(t).fit_to_exact_size(vec2(120.0, if *label == "Exit" { 42.0 } else { 87.0 })))).clicked());
                        let text = ui.button(egui::RichText::new(*label).strong()).clicked();
                        img.unwrap_or(false) || text
                    })
                    .inner;
                if clicked {
                    picked = Some(i);
                }
                if i == 3 {
                    ui.end_row();
                }
            }
        });
        // The original's mnemonics: General Store, Morgue, Guilds, Dungeon, Confinement, sEer, Bank, eXit.
        let typing = ui.ctx().egui_wants_keyboard_input();
        let keys = [Key::S, Key::M, Key::G, Key::D, Key::C, Key::E, Key::B, Key::X];
        if !typing && let Some(i) = ui.input(|inp| keys.iter().position(|&k| inp.key_pressed(k))) {
            picked = Some(i);
        }
        let g = self.game.as_mut().unwrap();
        match picked.map(|i| places[i]) {
            Some((_, Some(t))) => tab = t,
            Some(("Dungeon", _)) => match g.enter_dungeon(d) {
                Ok(()) => self.screen = Screen::Dungeon,
                Err(e) => self.status = e,
            },
            Some(("Exit", _)) => {
                self.persist();
                self.screen = Screen::Menu;
                return;
            }
            _ => {}
        }
        if let Screen::Town(_) = self.screen {
            self.screen = Screen::Town(tab);
        }
        ui.add_space(6.0);
        // Services sit in a gray window, as the original's did.
        egui::Frame::new().fill(Color32::from_gray(192)).stroke(Stroke::new(1.0, Color32::from_gray(64))).inner_margin(8.0).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let changed = match tab {
                Tab::Guilds => self.guilds(ui, d, t),
                Tab::Store => self.store(ui, d, t),
                Tab::Morgue => self.morgue(ui, d),
                Tab::Bank => self.bank(ui),
                Tab::Seer => self.seer_tab(ui, d),
                Tab::Confinement => self.confinement(ui, d),
            };
            if changed {
                self.persist();
            }
        });
    }

    fn guilds(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning) -> bool {
        let g = self.game.as_mut().unwrap();
        let c = &mut g.party[self.sel];
        let mut changed = false;
        let (need, gold) = (c.xp_needed(d, t), Tuning::level_gold(c.level(), d.guilds[c.guild].gold_factor));
        ui.label(format!("{}: the {} guild asks {need} experience and {gold} gold for level {}.", c.name, d.guilds[c.guild].name, c.level() + 1));
        ui.label(format!("{} has {} experience.", c.name, c.xp[c.guild]));
        if let Some(m) = c.quest {
            ui.label(format!("Quest: slay a {} to continue.", d.monsters[m].name));
        }
        if ui.button("Make level").clicked() {
            self.status = c.make_level(d, t).unwrap_or_else(|e| e);
            changed = true;
        }
        ui.add_space(6.0);
        egui::Grid::new("guilds").show(ui, |ui| {
            for (i, gd) in d.guilds.iter().enumerate() {
                ui.label(&gd.name);
                let lv = c.levels[i];
                ui.label(if lv > 0 { format!("level {lv}") } else { format!("joining {} gold", c.join_cost(d, i)) });
                let label = if i == c.guild { None } else if lv > 0 { Some("Act as") } else { Some("Join") };
                if let Some(l) = label
                    && ui.button(l).clicked()
                {
                    self.status = c.join(d, i).unwrap_or_else(|e| e);
                    changed = true;
                }
                ui.end_row();
            }
        });
        changed
    }

    fn store(&mut self, ui: &mut egui::Ui, d: &GameData, t: &Tuning) -> bool {
        let mut changed = false;
        ui.columns(2, |cols| {
            let g = self.game.as_mut().unwrap();
            let sel = self.sel;
            let list: Vec<(usize, u64, u8)> = g.stock(d).listed(d).into_iter().map(|i| (i, 0, g.store.count(i))).collect();
            let list: Vec<(usize, u64, u8)> = list.into_iter().map(|(i, _, n)| (i, g.buy_price(d, sel, i), n)).collect();
            cols[0].label(format!("For sale to {} [att/def] (swings), in stock", g.party[sel].name));
            egui::ScrollArea::vertical().id_salt("stock").max_height(300.0).show(&mut cols[0], |ui| {
                for (i, price, n) in list {
                    let it = &d.items[i];
                    ui.horizontal(|ui| {
                        if ui.button(format!("Buy {price}")).clicked() {
                            self.status = g.buy(d, sel, i).unwrap_or_else(|e| e);
                            changed = true;
                        }
                        let usable = it.guilds & (1 << g.party[sel].guild) != 0;
                        let stock = if n >= ddj_rules::store::UNLIMITED { String::new() } else { format!(" x{n}") };
                        let text = format!("{} [{}/{}]{}{stock}", it.name, it.attack, it.defense, if it.power > 0 { format!(" ({})", it.power) } else { String::new() });
                        ui.label(if usable { egui::RichText::new(text).strong() } else { egui::RichText::new(text).weak() });
                    });
                }
            });
            self.inventory(&mut cols[1], d, t, true);
        });
        changed
    }

    fn morgue(&mut self, ui: &mut egui::Ui, d: &GameData) -> bool {
        let g = self.game.as_mut().unwrap();
        let mut changed = false;
        let mut any = false;
        for i in 0..g.party.len() {
            let name = g.party[i].name.clone();
            match g.party[i].life {
                Life::Alive => continue,
                Life::Dead(p) => {
                    any = true;
                    if ui.button(format!("Hire rescuers for {name} on level {} ({} gold)", p.floor, g.rescue_cost(i))).clicked() {
                        g.rescue(i);
                        changed = true;
                    }
                }
                Life::InMorgue => {
                    any = true;
                    if ui.button(format!("Raise {name} ({} gold)", g.raise_cost(i))).clicked() {
                        g.raise(d, i);
                        changed = true;
                    }
                }
            }
        }
        if !any {
            ui.label("The morgue workers eye you hopefully.");
        }
        changed
    }

    fn bank(&mut self, ui: &mut egui::Ui) -> bool {
        let g = self.game.as_mut().unwrap();
        let c = &mut g.party[self.sel];
        ui.label(format!("{} has {} gold on hand and {} in the bank. City services draw on the bank when the purse runs short.", c.name, c.gold, c.bank));
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.amount);
            let n = self.amount.trim().parse::<u64>().ok();
            if ui.button("Deposit").clicked() {
                c.deposit(n.unwrap_or(0));
                changed = true;
            }
            if ui.button("Withdraw").clicked() {
                c.withdraw(n.unwrap_or(0));
                changed = true;
            }
            if ui.button("Deposit all").clicked() {
                c.deposit(c.gold);
                changed = true;
            }
        });
        if ui.button("Party: everyone deposits their gold").clicked() {
            for m in &mut g.party {
                m.deposit(m.gold);
            }
            changed = true;
        }
        changed
    }

    fn seer_tab(&mut self, ui: &mut egui::Ui, d: &GameData) -> bool {
        ui.label("The Seer locates lost characters and monsters' lairs, for 200 gold a try. Who or what do you seek?");
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.seer_query);
            if ui.button("Ask").clicked() {
                let q = self.seer_query.clone();
                self.status = self.game.as_mut().unwrap().seer(d, &q);
                changed = true;
            }
        });
        changed
    }

    fn confinement(&mut self, ui: &mut egui::Ui, d: &GameData) -> bool {
        let g = self.game.as_mut().unwrap();
        let mut changed = false;
        ui.columns(2, |cols| {
            cols[0].label("Creatures for sale");
            egui::ScrollArea::vertical().id_salt("conf").max_height(300.0).show(&mut cols[0], |ui| {
                let stock: Vec<(usize, u8)> = (0..d.monsters.len()).filter_map(|m| g.store.companions.get(m).filter(|&&n| n > 0).map(|&n| (m, n))).collect();
                for (m, n) in stock {
                    let price = monsters::companion_buy_price(d, m, n);
                    ui.horizontal(|ui| {
                        if ui.button(format!("Buy {price}")).clicked() {
                            self.status = g.buy_companion(d, m).unwrap_or_else(|e| e);
                            changed = true;
                        }
                        let mon = &d.monsters[m];
                        ui.label(format!("{} (level {}, {}/{}) x{n}", mon.name, mon.level, mon.attack, mon.defense));
                    });
                }
            });
            cols[1].label(format!("{}'s companions ({}/{})", g.party[0].name, g.party[0].companions.len(), monsters::MAX_COMPANIONS));
            let mut act = None;
            for (k, c) in g.party[0].companions.iter().enumerate() {
                cols[1].horizontal(|ui| {
                    if ui.button("Sell").clicked() {
                        act = Some((k, false));
                    }
                    if ui.button("Rebind").clicked() {
                        act = Some((k, true));
                    }
                    ui.label(format!("{} {}/{} hits, binding level {}", d.monsters[c.monster].name, c.hp, c.max_hp, c.bind));
                });
            }
            match act {
                Some((k, false)) => {
                    self.status = g.sell_companion(d, k);
                    changed = true;
                }
                Some((k, true)) => {
                    self.status = g.rebind(d, k).unwrap_or_else(|e| e);
                    changed = true;
                }
                None => {}
            }
        });
        changed
    }

    /// In town: add saved characters to the party (they must be alive and here).
    pub(crate) fn recruit(&mut self, ui: &mut egui::Ui) {
        let g = self.game.as_mut().unwrap();
        if g.party.len() >= game::MAX_PARTY {
            return;
        }
        let names: Vec<String> = game::saved(&saves()).into_iter().filter(|n| !g.party.iter().any(|m| &m.name == n)).collect();
        if names.is_empty() {
            return;
        }
        egui::ComboBox::from_id_salt("recruit").selected_text("Add an ally...").show_ui(ui, |ui| {
            for n in names {
                if ui.selectable_label(false, &n).clicked() {
                    self.status = match load(&n).map_err(|e| e.to_string()).and_then(|c: Character| g.add_ally(c)) {
                        Ok(()) => format!("{n} joins the party."),
                        Err(e) => e,
                    };
                }
            }
        });
    }
}
