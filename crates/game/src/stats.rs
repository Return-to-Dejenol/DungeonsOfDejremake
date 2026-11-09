//! The windows that stay open while playing: the character (frmCharStat1), Objects &
//! Spells (frmCharStat3), Messages (frmCharStat4), Party Information (frmParty) and the
//! AutoMap, laid out as the original's run-time code did.

use ddj_rules::{
    Tuning,
    ai::{Act, Target},
    game::{self, Game, Life, Side},
    magic,
    mdr::{GameData, square},
};
use eframe::egui::{self, Color32, Painter, Rect, Stroke, pos2, vec2};

use crate::{App, Dialog, Screen, win::{Desk, Drag, View}};

const SEL_BACK: Color32 = Color32::from_rgb(0, 0, 128);

/// The original's default arrangement (sub14B4) on its 800x600 desktop: the character
/// top left, the city or dungeon beside it, Objects & Spells under that, the party under
/// the character, the AutoMap beside the party, Messages bottom right.
/// On a wide desktop (phones, the web), three columns instead: the character; the city
/// or dungeon over the party and Messages; the AutoMap over Objects & Spells.
pub fn arrange(desk: &mut Desk) {
    if desk.pos.contains_key("frmCharStat1") && desk.arranged == Some(desk.wide) {
        return;
    }
    desk.arranged = Some(desk.wide);
    let o = desk.client.min;
    if desk.wide {
        let mut x = o.x + 1.0;
        for col in COLS {
            let mut y = o.y + 1.0;
            for &f in col {
                desk.pos.insert(f.into(), pos2(x, y));
                y += desk.outer(f).y + 1.0;
            }
            x += col.iter().map(|f| desk.outer(f).x).fold(0.0, f32::max) + 1.0;
        }
        desk.pos.insert("frmCityMain".into(), desk.pos["frmDungeonMain"]);
        return;
    }
    let (c1, dn, c3, c4) = (desk.outer("frmCharStat1"), desk.outer("frmDungeonMain"), desk.outer("frmCharStat3"), desk.outer("frmCharStat4"));
    let dun = o + vec2(1.0 + c1.x + 1.0, 1.0);
    let below = dun.y + dn.y + 4.0;
    for (f, p) in [
        ("frmCharStat1", o + vec2(1.0, 1.0)),
        ("frmDungeonMain", dun),
        ("frmCityMain", dun),
        ("frmCharStat3", pos2(dun.x + dn.x - c3.x, below)),
        ("frmParty", o + vec2(1.0, c1.y + 2.0)),
        ("frmCharStat4", pos2(desk.client.right() - c4.x - 8.0, desk.client.bottom() - 55.0 - c4.y)),
        ("frmAutoMap", pos2(dun.x, below)),
    ] {
        desk.pos.insert(f.into(), p);
    }
}

const COLS: [&[&str]; 3] = [&["frmCharStat1"], &["frmDungeonMain", "frmParty", "frmCharStat4"], &["frmAutoMap", "frmCharStat3"]];

/// The three columns' desktop size, with the main window's title bar and toolbar (50).
pub fn wide_size(desk: &Desk) -> egui::Vec2 {
    let col = |c: &[&str]| c.iter().map(|f| desk.outer(f)).fold(vec2(0.0, 0.0), |a, b| vec2(a.x.max(b.x + 1.0), a.y + b.y + 1.0));
    COLS.iter().map(|c| col(c)).fold(vec2(1.0, 0.0), |a, b| vec2(a.x + b.x, a.y.max(b.y + 51.0)))
}

/// A number with thousands separators, as the original shows gold and experience.
pub fn commas(n: impl std::fmt::Display) -> String {
    let s = n.to_string();
    let (sign, digits) = s.split_at(usize::from(s.starts_with('-')));
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

/// Status letters as the party window shows them.
fn status(m: &game::Character) -> String {
    if m.life != Life::Alive {
        return "Dead".into();
    }
    let s = &m.status;
    let ail: String = [(s.poison > 0, "Po"), (s.paralysis > 0, "Pa"), (s.disease > 0, "Di"), (s.stoned, "St")].iter().filter(|x| x.0).map(|x| x.1).collect();
    if ail.is_empty() { "OK".into() } else { ail }
}

/// Seen squares of the current floor into `r`, six pixels a square, north up and
/// centred on the party (an arrow).
fn automap(p: &Painter, r: Rect, s: f32, d: &GameData, g: &Game) {
    let pos = g.pos();
    let Some(f) = (pos.floor > 0).then(|| &d.floors[pos.floor - 1]) else { return };
    let Some(seen) = g.lead().seen.get(pos.floor - 1) else { return };
    let cell = 7.0 * s;
    let (ox, oy) = (r.center().x - (pos.x as f32 + 0.5) * cell, r.center().y - (f.height - pos.y) as f32 * cell + 0.5 * cell);
    let corner = |x: usize, y: usize| pos2(ox + x as f32 * cell, oy + (f.height - y) as f32 * cell);
    for y in 0..f.height {
        for x in 0..f.width {
            if !seen.get(y * f.width + x).copied().unwrap_or(false) && (x, y) != (pos.x, pos.y) {
                continue;
            }
            let fl = f.square(x, y).flags;
            let mark = [(square::STAIRS_UP, Color32::from_rgb(0, 255, 0)), (square::STAIRS_DOWN, Color32::from_rgb(255, 0, 0)), (square::WATER, Color32::from_rgb(0, 0, 255))].iter().find(|m| fl & m.0 != 0).map(|m| m.1);
            if let Some(c) = mark {
                p.rect_filled(Rect::from_two_pos(corner(x, y), corner(x + 1, y + 1)).shrink(cell / 3.0), 0.0, c);
            }
            for (dir, a, b) in [(0, corner(x, y + 1), corner(x + 1, y + 1)), (1, corner(x + 1, y), corner(x + 1, y + 1)), (2, corner(x, y), corner(x + 1, y)), (3, corner(x, y), corner(x, y + 1))] {
                match game::side(f, x, y, dir) {
                    Side::Wall => {
                        p.line_segment([a, b], Stroke::new(1.0, Color32::WHITE));
                    }
                    Side::Door => {
                        p.line_segment([a, b], Stroke::new(1.0, Color32::YELLOW));
                    }
                    Side::Open => {}
                }
            }
        }
    }
    // The original's cursor (shpLocation), with the facing as a short tick.
    let c = Rect::from_two_pos(corner(pos.x, pos.y), corner(pos.x + 1, pos.y + 1)).center();
    p.rect_stroke(Rect::from_center_size(c, vec2(cell * 0.5, cell * 0.5)), 0.0, Stroke::new(s, Color32::YELLOW), egui::StrokeKind::Inside);
    let dir = [vec2(0.0, -1.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(-1.0, 0.0)][pos.facing as usize] * cell * 0.45;
    p.line_segment([c, c + dir], Stroke::new(s, Color32::YELLOW));
}

impl App {
    fn member(&self) -> usize {
        self.game.as_ref().map_or(0, |g| self.sel.min(g.party.len() - 1))
    }

    /// The character window: its tabs fill the stat rows the original loaded at run time.
    pub(crate) fn stats_window(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) {
        const F: &str = "frmCharStat1";
        let sel = self.member();
        let g = self.game.as_ref().unwrap();
        let c = &g.party[sel];
        let tab = desk.state(F, "tabStatTab").tab;
        let mut v = View::default();
        v.caption = Some(format!("\"{}\"", c.name));
        v.show("elaTabFrame(0)", tab != 5).show("elaTabFrame(1)", tab == 5).show("picTabPicture", false);
        if desk.touch {
            v.tab_height("tabStatTab", 26.0);
            for k in ["elaTabFrame(0)", "elaTabFrame(1)"] {
                let r = desk.rect(F, k);
                v.rect(k, [r[0], r[1] + 12, r[2], r[3] - 12]);
            }
        }
        let (a, df) = c.att_def(d, t);
        let rows: Vec<(String, String)> = match tab {
            0 => {
                let mut r = vec![
                    ("Age".into(), c.age().to_string()),
                    ("Spells".into(), format!("{}/{}", commas(c.sp), commas(c.max_sp(d, t)))),
                    ("Spell Level".into(), c.level().to_string()),
                    ("Hits".into(), format!("{}/{}", commas(c.hp), commas(c.max_hp))),
                    ("Att/Def".into(), format!("{a}/{df}")),
                    ("Experience".into(), commas(c.xp[c.guild])),
                    ("Gold".into(), commas(c.gold)),
                ];
                for (i, n) in ["Strength", "Intelligence", "Wisdom", "Constitution", "Charisma", "Dexterity"].iter().enumerate() {
                    r.push(((*n).into(), c.stats[i].to_string()));
                }
                r
            }
            1 => vec![("Resistances".into(), if c.buffs.resist == 0 { "None".into() } else { format!("{:b}", c.buffs.resist) })],
            2 => vec![
                ("Race".into(), d.races[c.race].name.clone()),
                ("Sex".into(), if c.female { "Female" } else { "Male" }.into()),
                ("Alignment".into(), ["Good", "Neutral", "Evil"][c.alignment as usize].into()),
                ("Bank".into(), commas(c.bank)),
                ("Kills".into(), commas(c.kills)),
            ],
            3 => {
                let mut r: Vec<(String, String)> = (0..d.guilds.len()).filter(|&i| c.levels[i] > 0).map(|i| (d.guilds[i].name.clone(), format!("Level {}", c.levels[i]))).collect();
                for (i, n) in game::ABILITIES.iter().enumerate() {
                    r.push(((*n).into(), format!("{:.0}", c.ability(d, i))));
                }
                r
            }
            _ => {
                let mut r = Vec::new();
                if let Some(m) = c.quest {
                    r.push(("Quest".into(), d.monsters[m].name.clone()));
                }
                if c.buffs.xp_mult > 1.0 {
                    r.push(("Learning".into(), format!("{}x", c.buffs.xp_mult)));
                }
                if c.buffs.protection > 0 {
                    r.push(("Protection".into(), format!("+{}", c.buffs.protection)));
                }
                if c.buffs.levitate {
                    r.push(("Levitating".into(), String::new()));
                }
                match c.life {
                    Life::Dead(p) => r.push(("Dead on level".into(), format!("{} at {},{}", p.floor, p.x + 1, p.y + 1))),
                    Life::InMorgue => r.push(("Dead".into(), "in the morgue".into())),
                    Life::Alive => {}
                }
                r
            }
        };
        // Form_Resize: thirteen rows, half-row gaps after Age and Spell Level, a full
        // one (and the border) before the stats.
        let (s0, i0) = (desk.rect(F, "lblStat(0)"), desk.rect(F, "lblStatInfo(0)"));
        let (h, half) = (s0[3], s0[3] / 2);
        let mut top = s0[1];
        for i in 0..13u16 {
            if i > 0 {
                top += h + match i {
                    1 | 3 => half,
                    7 => 2 * half,
                    _ => 0,
                };
            }
            // Other tabs reuse the rows with a wider value column.
            let stat = if tab != 0 { [s0[0] - 40, top, s0[2] + 40, h] } else if i >= 7 { [i0[0] + i0[2], top, 27, h] } else { [s0[0], top, s0[2], h] };
            if i == 7 {
                v.rect("elaBorder", [5, top - (h as f32 / 1.2) as i32, 227, 8]);
            }
            if i == 0 || i == 6 {
                v.rect(&format!("lblStat({i})"), stat);
            } else {
                v.clone_rect("lblStat(0)", i, stat);
            }
            if i > 0 {
                v.clone_rect("lblStatInfo(0)", i, [i0[0], top, i0[2], h]);
            }
            let (name, value) = rows.get(usize::from(i)).cloned().unwrap_or_default();
            v.text(&format!("lblStatInfo({i})"), name).text(&format!("lblStat({i})"), value);
        }
        v.show("elaBorder", tab == 0);
        // The ailments, one fixed row each, beside the stats.
        let sp = desk.rect(F, "lblSpecial(0)");
        let first = s0[1] + 6 * h + 2 * half + 2 * h;
        for (i, (on, name)) in [(c.status.poison > 0, "Poisoned"), (c.status.disease > 0, "Diseased"), (c.status.paralysis > 0, "Paralyzed"), (c.status.stoned, "Stoned")].into_iter().enumerate() {
            let k = format!("lblSpecial({i})");
            let r = [sp[0], first + h * i as i32, sp[2], sp[3]];
            if i == 0 {
                v.rect(&k, r);
            } else {
                v.clone_rect("lblSpecial(0)", i as u16, r);
            }
            v.text(&k, name).show(&k, on && tab == 0);
        }
        v.text("lblRaceInfo", format!("{} {} ({})", if c.female { "Female" } else { "Male" }, d.races[c.race].name, ["Good", "Neutral", "Evil"][c.alignment as usize]));
        v.text("lblGuildInfo", format!("{} ({})", d.guilds[c.guild].name, c.level()));
        v.text("elaGroupInfo", if g.party.len() > 1 { format!("In a party of {}", g.party.len()) } else { "You are Alone".into() });
        // Buffers: the original's ten quick slots, empty in the remake.
        let b0 = desk.rect(F, "lblBuffer(0)");
        for i in 0..10u16 {
            if i > 0 {
                v.clone_rect("lblBuffer(0)", i, [b0[0], b0[1] + b0[3] * i32::from(i), b0[2], b0[3]]);
                v.clone_rect("lblBufferStat(0)", i, [desk.rect(F, "lblBufferStat(0)")[0], b0[1] + b0[3] * i32::from(i), 24, b0[3]]);
            }
            v.text(&format!("lblBuffer({i})"), format!("{}.", i + 1)).text(&format!("lblBufferStat({i})"), "");
        }
        desk.window(ctx, F, v, false);
    }

    /// Objects & Spells: the items list (drag them to the store), the spells, and the
    /// companions.
    pub(crate) fn objects_window(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) {
        const F: &str = "frmCharStat3";
        let sel = self.member();
        let tab = desk.state(F, "tabItemTab").tab;
        let g = self.game.as_ref().unwrap();
        let c = &g.party[sel];
        let mut v = View::default();
        for i in 0..3 {
            v.show(&format!("elaTabFrame({i})"), tab == i);
        }
        // Clear of the touch screen's thicker scroll bar.
        for i in (0..4).filter(|_| desk.touch) {
            let r = desk.rect(F, &format!("cmdCommand({i})"));
            v.rect(&format!("cmdCommand({i})"), [r[0] - 9, r[1], r[2], r[3]]);
        }
        // Items: sixteen-pixel rows in a strip that the scroll bar slides.
        let item = desk.rect(F, "lblItem(0)");
        let rows = 10usize;
        let top = desk.state(F, "vsbItems").top;
        let picked = desk.state(F, "items").sel.filter(|&i| i < c.items.len());
        // All forty slots, numbered, as the original lists them.
        const SLOTS: usize = 40;
        let top = top.min(SLOTS - rows);
        v.rect("elaItemFrame", [0, 1, 233, 168]).rect("picItems", [0, -(top as i32) * item[3], 400, item[3] * SLOTS as i32 + 2]);
        for i in 0..SLOTS {
            let (k, ks) = (format!("lblItem({i})"), format!("lblItemStatus({i})"));
            if i > 0 {
                v.clone_rect("lblItem(0)", i as u16, [item[0], item[1] + item[3] * i as i32, item[2], item[3]]);
                v.clone_rect("lblItemStatus(0)", i as u16, [2, item[1] + item[3] * i as i32, 14, 15]);
            }
            let h = c.items.get(i);
            v.text(&k, format!("{}. {}", i + 1, h.map_or(String::new(), |h| magic::item_name(d, h)))).text(&ks, if h.is_some_and(|h| h.equipped) { "E" } else { "" });
            if h.is_some() {
                v.drag(&k, Drag(1, sel, i));
            }
            if picked == Some(i) {
                v.back(&k, SEL_BACK).fore(&k, Color32::WHITE);
            }
        }
        // Spells: five rows in the light-yellow box.
        let known = magic::known(c, d);
        let n0 = desk.rect(F, "lblName(0)");
        let stop = desk.state(F, "vsbSpells").top.min(known.len().saturating_sub(5));
        let spick = desk.state(F, "spells").sel;
        v.items("cmbClass", vec!["All".into()]);
        for r in 0..5usize {
            let row = known.get(stop + r).copied();
            for base in ["lblName", "lblGuild", "lblLevel", "lblPoints"] {
                let b = desk.rect(F, &format!("{base}(0)"));
                if r > 0 {
                    v.clone_rect(&format!("{base}(0)"), r as u16, [b[0], b[1] + n0[3] * r as i32, b[2], b[3]]);
                }
            }
            let sp = row.map(|s| &d.spells[s]);
            v.text(&format!("lblName({r})"), sp.map_or(String::new(), |s| s.name.clone()));
            v.text(&format!("lblGuild({r})"), "").text(&format!("lblLevel({r})"), sp.map_or(String::new(), |s| s.level.to_string())).text(&format!("lblPoints({r})"), sp.map_or(String::new(), |s| s.sp.to_string()));
            if row.is_some() && row == spick {
                v.back(&format!("lblName({r})"), SEL_BACK).fore(&format!("lblName({r})"), Color32::WHITE);
            }
        }
        v.text("txtSpell", spick.map_or(String::new(), |s| d.spells[s].name.clone()));
        v.enable("cmdCast", spick.is_some_and(|s| magic::castable(c, d, t, s)));
        // Companions: four portraits.
        for k in 0..4usize {
            let comp = c.companions.get(k);
            v.show(&format!("picCompanion({k})"), comp.is_some()).show(&format!("lblCompName({k})"), comp.is_some()).show(&format!("lblCompHits({k})"), comp.is_some());
            if let Some(cp) = comp {
                v.text(&format!("lblCompName({k})"), d.monsters[cp.monster].name.clone()).text(&format!("lblCompHits({k})"), format!("{}/{}", cp.hp, cp.max_hp));
                if let Some(a) = &self.art {
                    let (id, uv) = a.portrait(d.monsters[cp.monster].picture);
                    v.pic(&format!("picCompanion({k})"), id, uv);
                }
                v.drag(&format!("picCompanion({k})"), Drag(2, sel, k));
            }
        }
        v.text("lblCarryInfo", format!("{} companion{}", c.companions.len(), if c.companions.len() == 1 { "" } else { "s" })).show("cmdCompanion(0)", false);
        let ev = desk.window(ctx, F, v, false);
        if let Some(i) = ev.clicked.as_deref().and_then(|k| k.strip_prefix("lblItem(")).and_then(|k| k.trim_end_matches(')').parse::<usize>().ok()) {
            desk.state(F, "items").sel = (i < self.game.as_ref().unwrap().party[sel].items.len()).then_some(i);
        }
        if let Some(r) = ev.index("lblName") {
            desk.state(F, "spells").sel = known.get(stop + usize::from(r)).copied();
        }
        let g = self.game.as_mut().unwrap();
        let picked = desk.state(F, "items").sel.filter(|&i| i < g.party[sel].items.len());
        let mut msg = String::new();
        match (ev.index("cmdCommand"), picked) {
            (Some(0), Some(i)) => msg = g.party[sel].toggle_equip(d, i).err().unwrap_or_default(),
            (Some(1), Some(i)) => {
                let it = g.party[sel].items[i].item;
                magic::use_item(g, d, t, sel, it);
            }
            (Some(2), Some(i)) if g.party[sel].items[i].item == game::HANDS => msg = "You can't drop your hands.".into(),
            (Some(2), Some(i)) => {
                let h = g.party[sel].items.remove(i);
                msg = format!("{} dropped {}.", g.party[sel].name, magic::item_name(d, &h));
            }
            (Some(3), Some(i)) => {
                let h = &g.party[sel].items[i];
                let it = &d.items[h.item];
                msg = if h.known { format!("{}: attack {}, defense {}, worth {} gold.", magic::item_name(d, h), it.attack, it.defense, it.price) } else { format!("{} is unidentified.", magic::item_name(d, h)) };
            }
            _ => {}
        }
        if ev.click("cmdCast")
            && let Some(s) = desk.state(F, "spells").sel
        {
            let target = if magic::kind(&d.spells[s]).is_some_and(|k| matches!(k, magic::Magic::Damage | magic::Magic::MassDamage | magic::Magic::Sleep)) { Target::Strongest } else { Target::LowestAlly };
            match (g.fight.as_mut(), sel) {
                (Some(f), 0) => {
                    f.order = Some(Act::Cast(s, target));
                    msg = format!("You will cast {} next round.", d.spells[s].name);
                }
                (Some(_), _) => msg = "Allies cast by their macros in combat.".into(),
                (None, _) => magic::cast(g, d, t, sel, s, target),
            }
        }
        if ev.clicked.is_some() {
            self.say(msg);
            self.persist();
        }
    }

    /// Messages: the log, newest at the bottom.
    pub(crate) fn messages_window(&mut self, desk: &mut Desk, ctx: &egui::Context) {
        let g = self.game.as_ref().unwrap();
        let all: Vec<&String> = g.log.iter().filter(|l| !l.is_empty()).collect();
        let lines: Vec<String> = all[all.len().saturating_sub(200)..].iter().map(|s| (*s).clone()).collect();
        // New lines scroll the list to the bottom.
        let st = desk.state("frmCharStat4", "lstMessages");
        if st.text != g.log.len().to_string() {
            st.top = usize::MAX;
            st.text = g.log.len().to_string();
            st.sel = lines.len().checked_sub(1);
        }
        let mut v = View::default();
        v.items("lstMessages", lines);
        desk.window(ctx, "frmCharStat4", v, false);
    }

    /// Party Information: a row a member; Switch To, Options (the remake's macros) and
    /// Leave.
    pub(crate) fn party_window(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, _t: &Tuning) {
        const F: &str = "frmParty";
        let g = self.game.as_ref().unwrap();
        let sel = self.member();
        let mut v = View::default();
        let row = desk.rect(F, "lblName(0)")[3];
        for (i, m) in g.party.iter().enumerate() {
            for base in ["lblName", "lblHits", "lblSpells", "lblStatus", "lblOption"] {
                if i > 0 {
                    let b = desk.rect(F, &format!("{base}(0)"));
                    v.clone_rect(&format!("{base}(0)"), i as u16, [b[0], b[1] + row * i as i32, b[2], b[3]]);
                }
            }
            let option = match (&g.fight, i) {
                (Some(f), 0) => match f.order {
                    None => "None".to_string(),
                    Some(Act::Fight(_)) => "Fight".into(),
                    Some(Act::Defend) => "Defend".into(),
                    Some(Act::Run) => "Run".into(),
                    Some(Act::Cast(s, _)) => d.spells[s].name.clone(),
                    Some(Act::Open) => "Open".into(),
                    Some(_) => "Other".into(),
                },
                (_, 0) => "None".into(),
                _ => "Macros".into(),
            };
            v.text(&format!("lblName({i})"), m.name.clone()).text(&format!("lblHits({i})"), format!("{}/{}", m.hp, m.max_hp)).text(&format!("lblSpells({i})"), m.sp.to_string());
            v.text(&format!("lblStatus({i})"), status(m)).text(&format!("lblOption({i})"), option);
            let well = m.alive();
            v.fore(&format!("lblName({i})"), if well { Color32::from_rgb(0, 128, 0) } else { Color32::from_rgb(192, 0, 0) });
            v.fore(&format!("lblHits({i})"), if m.hp * 2 >= m.max_hp { Color32::from_rgb(0, 128, 0) } else { Color32::from_rgb(192, 0, 0) });
            v.fore(&format!("lblStatus({i})"), if status(m) == "OK" { Color32::BLACK } else { Color32::from_rgb(192, 0, 0) });
        }
        let town = self.screen == Screen::Town;
        v.enable("cmdCommand(0)", sel > 0 && g.party[sel].alive()).enable("cmdCommand(2)", town && sel > 0);
        let ev = desk.window(ctx, F, v, false);
        for base in ["lblName", "lblHits", "lblSpells", "lblStatus", "lblOption"] {
            if let Some(i) = ev.index(base) {
                self.sel = usize::from(i);
            }
        }
        let g = self.game.as_mut().unwrap();
        match ev.index("cmdCommand") {
            Some(0) => {
                g.switch_to(sel);
                self.sel = 0;
            }
            Some(1) => self.dialog = Some(Dialog::Macros(sel)),
            Some(2) => {
                if let Some(c) = g.dismiss(sel) {
                    let _ = c.save(&crate::saves());
                    self.sel = 0;
                    self.persist();
                }
            }
            _ => {}
        }
    }

    /// The AutoMap: what the party has seen of this level.
    pub(crate) fn automap_window(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        let g = self.game.as_ref().unwrap();
        let p = g.pos();
        let mut v = View::default();
        v.caption = Some(format!("Automap (Level {})", p.floor));
        let size = desk.size("frmAutoMap");
        let (w, h) = (size.x as i32, size.y as i32);
        v.show("hsbLevel", false).show("lblLevel", false).show("lblInfo", false);
        v.rect("picPicture", [0, 0, w, h]).rect("picMaze", [0, 0, w - 2, h - 2]);
        v.paint("picMaze", move |pt, r, s| automap(pt, r, s, d, g));
        desk.window(ctx, "frmAutoMap", v, false);
    }
}
