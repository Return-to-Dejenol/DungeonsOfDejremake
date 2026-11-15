//! The windows that stay open while playing: the character (frmCharStat1), Objects &
//! Spells (frmCharStat3), Messages (frmCharStat4), Party Information (frmParty) and the
//! AutoMap, laid out as the original's run-time code did.

use ddj_rules::{
    Tuning,
    ai::{Act, Target},
    game::{self, Life},
    magic,
    mdr::{GameData, Words},
};
use std::collections::HashMap;

use ddj_extract::forms::Val;
use eframe::egui::{self, Color32, Rect, pos2, vec2};

use crate::{App, Deed, Dialog, Screen, art, win::{Desk, Drag, View}};

const SEL_BACK: Color32 = Color32::from_rgb(0, 0, 128);

/// The original's default arrangement (sub14B4) on its 800x600 desktop: the character
/// top left, the city or dungeon beside it, Objects & Spells under that, the party under
/// the character, the AutoMap beside the party, Messages bottom right.
/// On phones and the web, one of `LAYOUTS` instead, the city where the dungeon is.
pub fn arrange(desk: &mut Desk) {
    if desk.pos.contains_key("frmCharStat1") && desk.arranged == Some(desk.layout) {
        return;
    }
    desk.arranged = Some(desk.layout);
    let o = desk.client.min;
    if let Some(l) = desk.layout {
        let mut y = o.y + 1.0;
        for row in LAYOUTS[l] {
            let mut x = o.x + 1.0;
            for col in *row {
                let mut cy = y;
                for &f in *col {
                    desk.pos.insert(f.into(), pos2(x, cy));
                    cy += desk.outer(f).y + 1.0;
                }
                x += col.iter().map(|f| desk.outer(f).x).fold(0.0, f32::max) + 1.0;
            }
            y += row_size(desk, row).y;
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

/// The play windows' layouts for phones and the web: rows of columns of windows.
/// Landscape: the character; the dungeon over the party and Messages; the AutoMap over
/// Objects & Spells. Portrait: the dungeon, Messages, the character beside the AutoMap
/// over the party, then Objects & Spells.
pub const LAYOUTS: [&[&[&[&str]]]; 2] = [
    &[&[&["frmCharStat1"], &["frmDungeonMain", "frmParty", "frmCharStat4"], &["frmAutoMap", "frmCharStat3"]]],
    &[&[&["frmDungeonMain"]], &[&["frmCharStat4"]], &[&["frmCharStat1"], &["frmAutoMap", "frmParty"]], &[&["frmCharStat3"]]],
];

/// The phone shell's own windows (`add_shell`), and the windows each of its tabs shows.
pub const STRIP: &str = "phnTabs";
pub const PAD: &str = "phnPad";
pub const TABS: [&[&str]; 3] = [&["frmCharStat1"], &["frmCharStat3"], &["frmParty", "frmAutoMap"]];

/// The phone shell (Android) in place of the desktop: the City or Dungeon window (or the
/// open building) over Messages, as wide as the screen; then the tab strip and the windows
/// of the tab in front, as big as they fit; in the dungeon the touch pad at the bottom.
/// Landscape puts the strip and its windows on the right. The tab in front follows the
/// window last made active (the toolbar's buttons, the dungeon's Spell and Map).
pub fn shell(desk: &mut Desk, play: &str, dungeon: bool) {
    let c = desk.to_screen(desk.client);
    let tab = TABS.iter().position(|t| desk.active.as_deref().is_some_and(|a| t.contains(&a))).unwrap_or(desk.state(STRIP, "tabShell").tab);
    desk.state(STRIP, "tabShell").tab = tab;
    desk.place = ["frmCityMain", "frmDungeonMain", "frmCharStat1", "frmCharStat3", "frmParty", "frmAutoMap"].iter().map(|f| (f.to_string(), None)).collect();
    let o: HashMap<&str, egui::Vec2> = [play, "frmCharStat4", PAD, STRIP, "frmCharStat1", "frmCharStat3", "frmParty", "frmAutoMap"].into_iter().map(|f| (f, desk.outer(f))).collect();
    let left: Vec<&str> = [play, "frmCharStat4"].into_iter().chain(dungeon.then_some(PAD)).collect();
    let w = if c.height() > c.width() { c.width() } else { (c.height() / left.iter().map(|f| o[f].y / o[f].x).sum::<f32>()).min(c.width() * 0.6) };
    let pad = if dungeon { o[PAD].y * w / o[PAD].x } else { 0.0 };
    let mut y = c.top();
    for f in left {
        let at = if f == PAD { pos2(c.left(), c.bottom() - pad) } else { pos2(c.left(), y) };
        desk.place.insert(f.into(), Some((at, w / o[f].x)));
        y += o[f].y * w / o[f].x;
    }
    let area = if w < c.width() * 0.99 { Rect::from_min_max(pos2(c.left() + w, c.top()), c.max) } else { Rect::from_min_max(pos2(c.left(), y - pad), pos2(c.right(), c.bottom() - pad)) };
    // The strip as wide as its column, at one and a half times.
    let sw = (area.width() / 1.5) as i32;
    if let Some(f) = desk.forms.get_mut(STRIP) {
        f.rect[2] = sw;
        f.controls[0].rect[2] = sw;
    }
    desk.place.insert(STRIP.into(), Some((area.min, 1.5)));
    let rest = area.with_min_y(area.top() + o[STRIP].y * 1.5);
    let wins: Vec<&str> = TABS[tab].iter().copied().filter(|&f| dungeon || f != "frmAutoMap").collect();
    let (mw, sh) = wins.iter().fold((0.0f32, 0.0f32), |(a, b), f| (a.max(o[f].x), b + o[f].y));
    let k = (rest.width() / mw).min(rest.height() / sh).clamp(0.2, 2.0);
    let mut y = rest.top();
    for f in wins {
        desk.place.insert(f.into(), Some((pos2(rest.center().x - o[f].x * k / 2.0, y), k)));
        y += o[f].y * k;
    }
}

/// The shell's windows in the original's style: the tab strip, and the dungeon's touch pad
/// with the Dungeon window's own movement and action buttons (and the remake's Defend and
/// Run, the keyboard's D and R).
pub fn add_shell(desk: &mut Desk) {
    desk.synth(STRIP, "", 200, 28, &[("videosoftindextab", "tabShell", None, [0, 0, 200, 28], "Character|Objects|Party")]);
    #[rustfmt::skip]
    desk.synth(PAD, "", 240, 52, &[
        ("CommandButton", "cmdMove", Some(0), [2, 2, 36, 23], ""), ("CommandButton", "cmdMove", Some(2), [39, 2, 36, 23], ""),
        ("CommandButton", "cmdMove", Some(1), [76, 2, 36, 23], ""), ("CommandButton", "cmdMove", Some(3), [113, 2, 36, 23], ""),
        ("CommandButton", "cmdAction", Some(2), [152, 2, 42, 23], ""), ("CommandButton", "cmdAction", Some(3), [195, 2, 43, 23], ""),
        ("CommandButton", "cmdAction", Some(0), [2, 27, 58, 23], ""), ("CommandButton", "cmdDefend", None, [61, 27, 58, 23], "Defend"),
        ("CommandButton", "cmdRun", None, [120, 27, 58, 23], "Run"), ("CommandButton", "cmdAction", Some(1), [179, 27, 59, 23], ""),
    ]);
    let dun = desk.forms.get("frmDungeonMain").map(|f| f.controls.clone()).unwrap_or_default();
    for f in desk.forms.iter_mut().filter(|(k, _)| [STRIP, PAD].contains(&k.as_str())).map(|(_, f)| f) {
        f.props.insert("BorderStyle".into(), Val::N(0));
        for c in &mut f.controls {
            // Defend and Run take Fight's font.
            let same = dun.iter().find(|o| o.name == c.name && o.index == c.index);
            if let Some(o) = same.or(dun.iter().find(|o| o.name == "cmdAction" && o.index == Some(0) && c.name != "tabShell")) {
                let caption = c.props.get("Caption").cloned().filter(|_| same.is_none());
                c.props = o.props.clone();
                if let Some(cap) = caption {
                    c.props.insert("Caption".into(), cap);
                }
                if c.name == "cmdMove" {
                    c.props.insert("FontSize".into(), Val::F(12.0));
                }
            }
        }
    }
}

fn row_size(desk: &Desk, row: &[&[&str]]) -> egui::Vec2 {
    let col = |c: &[&str]| c.iter().map(|f| desk.outer(f)).fold(vec2(0.0, 0.0), |a, b| vec2(a.x.max(b.x + 1.0), a.y + b.y + 1.0));
    row.iter().map(|c| col(c)).fold(vec2(1.0, 0.0), |a, b| vec2(a.x + b.x, a.y.max(b.y)))
}

/// Each layout's desktop size, with the main window's title bar and toolbar (51), and
/// wide enough for the Main Menu.
pub fn layout_sizes(desk: &Desk) -> Vec<egui::Vec2> {
    let menu = desk.outer("frmMenu").x + 2.0;
    LAYOUTS.iter().map(|l| l.iter().map(|r| row_size(desk, r)).fold(vec2(menu, 51.0), |a, b| vec2(a.x.max(b.x), a.y + b.y))).collect()
}

pub use ddj_rules::game::commas;

/// The item info the original prints into the character window's Misc tab (sub151C),
/// as (row, column, text, style: 0 plain, 1 bold, 2 italic), as much as its
/// identification level `id` tells (0 completely ... 3 barely).
pub fn item_info(d: &GameData, item: usize, align: u8, id: u8) -> Vec<(i32, i32, String, u8)> {
    let it = &d.items[item];
    let t = |k: u32| d.t(k).to_string();
    let mut v = vec![(2, 1, t(0x1f64_8006), 1)];
    let name = if id < 2 {
        let mut n = it.name.clone();
        if align > 0 && id < 1 {
            n = format!("{n} {}", t([0x6a77_18bd, 0x7e99_ca7b, 0x82af_9716, 0x8a9e_5a8d][usize::from(align.min(3))]));
        }
        if id == 0 && it.w(40) != 0 {
            n += &t(0xe040_1cce);
        }
        n
    } else {
        t(0x1499_f31f)
    };
    v.push((2, 6, name, 0));
    let class_of = |k: i16| d.item_type_class.get(k as usize).copied().unwrap_or(0);
    if id < 2 {
        let mut ad = format!("{}/{}", it.attack, it.defense);
        if it.damage_mult != 0.0 && id == 0 && class_of(it.kind) == 0 {
            ad = format!("{ad} [{}]", it.damage_mult);
        }
        v.push((3, 1, t(0x26cb_6418), 1));
        v.push((3, 6, ad, 0));
    }
    v.push((4, 1, t(0x50f9_7d9f), 1));
    let mut class = match id {
        0..3 => d.item_types.get(it.kind as usize).cloned().unwrap_or_default(),
        3 => d.item_classes.get(class_of(it.kind) as usize).cloned().unwrap_or_default(),
        _ => t(0xcc20_0e59),
    };
    if id == 0 {
        class += &t(if it.w(42) != 0 { 0x8255_8a67 } else { 0xfce5_035e });
    }
    v.push((4, 6, class, 0));
    if id < 2 {
        v.push((7, 1, t(0x907e_fa93), 1));
        let req = ddj_rules::magic::stat_req(d, item);
        for s in 0..6 {
            v.push((6, s as i32 * 2 + 3, d.t(game::STAT_NAMES[s]).chars().take(3).collect(), 1));
            v.push((7, s as i32 * 2 + 4, req[s].to_string(), 0));
        }
    }
    if id < 1 {
        v.push((8, 1, t(0x44a3_7323), 1));
        for s in 0..6 {
            v.push((8, s as i32 * 2 + 4, it.w(33 + s).to_string(), 0));
        }
    }
    if id == 0 {
        let use_ = match it.w(10) {
            1..=10 => Some(0x9f2d_3105),
            11 => Some(0xf033_8c37),
            12 => Some(0x545f_4ac8),
            13 | 14 => Some(0x20ef_acad),
            _ => None,
        };
        if let Some(k) = use_ {
            v.push((10, 1, t(k), 0));
        }
    }
    if id < 2 {
        let hands = it.w(22);
        let swings = match it.power {
            1 if hands > 0 => t(0xd76c_a8c6),
            n if n > 1 => format!(", {n}{}", t(0x7f9b_ff0c)),
            _ => String::new(),
        };
        let words = [0x20af_1fc9, 0x1cb1_b18f, 0xfde2_0b3a];
        v.push((11, 1, if hands > 0 { format!("{}{}{swings}", words.get(hands as usize).map_or(hands.to_string(), |&k| t(k)), t(0x8545_e6cf)) } else { t(0x26a8_378a) }, 0));
    }
    if id < 1 && it.w(11) > 0 {
        let sp = d.spells.get(it.w(11) as usize - 1).map_or(String::new(), |s| s.name.clone());
        v.push((12, 1, format!("{}\"{sp}\"", t(0xa618_a785)), 0));
        if id == 0 {
            v.push((13, 1, if it.w(41) > 0 { format!("{}{}", t(0x8274_f490), it.w(41)) } else { t(0x41da_581e) }, 0));
        }
    }
    if id < 2 {
        v.push((10, 13, t(0x2378_e99d), 1));
        let mut n = 0;
        for g in (0..d.guilds.len()).filter(|&g| it.guilds & (1 << g) != 0) {
            n += 1;
            v.push((10 + n, 13, d.guilds[g].name.clone(), 0));
            v.push((10 + n, 19, if id == 0 { format!(" {}", ddj_rules::magic::guild_need(d, item, g)) } else { "?".into() }, 0));
        }
    }
    if id < 1 {
        let mut n = 0;
        let names = [0xbab3_1830, 0xd1dc_2b2c, 0x0ded_9363, 0xcb4b_34f3, 0xf31c_697d, 0x6e4d_112f];
        for (b, &k) in names.iter().enumerate().filter(|&(b, _)| it.long(7) >> b & 1 != 0) {
            let _ = b;
            n += 1;
            v.push((13 + n, 2, t(k), 2));
        }
        for r in (0..11).filter(|&r| it.long(24) >> r & 1 != 0) {
            n += 1;
            v.push((13 + n, 2, format!("{}{}", d.t(game::RESISTANCES[r]), t(0xbbdc_cf9f)), 2));
        }
    }
    v.push((20, 1, t(0x1a00_a69f), 1));
    v.push((21, 1, t([0x9951_9c42, 0xfb27_ac32, 0x6beb_260a, 0x6e4d_112f][usize::from(id.min(3))]), 0));
    if (500..500 + d.guilds.len() as i16).contains(&it.w(10)) {
        v.push((23, 1, t(0xc1ac_6cea), 1));
    }
    v
}

/// Status letters as the party window shows them.
fn status(m: &game::Character) -> String {
    if m.life != Life::Alive {
        return "Dead".into();
    }
    let s = &m.status;
    let ail: String = [(s.poison > 0, "Po"), (s.disease > 0, "Di"), (s.paralysis > 0, "Pa"), (s.stoned, "St")].iter().filter(|x| x.0).map(|x| x.1).collect();
    if ail.is_empty() { "OK".into() } else { ail }
}

/// Seen squares of the current floor into `r`, six pixels a square, north up and
/// centred on the party (an arrow).
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
        // Misc shows the item asked about (Info), printed as the original printed it.
        let info = self.info.filter(|_| tab == 6);
        v.show("elaTabFrame(0)", tab != 5 && info.is_none()).show("elaTabFrame(1)", tab == 5).show("picTabPicture", info.is_some());
        if let Some((item, align, id)) = info {
            let lines = item_info(d, item, align, id);
            v.paint("picTabPicture", move |p, r, s| {
                for (row, col, text, style) in &lines {
                    let fam = egui::FontFamily::Name(if *style == 1 { "ms1" } else if *style == 2 { "ms2" } else { "ms0" }.into());
                    let at = r.min + vec2(*col as f32 * 6.0, (*row - 1) as f32 * 13.0) * s;
                    p.text(at, egui::Align2::LEFT_TOP, text, egui::FontId::new(11.0 * s, fam), Color32::BLACK);
                }
            });
        }
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
                let mut r: Vec<(String, String)> = [
                    (0x8ecb_e13c, c.age().to_string()),
                    (0xcdfb_acee, format!("{}/{}", commas(c.sp), commas(c.max_sp(d, t)))),
                    (0x24e4_7b2b, c.level().to_string()),
                    (0xcae5_49bb, format!("{}/{}", commas(c.hp), commas(c.max_hp))),
                    (0x26cb_6418, format!("{a}/{df}")),
                    (0xace3_9749, commas(c.xp[c.guild])),
                    (0x6c94_b6b3, commas(c.gold)),
                ]
                .into_iter()
                .map(|(k, v)| (d.t(k).to_string(), v))
                .collect();
                for (i, &n) in game::STAT_NAMES.iter().enumerate() {
                    r.push((d.t(n).into(), c.stats[i].to_string()));
                }
                r
            }
            // As the original: each resistance (the race's, or an item's or spell's) or " -".
            1 => std::iter::once((d.t(0xe0be_bc84).into(), String::new()))
                .chain(game::RESISTANCES.iter().enumerate().map(|(i, &n)| {
                    let r = ddj_rules::monsters::resist(c, d, i);
                    (d.t(n).into(), if r > 0 { format!("{r}%") } else { d.t(0xa4c5_9e16).into() })
                }))
                .collect(),
            2 => vec![
                ("Race".into(), d.races[c.race].name.clone()),
                ("Sex".into(), d.t(game::SEXES[usize::from(c.female)]).into()),
                ("Alignment".into(), d.t(game::ALIGNMENTS[usize::from(c.alignment)]).into()),
                ("Bank".into(), commas(c.bank)),
                ("Kills".into(), commas(c.kills)),
            ],
            3 => {
                let mut r: Vec<(String, String)> = (0..d.guilds.len()).filter(|&i| c.levels[i] > 0).map(|i| (format!("{} ({})", d.guilds[i].name, c.levels[i]), String::new())).collect();
                for (i, &n) in game::ABILITIES.iter().enumerate() {
                    r.push((d.t(n).into(), format!("{:.0}", c.ability(d, i))));
                }
                r
            }
            _ => {
                let mut r = Vec::new();
                if let Some(m) = c.quest {
                    r.push((format!("{}{}.", d.t(0x6119_fc50), d.monsters[m].name), String::new()));
                }
                if c.buffs.xp_mult > 1.0 {
                    r.push(("Learning".into(), format!("{}x", c.buffs.xp_mult)));
                }
                if c.buffs.protection > 0 {
                    r.push(("Protection".into(), format!("+{}", c.buffs.protection)));
                }
                if c.buffs.levitate {
                    r.push((d.t(0x5480_c8d4).into(), String::new()));
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
                // The resistances: a half row under the heading, then one block.
                top += h + match (tab, i) {
                    (1, 1) | (0 | 2.., 1 | 3) => half,
                    (0 | 2.., 7) => 2 * half,
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
            } else if tab == 1 {
                v.rect("lblStatInfo(0)", [i0[0], top, stat[0] + stat[2] - i0[0], h]);
            }
            let (name, value) = rows.get(usize::from(i)).cloned().unwrap_or_default();
            v.text(&format!("lblStatInfo({i})"), name).text(&format!("lblStat({i})"), value);
        }
        v.show("elaBorder", tab == 0);
        v.show("lblStat(0)", tab != 1);
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
        v.text("lblRaceInfo", format!("{} {} ({})", d.t(game::SEXES[usize::from(c.female)]), d.races[c.race].name, d.t(game::ALIGNMENTS[usize::from(c.alignment)])));
        v.text("lblGuildInfo", format!("{} ({})", d.guilds[c.guild].name, c.level()));
        // "Character 2 of 3", or "You are Alone".
        v.text("elaGroupInfo", if g.party.len() > 1 { format!("{} {}{} {}", d.t(0xec73_40b0), sel + 1, d.t(0xd274_8962), g.party.len()) } else { d.t(0xb952_9b0d).into() });
        // Buffers (mdcharShowBuffer): "1. (Nothing)", "2. Item #3. Potion", or "3. "Fire
        // Ball"" with its spell points ("?" when none).
        let b0 = desk.rect(F, "lblBuffer(0)");
        for i in 0..10u16 {
            if i > 0 {
                v.clone_rect("lblBuffer(0)", i, [b0[0], b0[1] + b0[3] * i32::from(i), b0[2], b0[3]]);
                v.clone_rect("lblBufferStat(0)", i, [desk.rect(F, "lblBufferStat(0)")[0], b0[1] + b0[3] * i32::from(i), 24, b0[3]]);
            }
            let (text, stat) = match c.buffers[usize::from(i)] {
                Some(game::Buf::Item(it)) => match c.items.iter().position(|h| h.item == it) {
                    Some(s) => (format!("{}{}. {}", d.t(0x8896_dc63), s + 1, magic::item_name(d, &c.items[s])), String::new()),
                    None => (d.t(0x02f8_b523).into(), String::new()),
                },
                Some(game::Buf::Spell(s)) => (format!("\"{}\"", d.spells[s].name), match magic::sp_cost(c, d, s) { 0 => "?".into(), n => n.to_string() }),
                None => (d.t(0x02f8_b523).into(), String::new()),
            };
            v.text(&format!("lblBuffer({i})"), format!("{}. {text}", i + 1)).text(&format!("lblBufferStat({i})"), stat);
        }
        let ev = desk.window(ctx, F, v, false);
        if let Some((k, Drag(kind, m, x))) = ev.dropped.clone()
            && m == sel
            && let Some(i) = ["lblBuffer(", "lblBufferStat("].iter().find_map(|b| k.strip_prefix(b)).and_then(|r| r.trim_end_matches(')').parse::<usize>().ok()).filter(|&i| i < 10)
            && let Some(g) = self.game.as_mut()
        {
            let c = &mut g.party[sel];
            match kind {
                1 => c.buffers[i] = c.items.get(x).map(|h| game::Buf::Item(h.item)),
                3 => c.buffers[i] = Some(game::Buf::Spell(x)),
                _ => {}
            }
            self.persist();
        }
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
            v.scroll(&k, "vsbItems", item[3] as f32, SLOTS - rows);
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
                v.scroll(&format!("{base}({r})"), "vsbSpells", n0[3] as f32, known.len().saturating_sub(5));
            }
            let sp = row.map(|s| &d.spells[s]);
            v.text(&format!("lblName({r})"), sp.map_or(String::new(), |s| s.name.clone()));
            if let Some(s) = row {
                v.drag(&format!("lblName({r})"), Drag(3, sel, s));
            }
            v.text(&format!("lblGuild({r})"), "").text(&format!("lblLevel({r})"), sp.map_or(String::new(), |s| s.level.to_string())).text(&format!("lblPoints({r})"), row.map_or(String::new(), |s| magic::sp_cost(c, d, s).to_string()));
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
                v.text(&format!("lblCompName({k})"), d.monsters[cp.monster].name.clone()).text(&format!("lblCompHits({k})"), format!("{}\\{}", cp.hp, cp.max_hp));
                if let Some(a) = &self.art {
                    let (id, uv) = a.portrait(d.monsters[cp.monster].picture);
                    v.pic(&format!("picCompanion({k})"), id, uv);
                }
                v.drag(&format!("picCompanion({k})"), Drag(2, sel, k));
            }
        }
        v.text("lblCarryInfo", format!("{} companion{}", c.companions.len(), if c.companions.len() == 1 { "" } else { "s" })).show("cmdCompanion(0)", false);
        let ev = desk.window(ctx, F, v, false);
        // An item dragged from the bank's list comes back into the pack.
        if let Some((_, Drag(6, m, k))) = ev.dropped.clone()
            && m == sel
            && let Some(g) = self.game.as_mut()
        {
            if let Err(e) = g.party[sel].unstash(d, k) {
                self.boxes.push((String::new(), e));
            }
            desk.state("frmCityBank", "lstItems").sel = None;
        }
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
            // As the original: guild items stay, a cursed one worn stays, else it asks.
            (Some(2), Some(i)) => {
                let h = &g.party[sel].items[i];
                if (500..512).contains(&d.items[h.item].w(10)) {
                    msg = d.t(0xd36b_3441).into();
                } else if h.equipped && magic::cursed(d, h, g.party[sel].alignment) {
                    msg = format!("{}{}{}", d.t(0x8d15_b95b), magic::item_name(d, h), d.t(0x3ddd_f38a));
                } else {
                    self.dialog = Some(Dialog::Confirm(format!("{}{}?", d.t(0x4663_7a95), magic::item_name(d, h)), Deed::Drop(sel, i)));
                }
            }
            (Some(3), Some(i)) => {
                let h = &g.party[sel].items[i];
                App::show_info(&mut self.info, desk, h.item, h.align, h.id);
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
    /// Leave, and the remake's < > (the member shown) and Up / Down (the order).
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
        let (b1, b2) = (desk.rect(F, "cmdCommand(1)"), desk.rect(F, "cmdCommand(2)"));
        // The remake's four buttons between Options and a narrower Leave.
        let leave = b2[2] / 2;
        v.rect("cmdCommand(2)", [b2[0] + b2[2] - leave, b2[1], leave, b2[3]]);
        let unit = (b2[0] + b2[2] - leave - b1[0] - b1[2] - 10) / 10;
        let (n, calm) = (g.party.len(), g.fight.is_none());
        let mut x = b1[0] + b1[2] + 2;
        for (k, cap, wide, on) in [(3, "<", 2, n > 1), (4, ">", 2, n > 1), (5, "Up", 3, calm && sel > 1), (6, "Down", 3, calm && sel > 0 && sel + 1 < n)] {
            v.clone_rect("cmdCommand(0)", k, [x, b1[1], unit * wide, b1[3]]);
            v.text(&format!("cmdCommand({k})"), cap).enable(&format!("cmdCommand({k})"), on);
            x += unit * wide + 2;
        }
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
            Some(3) => self.sel = (sel + n - 1) % n,
            Some(4) => self.sel = (sel + 1) % n,
            Some(k @ (5 | 6)) => {
                self.sel = if k == 5 { sel - 1 } else { sel + 1 };
                g.reorder(sel, self.sel);
                self.persist();
            }
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

    /// The AutoMap: what the character playing has worked out of this level.
    pub(crate) fn automap_window(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        let g = self.game.as_ref().unwrap();
        let p = g.pos();
        let mut v = View::default();
        // The level shown: the last known while lost; with Auto Change Lev the party's,
        // else it stays (medDrawCursor).
        let floor = if g.lost.level { g.lost.floor } else if g.lead().panel.auto_level || self.automap_floor == 0 { p.floor } else { self.automap_floor };
        self.automap_floor = floor;
        v.caption = Some(format!("{} {floor})", d.t(0x3ea0_825e)));
        let size = desk.size("frmAutoMap");
        let (w, h) = (size.x as i32, size.y as i32);
        v.show("hsbLevel", false).show("lblLevel", false).show("lblInfo", false);
        v.rect("picPicture", [0, 0, w, h]).rect("picMaze", [0, 0, w - 2, h - 2]);
        let art = self.art.as_ref();
        let blink = !self.opts.no_blink;
        v.paint("picMaze", move |pt, r, s| art::automap(pt, r, s, art, d, g, floor, blink));
        if desk.window(ctx, "frmAutoMap", v, false).closed {
            self.automap_open = false;
        }
    }
}
