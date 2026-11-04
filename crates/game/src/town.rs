//! The city (frmCityMain) and its buildings, each the original's window: the General
//! Store, Guilds, Bank, Morgue, Seer and Confinement & Holding.

use ddj_rules::{
    Tuning,
    game::Life,
    magic, monsters,
    mdr::GameData,
};
use eframe::egui::{self, Color32, Key};

use crate::{App, Deed, Dialog, Screen, win::{Desk, Drag, View}};

const SEL_BACK: Color32 = Color32::from_rgb(0, 0, 128);

/// The buildings by the city's picture index (frmCityMain's imgPicture/lblSelection).
const PLACES: [Option<&str>; 8] = [Some("frmCityStore"), Some("frmCityBank"), Some("frmCityMorgue"), Some("frmCitySeer"), Some("frmCityGuild"), None, None, Some("frmCityCStore")];

impl App {
    /// The city window: eight pictures with their captions; the original's mnemonics
    /// also work (S, B, M, R, G, D, X, C).
    pub(crate) fn town(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, _t: &Tuning) {
        let ev = desk.window(ctx, "frmCityMain", View::default(), false);
        let typing = ctx.egui_wants_keyboard_input() || self.dialog.is_some() || self.building.is_some();
        let keys = [Key::S, Key::B, Key::M, Key::R, Key::G, Key::D, Key::X, Key::C];
        let pick = ev.index("imgPicture").or(ev.index("lblSelection")).map(usize::from).or_else(|| (!typing).then(|| ctx.input(|i| keys.iter().position(|&k| i.key_pressed(k)))).flatten());
        match pick {
            Some(5) => {
                let g = self.game.as_mut().unwrap();
                match g.enter_dungeon(d) {
                    Ok(()) => {
                        self.screen = Screen::Dungeon;
                        self.building = None;
                    }
                    Err(e) => self.say(e),
                }
            }
            Some(6) => {
                self.persist();
                self.loaded = self.game.as_ref().map(|g| g.party[0].name.clone());
                self.game = None;
                self.building = None;
                self.screen = Screen::Menu;
            }
            Some(i) => self.building = PLACES[i],
            None => {}
        }
    }

    /// The open building's window.
    pub(crate) fn building(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning, f: &'static str) {
        if self.game.is_none() {
            return;
        }
        desk.pos.entry(f.into()).or_insert(desk.client.min);
        let closed = match f {
            "frmCityStore" => self.store(desk, ctx, d, t),
            "frmCityGuild" => self.guild(desk, ctx, d, t),
            "frmCityBank" => self.bank(desk, ctx),
            "frmCityMorgue" => self.morgue(desk, ctx),
            "frmCitySeer" => {
                let ev = desk.window(ctx, f, View::default(), false);
                if ev.index("cmdCommand").is_some_and(|i| i != 1) {
                    self.dialog = Some(Dialog::Seer);
                }
                ev.click("cmdCommand(1)") || ev.closed
            }
            _ => self.confinement(desk, ctx, d),
        };
        if closed {
            self.building = None;
            self.persist();
        }
    }

    /// The General Store: buy from the stock list; drop an item from Objects & Spells on
    /// the Uncurse or Identify box, then Uncurse, ID or Sell.
    fn store(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, _t: &Tuning) -> bool {
        const F: &str = "frmCityStore";
        let sel = self.sel.min(self.game.as_ref().unwrap().party.len() - 1);
        let g = self.game.as_mut().unwrap();
        let search = desk.state(F, "txtSearch").text.to_lowercase();
        let list: Vec<usize> = g.stock(d).listed(d).into_iter().filter(|&i| d.items[i].name.to_lowercase().contains(&search)).collect();
        let rows = 10usize;
        let top = desk.state(F, "vsbItems").top.min(list.len().saturating_sub(rows));
        let picked = desk.state(F, "buy").sel.filter(|&i| list.contains(&i));
        let row0 = desk.rect(F, "elaItem(0)");
        let info0 = desk.rect(F, "elaItemInfo(0)");
        let mut v = View::default();
        for r in 0..rows {
            if r > 0 {
                v.clone_rect("elaItem(0)", r as u16, [row0[0], row0[1] + row0[3] * r as i32, row0[2], row0[3]]);
                v.clone_rect("elaItemInfo(0)", r as u16, [info0[0], info0[1] + info0[3] * r as i32, info0[2], info0[3]]);
            }
            let item = list.get(top + r).copied();
            let (k, ki) = (format!("elaItem({r})"), format!("elaItemInfo({r})"));
            let usable = item.is_some_and(|i| d.items[i].guilds & (1 << g.party[sel].guild) != 0);
            v.text(&k, item.map_or(String::new(), |i| d.items[i].name.clone())).fore(&k, if usable { Color32::BLACK } else { Color32::from_gray(128) });
            let n = item.map(|i| g.store.count(i));
            let unlimited = n.is_some_and(|n| n >= ddj_rules::store::UNLIMITED);
            v.text(&ki, match (item, n) {
                (Some(i), Some(_)) if unlimited => crate::stats::commas(g.buy_price(d, sel, i)),
                (_, Some(n)) => n.to_string(),
                _ => String::new(),
            });
            if unlimited {
                v.bold(&k).bold(&ki);
            }
            if item.is_some() && item == picked {
                v.back(&k, SEL_BACK).fore(&k, Color32::WHITE);
            }
        }
        v.text("Label9(1)", "Stock");
        v.text("elaSelectedItem", picked.map_or(String::new(), |i| d.items[i].name.clone()));
        v.text("elaSelectedItemCost", picked.map_or(String::new(), |i| g.buy_price(d, sel, i).to_string()));
        v.enable("cmdBuy", picked.is_some()).enable("cmdInfo", picked.is_some());
        // The items dropped on the Uncurse and Identify boxes (slots in the member's pack).
        let unc = desk.state(F, "uncurse").sel.filter(|&s| s < g.party[sel].items.len());
        let idd = desk.state(F, "id").sel.filter(|&s| s < g.party[sel].items.len());
        v.text("txtUncurse", unc.map_or(String::new(), |s| magic::item_name(d, &g.party[sel].items[s])));
        v.text("lblUncurse", unc.map_or(String::new(), |s| ddj_rules::store::uncurse_price(d, g.party[sel].items[s].item).to_string()));
        v.enable("cmdUncurse", unc.is_some_and(|s| g.party[sel].items[s].equipped && magic::cursed(d, g.party[sel].items[s].item, g.party[sel].alignment)));
        v.text("txtID", idd.map_or(String::new(), |s| magic::item_name(d, &g.party[sel].items[s])));
        v.text("lblValue", idd.map_or(String::new(), |s| g.sell_price(d, sel, s).to_string()));
        v.text("lblIDCost", idd.map_or(String::new(), |s| if g.party[sel].items[s].known { String::new() } else { g.identify_price(d, sel, s).to_string() }));
        v.text("lblIDLevel", "");
        v.enable("cmdSell", idd.is_some()).enable("cmdID", idd.is_some_and(|s| !g.party[sel].items[s].known)).enable("cmdIDInfo", idd.is_some());
        v.text("txtCombine", "").enable("txtCombine", false);
        let ev = desk.window(ctx, F, v, false);
        if let Some(r) = ev.index("elaItem").or(ev.index("elaItemInfo")) {
            desk.state(F, "buy").sel = list.get(top + usize::from(r)).copied();
        }
        if let Some((k, Drag(1, m, slot))) = ev.dropped.clone()
            && m == sel
        {
            match k.as_str() {
                "txtUncurse" | "lblUncurse" => desk.state(F, "uncurse").sel = Some(slot),
                "txtID" | "lblValue" | "lblIDCost" | "lblIDLevel" => desk.state(F, "id").sel = Some(slot),
                _ => {}
            }
        }
        let mut msg = String::new();
        if ev.click("cmdBuy")
            && let Some(i) = picked
        {
            msg = g.buy(d, sel, i).unwrap_or_else(|e| e);
        }
        if ev.click("cmdInfo")
            && let Some(i) = picked
        {
            let it = &d.items[i];
            msg = format!("{}: attack {}, defense {}, level {}.", it.name, it.attack, it.defense, it.level);
        }
        if ev.click("cmdUncurse")
            && let Some(s) = unc
        {
            msg = g.uncurse(d, sel, s).unwrap_or_else(|e| e);
            desk.state(F, "uncurse").sel = None;
        }
        if ev.click("cmdSell")
            && let Some(s) = idd
        {
            msg = g.sell(d, sel, s);
            desk.state(F, "id").sel = None;
            desk.state(F, "uncurse").sel = None;
        }
        if ev.click("cmdID")
            && let Some(s) = idd
        {
            msg = g.identify(d, sel, s).unwrap_or_else(|e| e);
        }
        if ev.click("cmdIDInfo")
            && let Some(s) = idd
        {
            let h = &g.party[sel].items[s];
            msg = format!("{} would sell for {} gold.", magic::item_name(d, h), g.sell_price(d, sel, s));
        }
        let done = ev.click("cmdExit") || ev.closed;
        if ev.clicked.is_some() {
            self.say(msg);
            self.persist();
        }
        done
    }

    /// Guilds: visit any guild from the list; join it, or make a level in your own.
    fn guild(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) -> bool {
        const F: &str = "frmCityGuild";
        let sel = self.sel.min(self.game.as_ref().unwrap().party.len() - 1);
        let g = self.game.as_mut().unwrap();
        let c = &g.party[sel];
        let visit = desk.state(F, "visit").sel.unwrap_or(c.guild).min(d.guilds.len() - 1);
        if desk.state(F, "lstGuilds").sel.is_none() {
            desk.state(F, "lstGuilds").sel = Some(visit);
        }
        let gd = &d.guilds[visit];
        let mut v = View::default();
        v.items("lstGuilds", d.guilds.iter().map(|g| g.name.clone()).collect());
        v.text("lblWelcome", format!("Welcome to the {}'s Guild!", gd.name));
        v.text("lblGuildMaster", "");
        let s0 = desk.rect(F, "lblStat(0)");
        let n0 = desk.rect(F, "lblStatName(0)");
        for (i, n) in ["Str", "Int", "Wis", "Con", "Cha", "Dex"].iter().enumerate() {
            if i > 0 {
                let dx = 32 * i as i32;
                v.clone_rect("lblStat(0)", i as u16, [s0[0] + dx, s0[1], s0[2], s0[3]]).clone_rect("lblStatName(0)", i as u16, [n0[0] + dx, n0[1], n0[2], n0[3]]);
            }
            v.text(&format!("lblStat({i})"), gd.stat_req[i].to_string()).text(&format!("lblStatName({i})"), *n);
        }
        let lv = c.levels[visit];
        let info = if visit == c.guild {
            let (need, gold) = (c.xp_needed(d, t), ddj_rules::Tuning::level_gold(c.level(), gd.gold_factor));
            match c.quest {
                Some(m) => format!("You must slay a {} before making level {}.", d.monsters[m].name, c.level() + 1),
                None => format!("Level {} needs {need} experience and {gold} gold.", c.level() + 1),
            }
        } else if lv > 0 {
            format!("You are a level {lv} member. Join to make it your guild.")
        } else {
            format!("Joining costs {} gold.", c.join_cost(d, visit))
        };
        v.text("lblWelcomeInfo", info);
        v.enable("cmdMake", visit == c.guild).enable("cmdJoin", visit != c.guild).enable("cmdLog", false).enable("cmdLibrary", false);
        let ev = desk.window(ctx, F, v, false);
        if ev.click("cmdVisit") || ev.double.as_deref() == Some("lstGuilds") {
            desk.state(F, "visit").sel = desk.state(F, "lstGuilds").sel;
        }
        let c = &mut g.party[sel];
        let mut msg = String::new();
        if ev.click("cmdMake") {
            msg = c.make_level(d, t).unwrap_or_else(|e| e);
        }
        if ev.click("cmdJoin") {
            msg = c.join(d, visit).unwrap_or_else(|e| e);
        }
        if ev.click("cmdInfo") {
            msg = format!("{} has {} experience; level {} needs {}.", c.name, c.xp[c.guild], c.level() + 1, c.xp_needed(d, t));
        }
        let done = ev.click("cmdExit") || ev.closed;
        if ev.clicked.is_some() {
            if done {
                desk.st.retain(|k, _| !k.starts_with("frmCityGuild."));
            }
            self.say(msg);
            self.persist();
        }
        done
    }

    /// The Bank: type an amount and press Enter (or All) to deposit or withdraw; the
    /// party buttons pool and deposit everyone's gold.
    fn bank(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        const F: &str = "frmCityBank";
        let sel = self.sel.min(self.game.as_ref().unwrap().party.len() - 1);
        let g = self.game.as_mut().unwrap();
        let c = &g.party[sel];
        let mut v = View::default();
        v.text("lblStatus", format!("You have {} gold on hand and {} in the bank.", c.gold, c.bank)).items("lstItems", Vec::new()).enable("cmdInfo", false);
        let ev = desk.window(ctx, F, v, false);
        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
        let amount = |desk: &mut Desk, k: &str| desk.state(F, k).text.trim().replace(',', "").parse::<u64>().ok();
        let c = &mut g.party[sel];
        if ev.click("cmdCommand(0)") || (enter && amount(desk, "txtAmount(0)").is_some()) {
            let n = if ev.click("cmdCommand(0)") { c.gold } else { amount(desk, "txtAmount(0)").unwrap_or(0) };
            c.deposit(n);
            desk.state(F, "txtAmount(0)").text.clear();
        }
        if ev.click("cmdCommand(1)") || (enter && amount(desk, "txtAmount(1)").is_some()) {
            let n = if ev.click("cmdCommand(1)") { c.bank } else { amount(desk, "txtAmount(1)").unwrap_or(0) };
            c.withdraw(n);
            desk.state(F, "txtAmount(1)").text.clear();
        }
        match ev.index("cmdCommand") {
            // Pool & Deposit: the party's gold to this character's account.
            Some(2) | Some(4) => {
                let pooled: u64 = g.party.iter_mut().enumerate().filter(|(i, _)| *i != sel).map(|(_, m)| std::mem::take(&mut m.gold)).sum();
                g.party[sel].gold += pooled;
                let gold = g.party[sel].gold;
                g.party[sel].deposit(gold);
            }
            // Party Deposit: everyone deposits their own.
            Some(3) => {
                for m in &mut g.party {
                    m.deposit(m.gold);
                }
            }
            _ => {}
        }
        if ev.clicked.is_some() || enter {
            self.persist();
        }
        ev.click("cmdExit") || ev.closed
    }

    /// The Morgue: raise a member brought back, or hire rescuers for one lost below.
    fn morgue(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let g = self.game.as_mut().unwrap();
        let in_morgue = (0..g.party.len()).find(|&i| g.party[i].life == Life::InMorgue);
        let lost = (0..g.party.len()).find(|&i| matches!(g.party[i].life, Life::Dead(_)));
        let mut v = View::default();
        v.enable("cmdCommand(0)", in_morgue.is_some()).enable("cmdCommand(1)", false).enable("cmdCommand(3)", lost.is_some());
        let ev = desk.window(ctx, "frmCityMorgue", v, false);
        if ev.click("cmdCommand(0)")
            && let Some(i) = in_morgue
        {
            self.dialog = Some(Dialog::Confirm(format!("Raise \"{}\" for {} gold?", g.party[i].name, g.raise_cost(i)), Deed::Raise(i)));
        }
        if ev.click("cmdCommand(3)")
            && let Some(i) = lost
        {
            self.dialog = Some(Dialog::Confirm(format!("Hire rescuers to bring back \"{}\" for {} gold?", g.party[i].name, g.rescue_cost(i)), Deed::Rescue(i)));
        }
        ev.click("cmdExit") || ev.closed
    }

    /// Confinement & Holding: buy a creature from the list; drop a companion from Objects
    /// & Spells on the Bind box to rebind it, or on the Sell box to sell it.
    fn confinement(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        const F: &str = "frmCityCStore";
        let g = self.game.as_mut().unwrap();
        let search = desk.state(F, "txtSearch").text.to_lowercase();
        let list: Vec<(usize, u8)> = (0..d.monsters.len()).filter_map(|m| g.store.companions.get(m).filter(|&&n| n > 0).map(|&n| (m, n))).filter(|(m, _)| d.monsters[*m].name.to_lowercase().contains(&search)).collect();
        let rows = 9usize;
        let top = desk.state(F, "vsbComps").top.min(list.len().saturating_sub(rows));
        let picked = desk.state(F, "buy").sel.and_then(|m| list.iter().find(|x| x.0 == m).copied());
        let (row0, info0) = (desk.rect(F, "elaComp(0)"), desk.rect(F, "elaCompInfo(0)"));
        let mut v = View::default();
        for r in 0..rows {
            if r > 0 {
                v.clone_rect("elaComp(0)", r as u16, [row0[0], row0[1] + row0[3] * r as i32, row0[2], row0[3]]);
                v.clone_rect("elaCompInfo(0)", r as u16, [info0[0], info0[1] + info0[3] * r as i32, info0[2], info0[3]]);
            }
            let it = list.get(top + r);
            let k = format!("elaComp({r})");
            v.text(&k, it.map_or(String::new(), |(m, _)| d.monsters[*m].name.clone())).text(&format!("elaCompInfo({r})"), it.map_or(String::new(), |(_, n)| n.to_string()));
            if it.is_some() && it.map(|x| x.0) == picked.map(|x| x.0) {
                v.back(&k, SEL_BACK).fore(&k, Color32::WHITE);
            }
        }
        v.text("lblLabel(4)", "Stock");
        v.text("lblSelectedComp", picked.map_or(String::new(), |(m, _)| d.monsters[m].name.clone()));
        v.text("lblSelectedCompCost", picked.map_or(String::new(), |(m, n)| monsters::companion_buy_price(d, m, n).to_string()));
        v.enable("cmdBuy", picked.is_some()).enable("cmdInfo", picked.is_some());
        let comps = &g.party[0].companions;
        let bind = desk.state(F, "bind").sel.filter(|&k| k < comps.len());
        let sell = desk.state(F, "sell").sel.filter(|&k| k < comps.len());
        v.text("txtBind", bind.map_or(String::new(), |k| d.monsters[comps[k].monster].name.clone())).enable("cmdBind", bind.is_some()).text("lblBindCost", "");
        v.text("txtID", sell.map_or(String::new(), |k| d.monsters[comps[k].monster].name.clone())).enable("cmdSell", sell.is_some());
        v.text("lblValue", "").text("lblIDCost", "").text("lblIDLevel", "");
        let ev = desk.window(ctx, F, v, false);
        if let Some(r) = ev.index("elaComp").or(ev.index("elaCompInfo")) {
            desk.state(F, "buy").sel = list.get(top + usize::from(r)).map(|x| x.0);
        }
        if let Some((k, Drag(2, 0, comp))) = ev.dropped.clone() {
            match k.as_str() {
                "txtBind" | "lblBindCost" => desk.state(F, "bind").sel = Some(comp),
                "txtID" | "lblValue" => desk.state(F, "sell").sel = Some(comp),
                _ => {}
            }
        }
        let mut msg = String::new();
        if ev.click("cmdBuy")
            && let Some((m, _)) = picked
        {
            msg = g.buy_companion(d, m).unwrap_or_else(|e| e);
        }
        if ev.click("cmdInfo")
            && let Some((m, _)) = picked
        {
            let mon = &d.monsters[m];
            msg = format!("{}: level {}, attack {}, defense {}.", mon.name, mon.level, mon.attack, mon.defense);
        }
        if ev.click("cmdBind")
            && let Some(k) = bind
        {
            msg = g.rebind(d, k).unwrap_or_else(|e| e);
        }
        if ev.click("cmdSell")
            && let Some(k) = sell
        {
            msg = g.sell_companion(d, k);
            desk.state(F, "sell").sel = None;
            desk.state(F, "bind").sel = None;
        }
        if ev.clicked.is_some() {
            self.say(msg);
            self.persist();
        }
        ev.click("cmdExit") || ev.closed
    }
}
