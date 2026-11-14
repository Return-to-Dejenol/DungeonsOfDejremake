//! The city (frmCityMain) and its buildings, each the original's window: the General
//! Store, Guilds, Bank, Morgue, Seer and Confinement & Holding.

use ddj_rules::{
    Tuning,
    game::{CONGRATULATIONS, Life},
    magic, monsters,
    mdr::GameData,
};
use eframe::egui::{self, Color32, Key};

use crate::{App, Deed, Dialog, Screen, stats::commas, win::{Desk, Drag, View}};

const SEL_BACK: Color32 = Color32::from_rgb(0, 0, 128);

/// A service's answer: its Messages line, or a message box queued (title, text).
fn answer(boxes: &mut Vec<(String, String)>, d: &GameData, r: Result<String, (u32, String)>) -> String {
    r.unwrap_or_else(|(title, text)| {
        boxes.push((d.t(title).into(), text));
        String::new()
    })
}

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
            "frmCityBank" => self.bank(desk, ctx, d),
            "frmCityMorgue" => self.morgue(desk, ctx, d),
            "frmCitySeer" => {
                let ev = desk.window(ctx, f, View::default(), false);
                if let Some(i) = ev.index("cmdCommand").filter(|&i| i != 1) {
                    self.dialog = Some(Dialog::Seer(i));
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
        // The alignment tabs (cmdAlign: none, good, neutral, evil) pick the shelf.
        let al = desk.state(F, "align").sel.unwrap_or(0).min(3) as u8;
        let list: Vec<usize> = g.stock(d).listed(d, al).into_iter().filter(|&i| d.items[i].name.to_lowercase().contains(&search)).collect();
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
            v.scroll(&k, "vsbItems", row0[3] as f32, list.len().saturating_sub(rows)).scroll(&ki, "vsbItems", row0[3] as f32, list.len().saturating_sub(rows));
            let usable = item.is_some_and(|i| d.items[i].guilds & (1 << g.party[sel].guild) != 0);
            v.text(&k, item.map_or(String::new(), |i| d.items[i].name.clone())).fore(&k, if usable { Color32::BLACK } else { Color32::from_gray(128) });
            let n = item.map(|i| g.store.count(i, al));
            let unlimited = n.is_some_and(|n| n >= ddj_rules::store::UNLIMITED);
            v.text(&ki, match (item, n) {
                (Some(i), Some(_)) if unlimited => crate::stats::commas(g.buy_price(d, sel, i, al)),
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
        v.text("elaSelectedItemCost", picked.map_or(String::new(), |i| g.buy_price(d, sel, i, al).to_string()));
        v.enable("cmdBuy", picked.is_some()).enable("cmdInfo", picked.is_some());
        // The items dropped on the Uncurse and Identify boxes (slots in the member's pack).
        let unc = desk.state(F, "uncurse").sel.filter(|&s| s < g.party[sel].items.len());
        let idd = desk.state(F, "id").sel.filter(|&s| s < g.party[sel].items.len());
        v.text("txtUncurse", unc.map_or(String::new(), |s| magic::item_name(d, &g.party[sel].items[s])));
        v.text("lblUncurse", unc.map_or(String::new(), |s| ddj_rules::store::uncurse_price(d, g.party[sel].items[s].item).to_string()));
        v.enable("cmdUncurse", unc.is_some_and(|s| g.party[sel].items[s].equipped && magic::cursed(d, &g.party[sel].items[s], g.party[sel].alignment)));
        v.text("txtID", idd.map_or(String::new(), |s| magic::item_name(d, &g.party[sel].items[s])));
        v.text("lblValue", idd.map_or(String::new(), |s| g.sell_price(d, sel, s).to_string()));
        v.text("lblIDCost", idd.map_or(String::new(), |s| if g.party[sel].items[s].known() { String::new() } else { g.identify_price(d, sel, s).to_string() }));
        v.text("lblIDLevel", "");
        v.enable("cmdSell", idd.is_some()).enable("cmdID", idd.is_some_and(|s| !g.party[sel].items[s].known())).enable("cmdIDInfo", idd.is_some());
        // Combine: an item dropped here gathers the charges of its fellows (sub2621).
        let comb = desk.state(F, "combine").sel.filter(|&s| s < g.party[sel].items.len());
        v.text("txtCombine", comb.map_or(String::new(), |s| magic::item_name(d, &g.party[sel].items[s]))).enable("cmdCommand(0)", comb.is_some());
        let ev = desk.window(ctx, F, v, false);
        if let Some(a) = ev.index("cmdAlign") {
            desk.state(F, "align").sel = Some(usize::from(a));
        }
        if let Some(r) = ev.index("elaItem").or(ev.index("elaItemInfo")) {
            desk.state(F, "buy").sel = list.get(top + usize::from(r)).copied();
        }
        if let Some((k, Drag(1, m, slot))) = ev.dropped.clone()
            && m == sel
        {
            match k.as_str() {
                "txtUncurse" | "lblUncurse" => desk.state(F, "uncurse").sel = Some(slot),
                "txtID" | "lblValue" | "lblIDCost" | "lblIDLevel" => desk.state(F, "id").sel = Some(slot),
                "txtCombine" | "lblCombine" => desk.state(F, "combine").sel = Some(slot),
                _ => {}
            }
        }
        let mut msg = String::new();
        if ev.click("cmdBuy")
            && let Some(i) = picked
        {
            msg = answer(&mut self.boxes, d, g.buy(d, sel, i, al));
        }
        if let Some(a) = ev.index("cmdRealign")
            && let Some(s) = idd
        {
            msg = answer(&mut self.boxes, d, g.realign(d, sel, s, a as u8 + 1));
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
            msg = answer(&mut self.boxes, d, g.uncurse(d, sel, s));
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
            msg = answer(&mut self.boxes, d, g.identify(d, sel, s));
        }
        if ev.click("cmdCommand(0)")
            && let Some(s) = comb
        {
            match g.party[sel].combine(d, s) {
                Ok(m) => msg = m,
                Err(e) => self.boxes.push((String::new(), e)),
            }
            desk.state(F, "combine").sel = None;
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

    /// Guilds (frmCityGuild): those that take the character's race and alignment, with
    /// its levels in them; join one, or make a level in your own. The words are the
    /// original's (ddj_extract::text).
    fn guild(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) -> bool {
        const F: &str = "frmCityGuild";
        let sel = self.sel.min(self.game.as_ref().unwrap().party.len() - 1);
        let g = self.game.as_mut().unwrap();
        let c = &g.party[sel];
        let takes = |k: usize| d.guilds[k].races & (1 << c.race) != 0 && d.guilds[k].alignments & (1 << c.alignment) != 0;
        let shown: Vec<usize> = (0..d.guilds.len()).filter(|&k| takes(k)).collect();
        let visit = desk.state(F, "visit").sel.unwrap_or(c.guild).min(d.guilds.len() - 1);
        if desk.state(F, "lstGuilds").sel.is_none() {
            desk.state(F, "lstGuilds").sel = shown.iter().position(|&k| k == visit);
        }
        let gd = &d.guilds[visit];
        let mut v = View::default();
        v.items("lstGuilds", shown.iter().map(|&k| if c.levels[k] > 0 { format!("{} ({})", d.guilds[k].name, c.levels[k]) } else { d.guilds[k].name.clone() }).collect());
        v.text("lblWelcome", format!("{}{}{}", d.t(0x6621_4d79), gd.name, d.t(0x573f_816b)));
        let (gm, gm_level) = g.hall.guildmaster(d, visit);
        v.text("lblGuildMaster", if gm == c.name { d.t(0xc456_ef26).into() } else if gm.is_empty() { d.t(0x98d4_d73d).into() } else { format!(" {gm}{} {gm_level})", d.t(0x7c29_736d)) });
        let s0 = desk.rect(F, "lblStat(0)");
        let n0 = desk.rect(F, "lblStatName(0)");
        for (i, n) in ["Str", "Int", "Wis", "Con", "Cha", "Dex"].iter().enumerate() {
            if i > 0 {
                let dx = 32 * i as i32;
                v.clone_rect("lblStat(0)", i as u16, [s0[0] + dx, s0[1], s0[2], s0[3]]).clone_rect("lblStatName(0)", i as u16, [n0[0] + dx, n0[1], n0[2], n0[3]]);
            }
            v.text(&format!("lblStat({i})"), gd.stat_req[i].to_string()).text(&format!("lblStatName({i})"), *n);
        }
        let (info, refused) = c.welcome(d, visit);
        v.text("lblWelcomeInfo", info);
        if c.levels[visit] > 0 {
            v.text("cmdJoin", d.t(0x242f_b009));
        }
        let (need, gold) = (c.xp_needed(d, t), ddj_rules::Tuning::level_gold(c.level(), d.guilds[c.guild].gold_factor));
        let ready = c.xp[c.guild] >= need && c.gold + c.bank >= gold;
        v.enable("cmdMake", visit == c.guild && ready).enable("cmdInfo", visit == c.guild).enable("cmdJoin", visit != c.guild && !refused);
        v.enable("cmdLog", false).enable("cmdLibrary", false);
        let ev = desk.window(ctx, F, v, false);
        if ev.click("cmdVisit") || ev.double.as_deref() == Some("lstGuilds") {
            desk.state(F, "visit").sel = desk.state(F, "lstGuilds").sel.and_then(|i| shown.get(i).copied());
        }
        let mut msg = String::new();
        if ev.click("cmdMake") {
            match g.party[sel].make_level(d, t) {
                Ok(lines) => {
                    let level = format!("{}\n{} {}!", d.t(CONGRATULATIONS), d.t(0x8061_d5d4), g.party[sel].level());
                    self.boxes.push((d.t(0xe241_4a85).into(), level));
                    g.log.extend(lines);
                    self.boxes.extend(g.guildmaster(d, sel).into_iter().map(|(title, s)| (d.t(title).into(), s)));
                }
                // Pinned with a quest open: the original offers a forfeit.
                Err((_, s)) if g.party[sel].may_forfeit(d, t) => self.dialog = Some(Dialog::Confirm(s, Deed::Forfeit(sel))),
                Err((title, s)) => self.boxes.push((d.t(title).into(), s)),
            }
        }
        let c = &mut g.party[sel];
        if ev.click("cmdJoin") && c.levels[visit] > 0 {
            match c.join(d, visit) {
                Ok(s) => msg = s,
                Err(e) => self.boxes.push((d.t(0x8b3c_5ec6).into(), e)),
            }
        } else if ev.click("cmdJoin") {
            let mut q = String::new();
            if c.levels.iter().all(|&l| l < 10) {
                q = format!("{}\n{}\n{}\n\n", d.t(0x62a6_019b), d.t(0x73b0_1cc7), d.t(0x5868_35bd));
            }
            q += &match c.join_cost(d, visit) {
                0 => format!("{}{}{}", d.t(0xbb1b_80ba), gd.name, d.t(0x653f_9775)),
                cost => format!("{}{}{}\n{}{}.\n{}", d.t(0xba63_2f98), gd.name, d.t(0xb607_c7e8), d.t(0x5809_706f), commas(cost), d.t(0xd43c_259a)),
            };
            self.dialog = Some(Dialog::Confirm(q, Deed::Join(sel, visit)));
        }
        if ev.click("cmdInfo") {
            let have = d.t(0x10e9_c688);
            let mut s = format!("{}{}{}\n{have}{}{}\n", d.t(0x193a_3c60), commas(need), d.t(0x0265_25a6), commas(c.xp[c.guild]), d.t(0xcd50_d9ab));
            s += &format!("{}{}{}\n{have}{}{}\n", d.t(0xd55d_cc25), commas(gold), d.t(0x1aa5_7089), commas(c.gold + c.bank), d.t(0x1aa5_7089));
            if let Some(m) = c.quest {
                s += &format!("\n{}\n{}{}.\n", d.t(0x0e54_b26c), d.t(0x6cc2_654d), d.monsters[m].name);
            }
            if let Some(it) = c.quest_item {
                s += &format!("\n{}\n{}{}.\n", d.t(0x0e54_b26c), d.t(0x4bc8_33ad), d.items[it].name);
            }
            self.boxes.push((d.t(0xc0e1_04e4).into(), s));
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
    fn bank(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        const F: &str = "frmCityBank";
        let sel = self.sel.min(self.game.as_ref().unwrap().party.len() - 1);
        let g = self.game.as_mut().unwrap();
        let c = &g.party[sel];
        let mut v = View::default();
        // The bank's items (lstItems): drop a pack item on it to bank it, pick one to take it back.
        let kept: Vec<String> = c.vault.iter().map(|h| magic::item_name(d, h)).collect();
        v.text("lblStatus", format!("You have {} gold on hand and {} in the bank.", c.gold, c.bank)).items("lstItems", kept).enable("cmdInfo", false);
        let ev = desk.window(ctx, F, v, false);
        if let Some((k, Drag(1, m, slot))) = ev.dropped.clone()
            && m == sel
            && k == "lstItems"
            && let Err(e) = g.party[sel].stash(d, slot)
        {
            self.boxes.push((String::new(), e));
        }
        if let Some(k) = desk.state(F, "lstItems").sel.take()
            && k < g.party[sel].vault.len()
            && let Err(e) = g.party[sel].unstash(d, k)
        {
            self.boxes.push((String::new(), e));
        }
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
            // Pool & Deposit: the party's purses to this character's account; Pool All
            // (4) the others' bank balances too (cmdCommand_Click).
            Some(b @ (2 | 4)) => {
                let pooled: u64 = g.party.iter_mut().enumerate().filter(|(i, _)| *i != sel).map(|(_, m)| std::mem::take(&mut m.gold) + if b == 4 { std::mem::take(&mut m.bank) } else { 0 }).sum();
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
    fn morgue(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        let g = self.game.as_mut().unwrap();
        let in_morgue = (0..g.party.len()).find(|&i| g.party[i].life == Life::InMorgue);
        let lost = (0..g.party.len()).find(|&i| matches!(g.party[i].life, Life::Dead(_)));
        let mut v = View::default();
        // "Raise <companion> for <fee>" for the first dead companion (sub2BDB).
        let comp = g.companion_raise_cost(d, 0);
        if let Some((k, cost)) = comp {
            v.text("cmdCommand(2)", format!("{}{}{}{}", d.t(0x4c84_5f59), d.monsters[g.party[0].companions[k].monster].name, d.t(0x6bc1_25fe), commas(cost)));
        }
        // The original's dungeon button for the dead, "Wait for Rescue", in place of Pickup.
        v.text("cmdCommand(1)", d.t(0xfc5d_f418));
        v.enable("cmdCommand(0)", in_morgue.is_some()).enable("cmdCommand(1)", lost.is_some()).enable("cmdCommand(2)", comp.is_some()).enable("cmdCommand(3)", lost.is_some());
        let ev = desk.window(ctx, "frmCityMorgue", v, false);
        if ev.click("cmdCommand(1)")
            && let Some(i) = lost
            && let Some(s) = g.wait_for_rescue(d, i)
        {
            self.boxes.push((d.t(0xfa49_19a4).into(), s));
        }
        if ev.click("cmdCommand(2)") && comp.is_some() {
            match g.raise_companion(d, 0) {
                Ok(s) => g.log.push(s),
                Err(e) => self.boxes.push((d.t(0xa003_c859).into(), e)),
            }
        }
        if ev.click("cmdCommand(0)")
            && let Some(i) = in_morgue
        {
            let q = format!("{}{}{}{}.\n{}", d.t(0x659b_51d7), g.party[i].name, d.t(0x9ef2_f919), commas(g.raise_cost(i)), d.t(0x954a_4cca));
            self.dialog = Some(Dialog::Confirm(q, Deed::Raise(i)));
        }
        if ev.click("cmdCommand(3)")
            && let Some(i) = lost
        {
            let q = format!("{}{}{}{}.\n{}", d.t(0xe5ec_e427), commas(g.rescue_cost(i)), d.t(0xc3df_d96b), g.party[i].name, d.t(0x12bc_468a));
            self.dialog = Some(Dialog::Confirm(q, Deed::Rescue(i)));
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
            v.scroll(&k, "vsbComps", row0[3] as f32, list.len().saturating_sub(rows));
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
        // The companion dropped on the ID box: its value, ID cost and level (fn33FA, fn33E9).
        v.text("lblValue", sell.map_or(String::new(), |k| commas(g.companion_sell_price(d, k))));
        v.text("lblIDCost", sell.filter(|&k| comps[k].id > 0).map_or(String::new(), |k| commas(g.companion_id_price(d, k))));
        v.text("lblIDLevel", sell.map_or(String::new(), |k| comps[k].id.to_string())).enable("cmdID", sell.is_some_and(|k| comps[k].id > 0));
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
            msg = answer(&mut self.boxes, d, g.buy_companion(d, m));
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
            match g.rebind_offer(d, k) {
                Ok((new, price)) => {
                    let q = format!("{}{new}{}{}.\n{}", d.t(0x4095_a138), d.t(0x9ef2_f919), commas(price), d.t(0x954a_4cca));
                    self.dialog = Some(Dialog::Confirm(q, Deed::Bind(k)));
                }
                Err(e) => self.boxes.push((d.t(0x3e5c_ab8c).into(), e)),
            }
        }
        if ev.click("cmdID")
            && let Some(k) = sell
        {
            msg = answer(&mut self.boxes, d, g.identify_companion(d, k));
        }
        if let Some(a) = ev.index("cmdRealign")
            && let Some(k) = sell
        {
            msg = answer(&mut self.boxes, d, g.realign_companion(d, k, a as i16));
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
