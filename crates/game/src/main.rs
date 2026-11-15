#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use ddj_extract::vfs as fs;
use web_time::Instant;

use ddj_rules::{
    Tuning,
    game::{self, Character, Game},
    hall::Hall,
    library::{Library, day},
    mdr::GameData,
};
use eframe::egui::{self, Color32};

mod art;
mod audio;
mod dungeon;
#[cfg(target_os = "android")]
mod android;
mod help;
mod opts;
mod party;
mod stats;
mod sync;
mod town;
#[cfg(any(windows, target_os = "android"))]
mod update;
mod win;
#[cfg(target_arch = "wasm32")]
mod web;
use art::Art;
use win::{Desk, View};

/// The free shareware release (floors 1-3), which its publisher lets anyone pass on.
/// The game starts with it until the player imports their registered copy.
const SHAREWARE: &[u8] = include_bytes!("../../../tests/fixtures/MORDOR11.ZIP");

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Create,
    /// The Character List.
    List,
    Town,
    Dungeon,
}

/// A window open over the current screen.
#[derive(Clone, PartialEq)]
enum Dialog {
    Options,
    About,
    Hall,
    Import,
    /// A message box: title, text.
    Msg(String, String),
    /// "Load which character?" (the original's Pickup Character window).
    Load,
    /// "Delete this character?"
    Delete(String),
    /// A yes/no question before a costly service: the question, then what Yes does.
    Confirm(String, Deed),
    /// The original's "Pickup Character": add a saved character to the party.
    Recruit,
    /// A question for the Seer: the button asked (Character 0, Monster 2, Item 3).
    Seer(u16),
    /// The Morgue's "Character's Name?" for its Raise (0) or rescue (3) button.
    MorgueWho(u16),
    /// The rescue's "Last known coords?" for party member i.
    MorgueWhere(usize),
    Macros(usize),
    /// The Control Panel (F10): the character's options, window control, Game Options.
    Panel,
    /// The original's PASSWORD prompts (fn16A0, masked).
    Pass(Pass),
    /// The guild log of the guild being visited (frmGuildLog).
    GuildLog(usize),
    /// The Library (frmLibrary), with a Search message box over it.
    Library(Option<String>),
    /// Exchange Items: with whom, the exchange itself, and gold dragged from a side.
    ExchangeWho,
    Exchange,
    ExchangeGold(usize),
    /// Give (G): to whom, then how much gold, then which item.
    Give(Option<usize>, Option<u64>),
    /// Teleport or Displacement: the offsets, picked with +/- (the original typed them).
    Teleport,
    /// Cloud saves: the server and the sync code.
    Sync,
    /// A character changed here and on another device: keep which?
    Clash,
}

/// Which password is asked, and what was typed so far.
#[derive(Clone, PartialEq)]
enum Pass {
    /// Creating the named character: the password, then again; a blank one asks first.
    New(String, Option<String>),
    NoPass(String),
    /// Loading (Load Character, 0), adding (Ask to Join, 1) or exchanging with (2) a
    /// character with one.
    Check(String, u8),
    /// Change Password: the old one, then the new one twice.
    Old,
    Change(Option<String>),
}

/// A service done once the player confirms it.
#[derive(Clone, Copy, PartialEq)]
enum Deed {
    Raise(usize),
    /// Party member, the coordinates given (x, y, level).
    Rescue(usize, (i32, i32, i32)),
    /// Party member, guild.
    Join(usize, usize),
    /// Bind the lead's companion tighter.
    Bind(usize),
    /// Party member, item slot.
    Drop(usize, usize),
    /// File for forfeit of a party member's quest.
    Forfeit(usize),
    /// Clean the guild log; its second question asks about GUILDLOG.TXT.
    CleanLog,
    CleanLogText(bool),
    /// Save a new character with stat points left: its password comes next.
    Points,
    /// Download and install a newer release.
    #[cfg(any(windows, target_os = "android"))]
    Update,
}

struct Draft {
    race: usize,
    female: bool,
    alignment: u8,
    stats: [i16; 6],
    name: String,
}

struct App {
    tuning: Result<Tuning, String>,
    data: Option<GameData>,
    /// The original pictures, once imported.
    art: Option<Art>,
    desk: Option<Desk>,
    styled: bool,
    screen: Screen,
    dialog: Option<Dialog>,
    /// Message boxes waiting their turn: title, text.
    boxes: Vec<(String, String)>,
    /// The phone shell's banners for boxes that only tell: title, text, until.
    toasts: Vec<(String, String, f64)>,
    /// The character the menu would run ("Run "Name""), once loaded.
    loaded: Option<String>,
    /// The city building whose window is open.
    building: Option<&'static str>,
    draft: Draft,
    game: Option<Game>,
    paused: bool,
    in_fight: bool,
    last_round: Instant,
    /// The party member shown in the character windows.
    sel: usize,
    rule: party::RuleDraft,
    audio: Option<audio::Audio>,
    /// The web build opens audio after the first click, as browsers require.
    audio_tried: bool,
    /// Log lines already given their sound.
    heard: usize,
    /// The HelpLesson's windows, while open.
    help: Option<help::Viewer>,
    #[cfg(any(windows, target_os = "android"))]
    updater: update::Updater,
    sync: sync::Sync,
    /// The Options window's settings (MORDOR.INI).
    opts: opts::Opts,
    /// Whether the AutoMap window is open (it opens on loading unless "No Load Automap").
    automap_open: bool,
    /// The level the AutoMap shows; with Auto Change Lev off it stays put.
    automap_floor: usize,
    /// The toolbar's last button (Z) shows the character's resistances instead.
    res_bar: bool,
    /// The item the character window's Misc tab tells about: kind, alignment, ID level.
    info: Option<(usize, u8, u8)>,
    /// Exchange Items: the loaded character and the other, until saved or cancelled.
    swap: Option<Box<[Character; 2]>>,
}

/// Clean Guild Log (sub316B): only the last 50 entries stay; the rest may go to
/// GUILDLOG.TXT beside the saves, under who cleaned it.
fn clean_log(g: &mut Game, d: &GameData, text: bool) {
    let cut = g.hall.log.len().saturating_sub(50);
    let gone: Vec<_> = g.hall.log.drain(..cut).collect();
    if text {
        let mut s = format!("\n{}{})\n{}\"{}\"\n\n", d.t(0xc70b_577e), today(), d.t(0x02b5_fb44), g.party[0].name);
        for e in gone {
            s += &format!("{}{}\n{}{}\n{}{}\n", d.t(0xa54b_a70a), d.guilds.get(e.guild).map_or("", |x| x.name.as_str()), d.t(0x5f99_ed69), e.date, d.t(0x7d1f_8b6b), e.text);
        }
        let path = saves().join("GUILDLOG.TXT");
        let old = fs::read(&path).unwrap_or_default();
        let _ = fs::write(&path, [old, s.into_bytes()].concat());
    }
    let _ = g.hall.save(&saves());
}

/// Today as the original's short date (m/d/yyyy, UTC).
fn today() -> String {
    ddj_rules::library::date(web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64 + 719_468)
}

/// The Hall of Records (frmHall, sub32F6): the six stat records down the left, the six
/// feats down the right, each a title over who set it and with what.
fn hall(desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
    const F: &str = "frmHall";
    // The records' titles (sub32F6), by text id.
    const TITLES: [u32; 12] = [
        0xa68a_b0ca, 0x328a_6f32, 0xf7a1_5d2a, 0x1ced_8872, 0x0f3f_3e3f, 0xd163_64c4, 0x333c_9c85, 0xaec6_cc75, 0xa4f5_b4b8, 0xb433_35de, 0x7722_e11f, 0x589c_b0ec,
    ];
    let h = Hall::load(&saves(), String::new());
    let mut v = View::default();
    for (j, r) in h.records.iter().enumerate() {
        let (side, i) = (if j < 6 { "Record" } else { "History" }, (j % 6) as u16);
        let (s0, c0) = (desk.rect(F, &format!("lbl{side}Stat(0)")), desk.rect(F, &format!("lbl{side}Char(0)")));
        let top = (s0[3] + c0[3]) * i32::from(i);
        v.clone_rect(&format!("lbl{side}Stat(0)"), i, [s0[0], s0[1] + top, s0[2], s0[3]]);
        v.clone_rect(&format!("lbl{side}Char(0)"), i, [c0[0], c0[1] + top, c0[2], c0[3]]);
        let set = r.date.as_ref().map_or(d.t(0xe302_134e).into(), |t| format!("{}{t}{}", d.t(0xd39e_c18c), d.t(0x4755_4eae)));
        let what = match j {
            0..6 => format!("{}{} {}", d.t(0xf927_bada), r.value, d.t(game::STAT_NAMES[j])),
            6 => format!("{}\"{}\"", d.t(0xa36f_fc2c), d.monsters.iter().find(|m| i64::from(m.id) == r.value).map_or(d.t(0xe277_bc82), |m| &m.name)),
            7 => format!("{}{}{}", d.t(0x4f89_85d4), stats::commas(r.value), d.t(0xf6cd_1089)),
            8 => format!("{}{}{}", d.t(0x2213_30c6), stats::commas(r.value), d.t(0x39b4_b767)),
            9 => format!("{}{}", d.t(0x3254_fc96), r.value),
            10 => format!("{}{}", d.t(0xc408_12fd), r.value),
            _ => format!("{}{}", d.t(0x21c6_6425), r.value),
        };
        v.text(&format!("lbl{side}Stat({i})"), d.t(TITLES[j])).text(&format!("lbl{side}Char({i})"), format!("{set}\"{}\"\n{what}", h.holder(d, j)));
    }
    let ev = desk.window(ctx, F, v, true);
    ev.click("cmdCommand(0)") || ev.closed
}

fn saves() -> PathBuf {
    ddj_extract::home().join("saves")
}

/// Player patches: every `patches/*.toml` except the built-in `modern.toml`, by name.
fn load_tuning() -> Result<Tuning, String> {
    let mut paths = fs::read_dir(ddj_extract::home().join("patches")).unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "toml") && !p.ends_with("modern.toml"));
    paths.sort();
    let texts: Vec<String> = paths.iter().filter_map(|p| String::from_utf8(fs::read(p).ok()?).ok()).collect();
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
        let dir = ddj_extract::data_dir();
        let mut data = GameData::load(&dir).ok();
        if data.is_none() {
            let _ = ddj_extract::import_files(vec![("MORDOR11.ZIP".into(), SHAREWARE.to_vec())], &dir);
            data = GameData::load(&dir).ok();
        }
        // Data imported before the remake read the original's text: take it from the
        // bundled shareware's MORDOR.EXE.
        if data.as_ref().is_some_and(|d| d.text.is_empty()) {
            let _ = ddj_extract::import_only(vec![("MORDOR11.ZIP".into(), SHAREWARE.to_vec())], &dir, "MORDOR.EXE");
            data = GameData::load(&dir).ok();
        }
        let opts = opts::Opts::load();
        win::FLAT.store(opts.no_3d, std::sync::atomic::Ordering::Relaxed);
        // The Control Panel's Preload Char.: that character waits in the menu.
        let loaded = opts.preload.clone().filter(|n| game::saved(&saves()).contains(n));
        Self {
            tuning: load_tuning(),
            data,
            art: None,
            desk: None,
            styled: false,
            screen: Screen::Menu,
            dialog: None,
            boxes: Vec::new(),
            toasts: Vec::new(),
            loaded,
            building: None,
            draft: Draft { race: 0, female: false, alignment: 1, stats: [0; 6], name: String::new() },
            game: None,
            paused: false,
            in_fight: false,
            last_round: Instant::now(),
            sel: 0,
            rule: party::RuleDraft::default(),
            audio: None,
            audio_tried: false,
            heard: 0,
            help: None,
            #[cfg(any(windows, target_os = "android"))]
            updater: update::Updater::start(&saves(), &today()),
            sync: sync::Sync::load(),
            opts,
            automap_open: true,
            automap_floor: 0,
            res_bar: false,
            swap: None,
            info: None,
        }
    }

    /// A message: into the Messages window while playing, else a message box.
    fn say(&mut self, s: impl Into<String>) {
        let s = s.into();
        if s.is_empty() {
            return;
        }
        match self.game.as_mut() {
            Some(g) => g.log.push(s),
            None => self.dialog = Some(Dialog::Msg("Mordor".into(), s)),
        }
    }

    /// Opens the HelpLesson in the window named, at the topic for a HelpContextID (the
    /// contents with none), as the original's WinHelp calls did.
    fn help(&mut self, desk: &mut Desk, win: &str, id: Option<i64>) {
        if self.help.is_none() {
            self.help = help::Viewer::load(&ddj_extract::data_dir(), SHAREWARE);
        }
        match self.help.as_mut() {
            Some(h) => {
                let (w, t) = (h.window(win), id.map_or(h.contents(), |i| h.context(i)));
                h.go(desk, w, t);
            }
            None => self.say("The HelpLesson could not be read from MORDOR.HLP."),
        }
    }

    /// Saves every party member after each change; the files are small.
    fn persist(&mut self) {
        let Some(g) = &mut self.game else { return };
        g.store.saved = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut err = None;
        for m in &g.party {
            if let Err(e) = m.save(&saves()) {
                err = Some(format!("Could not save {}: {e}", m.name));
            }
        }
        if let Err(e) = g.store.save(&saves()) {
            err = Some(format!("Could not save the store: {e}"));
        }
        if let Err(e) = g.hall.save(&saves()) {
            err = Some(format!("Could not save the Hall of Records: {e}"));
        }
        if let Err(e) = g.library.save(&saves()) {
            err = Some(format!("Could not save the Library: {e}"));
        }
        self.sync.changed();
        if let Some(e) = err {
            self.say(e);
        }
    }

    /// Starts play with a character, bringing its allies along when they can come.
    fn start(&mut self, mut ch: Character, d: &GameData) {
        ch.upgrade_map(d);
        self.automap_open = !self.opts.no_load_automap;
        let allies = ch.allies.clone();
        let mut g = Game::new(ch);
        g.store = ddj_rules::store::Store::load(&saves(), d);
        g.hall = Hall::load(&saves(), today());
        g.hall.seed(d);
        g.library = Library::load(&saves(), today(), d, &mut g.party[0]);
        for name in allies {
            if let Ok(a) = load(&name)
                && let Err(e) = g.add_ally(d, a)
            {
                g.log.push(e);
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
        self.building = None;
        self.screen = Screen::Town;
    }

    /// The main window's title and toolbar: the original's tool buttons, gold and
    /// experience.
    fn main_window(&mut self, desk: &mut Desk, ui: &mut egui::Ui, d: &GameData, t: &Tuning) {
        // The original's title: its name and version, "(Limited Version)" for the shareware.
        let title = format!("Mordor 1.1{}", if d.floors.len() < 15 { d.t(0xd92c_9a48) } else { "" });
        let mut v = View::default();
        // The toolbar: hidden in design, shown across the top while playing, with the
        // party's gold and the experience still needed at its right.
        v.show("picToolbar", self.game.is_some() && !self.opts.no_toolbar).rect("picToolbar", [0, 0, desk.screen.x as i32, 32]).show("mciMusic", false).show("picPalette", false);
        if let Some(g) = &self.game {
            let c = &g.party[self.sel.min(g.party.len() - 1)];
            let gold: u64 = g.party.iter().map(|m| m.gold).sum();
            let need = c.xp_needed(d, t).saturating_sub(c.xp[c.guild]);
            let (lg, le) = (desk.rect("frmMain", "lblGold"), desk.rect("frmMain", "lblExperience"));
            let right = desk.screen.x as i32 - 4;
            v.rect("lblGold", [right - lg[2], lg[1], lg[2], lg[3]]).rect("lblExperience", [right - le[2], le[1], le[2], le[3]]);
            v.text("lblGold", format!("{}{}{}", d.t(0x336b_5380), stats::commas(gold), d.t(0xf4e9_3569)));
            // Past the next level's experience, the way to the pin (sub1617): one short of
            // the level after next.
            let xp = c.xp[c.guild];
            let pin = t.xp_needed(c.level() + 1, d.races[c.race].xp_factor, d.guilds[c.guild].xp_factor).saturating_sub(1);
            v.text("lblExperience", match () {
                _ if need > 0 => format!("{}{}{}{}.", d.t(0x9ca1_2838), stats::commas(need), d.t(0x87e5_d3e9), c.level() + 1),
                _ if xp >= pin => d.t(0xb99a_8146).to_string(),
                _ => format!("{}{}{}", d.t(0x9ca1_2838), stats::commas(pin - xp), d.t(0xb26a_ca3e)),
            });
        }
        let playing = self.game.is_some();
        // The resistance bar (sub152F): the eleven resistances' first three letters over
        // the controlled character's values, or " -", in place of the buttons.
        if self.res_bar
            && let Some(g) = &self.game
        {
            let c = &g.party[self.sel.min(g.party.len() - 1)];
            let cols: Vec<(String, String)> = game::RESISTANCES.iter().enumerate().map(|(i, &n)| (d.t(n).chars().take(3).collect(), match ddj_rules::monsters::resist(c, d, i) { 0 => " -".into(), r => r.to_string() })).collect();
            for i in 0..8 {
                v.show(&format!("cmdTool({i})"), false);
            }
            v.paint("picToolbar", move |p, r, s| {
                let font = egui::FontId::monospace(11.0 * s);
                for (i, (name, val)) in cols.iter().enumerate() {
                    let x = r.left() + (7 + 4 * i) as f32 * 7.0 * s;
                    p.text(egui::pos2(x, r.top() + 2.0 * s), egui::Align2::LEFT_TOP, name, font.clone(), Color32::BLACK);
                    p.text(egui::pos2(x + 0.5, r.top() + 2.0 * s), egui::Align2::LEFT_TOP, name, font.clone(), Color32::BLACK);
                    p.text(egui::pos2(x, r.top() + 16.0 * s), egui::Align2::LEFT_TOP, val, font.clone(), Color32::BLACK);
                }
            });
        }
        for i in [1, 2, 3, 4, 5, 7, 8] {
            v.enable(&format!("cmdTool({i})"), playing);
        }
        // Leave (L) only puts down a carried character, which the remake doesn't carry.
        v.enable("cmdTool(6)", false);
        let ev = desk.main(ui, &title, v);
        match ev.index("cmdTool") {
            // The HelpLesson; F5: Objects & Spells; F8: the AutoMap; 'A': add a character; F10: options.
            Some(0) => self.help(desk, "main", None),
            Some(1) => desk.active = Some("frmCharStat3".into()),
            Some(2) => {
                self.automap_open = true;
                desk.active = Some("frmAutoMap".into());
            }
            Some(3) => self.dialog = Some(Dialog::Recruit),
            Some(5) => {
                // 'P': pool the party's gold to the character shown.
                if let Some(g) = self.game.as_mut() {
                    let sel = self.sel.min(g.party.len() - 1);
                    let msg = if g.party.len() == 1 {
                        d.t(0x6a6b_6533).into()
                    } else if g.fight.is_some() {
                        d.t(0x5df7_32e1).into()
                    } else {
                        let pooled: u64 = g.party.iter_mut().enumerate().filter(|(i, _)| *i != sel).map(|(_, m)| std::mem::take(&mut m.gold)).sum();
                        g.party[sel].gold += pooled;
                        format!("{}\"{}\"{}", d.t(0x2928_069c), g.party[sel].name, d.t(0xb0af_31db))
                    };
                    g.log.push(msg);
                    self.persist();
                }
            }
            Some(7) => self.dialog = Some(Dialog::Panel),
            Some(8) => self.res_bar = !self.res_bar,
            // 'G': give gold and an item to another member; nothing while paralysed.
            Some(4) => {
                if let Some(g) = self.game.as_mut() {
                    let sel = self.sel.min(g.party.len() - 1);
                    if g.party.len() == 1 {
                        g.log.push(d.t(0xcff8_3abb).into());
                    } else if g.party[sel].status.paralysis == 0 {
                        desk.st.retain(|k, _| !k.starts_with("frmInteract."));
                        self.dialog = Some(Dialog::Give(None, None));
                    }
                }
            }
            _ => {}
        }
    }

    /// The opening menu (frmMenu); behind the character list and creation it only shows.
    /// As the original's sub2D2E: with a character loaded the first button runs it and
    /// the load button unloads; Delete, Exchange and Password show only then.
    fn menu(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        let mut v = View::default();
        match &self.loaded {
            Some(name) => v.text("cmdCommand(0)", format!("{}\"{name}\"", d.t(0x01d8_d032))).text("cmdCommand(7)", d.t(0xb029_33cd)),
            None => v.text("cmdCommand(0)", d.t(0x976f_efad)).text("cmdCommand(7)", d.t(0xf9fb_4a36)),
        };
        for i in [1, 4, 5] {
            v.show(&format!("cmdCommand({i})"), self.loaded.is_some());
        }
        let outer = desk.outer("frmMenu");
        let at = desk.client.center() - outer / 2.0;
        desk.pos.entry("frmMenu".into()).or_insert(at);
        let ev = desk.window(ctx, "frmMenu", v, false);
        if self.screen != Screen::Menu || self.dialog.is_some() {
            return;
        }
        match ev.index("cmdCommand") {
            Some(0) => match self.loaded.clone() {
                Some(name) => match load(&name) {
                    Ok(ch) => self.start(ch, d),
                    Err(e) => self.say(format!("Could not load {name}: {e}")),
                },
                None => {
                    let stats = game::start_stats(d, 0);
                    self.draft = Draft { race: 0, female: false, alignment: 1, stats, name: String::new() };
                    desk.st.retain(|k, _| !k.starts_with("frmCreateChar."));
                    self.screen = Screen::Create;
                }
            },
            Some(7) if self.loaded.is_some() => self.loaded = None,
            Some(7) => self.dialog = Some(Dialog::Load),
            Some(4) if self.loaded.is_some() => {
                desk.st.retain(|k, _| !k.starts_with("frmInteract."));
                self.dialog = Some(Dialog::ExchangeWho);
            }
            Some(1) if self.loaded.is_some() => {
                desk.st.retain(|k, _| !k.starts_with("frmInteract."));
                self.dialog = Some(Dialog::Pass(Pass::Old));
            }
            Some(9) => self.screen = Screen::List,
            Some(3) => self.help(desk, "main", None),
            Some(10) => self.dialog = Some(Dialog::Hall),
            Some(5) => self.dialog = self.loaded.clone().map(Dialog::Delete),
            Some(6) => self.dialog = Some(Dialog::Options),
            Some(8) => self.dialog = Some(Dialog::About),
            Some(2) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
    }

    /// The Character List (frmCharList).
    fn list(&mut self, desk: &mut Desk, ctx: &egui::Context) {
        let names = game::saved(&saves());
        let mut v = View::default();
        v.items("lstCharacters", names.clone()).text("lblNumber", names.len().to_string());
        let ev = desk.window(ctx, "frmCharList", v, true);
        if ev.click("cmdExit") || ev.closed {
            self.screen = Screen::Menu;
        }
    }

    /// "Load which character?": the original's Pickup Character window.
    fn load_dialog(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        let names = game::saved(&saves());
        // The first character comes up chosen, as in the original.
        let st = desk.state("frmInteract", "cmbCharacter");
        st.sel = st.sel.filter(|&i| i < names.len()).or((!names.is_empty()).then_some(0));
        let mut v = View::default();
        v.caption = Some(d.t(0xfbee_e2ea).into());
        v.text("lblQuestion", d.t(0x2df5_bd1c)).show("txtQuestion", false).show("cmbCharacter", true).items("cmbCharacter", names.clone());
        let ev = desk.window(ctx, "frmInteract", v, true);
        if ev.click("cmdOK")
            && let Some(name) = desk.state("frmInteract", "cmbCharacter").sel.and_then(|i| names.get(i)).cloned()
        {
            if load(&name).is_ok_and(|c| !c.password.trim().is_empty()) {
                desk.st.retain(|k, _| !k.starts_with("frmInteract."));
                self.dialog = Some(Dialog::Pass(Pass::Check(name, 0)));
            } else {
                self.loaded = Some(name);
            }
        }
        ev.click("cmdOK") || ev.click("cmdCancel") || ev.closed
    }

    /// Character creation (frmCreateChar).
    fn create(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        const F: &str = "frmCreateChar";
        if desk.state(F, "cmbSex").sel.is_none() {
            desk.state(F, "cmbSex").sel = Some(0);
            desk.state(F, "cmbRace").sel = Some(0);
        }
        let dr = &mut self.draft;
        let race = desk.state(F, "cmbRace").sel.unwrap_or(0).min(d.races.len() - 1);
        if race != dr.race {
            dr.race = race;
            dr.stats = game::start_stats(d, race);
        }
        dr.female = desk.state(F, "cmbSex").sel == Some(1);
        let aligns: Vec<u8> = (0..3).filter(|a| d.races[race].alignments & (1 << a) != 0).collect();
        let a = desk.state(F, "cmbAlignment").sel.unwrap_or(aligns.iter().position(|&a| a == 1).unwrap_or(0)).min(aligns.len().saturating_sub(1));
        desk.state(F, "cmbAlignment").sel = Some(a);
        dr.alignment = aligns.get(a).copied().unwrap_or(1);
        let r = &d.races[race];
        let base = game::start_stats(d, race);
        let left = r.stat_points - dr.stats.iter().zip(base).map(|(s, b)| s - b).sum::<i16>();
        let mut v = View::default();
        v.items("cmbRace", d.races.iter().map(|r| r.name.clone()).collect());
        v.items("cmbSex", game::SEXES.iter().map(|&s| d.t(s).to_string()).collect());
        v.items("cmbAlignment", aligns.iter().map(|&a| d.t(game::ALIGNMENTS[usize::from(a)]).to_string()).collect());
        v.text("lblPoints", left.to_string());
        // A stat may go down to the race's minimum, each step a point back, as in the original.
        for i in 0..6 {
            v.text(&format!("pnlPanel({i})"), dr.stats[i].to_string()).text(&format!("lblStatInfo({i})"), format!("({}-{})", r.stat_min[i], r.stat_max[i]));
            v.enable(&format!("cmdAdd({i})"), left > 0 && dr.stats[i] < r.stat_max[i]).enable(&format!("cmdSubtract({i})"), dr.stats[i] > r.stat_min[i]);
            // On touch, row-high buttons either side of the number: down, value, up, range.
            if desk.touch {
                let y = desk.rect(F, &format!("pnlPanel({i})"))[1];
                v.rect(&format!("cmdSubtract({i})"), [81, y - 4, 23, 26]).rect(&format!("pnlPanel({i})"), [105, y, 21, 18]);
                v.rect(&format!("cmdAdd({i})"), [127, y - 4, 23, 26]).rect(&format!("lblStatInfo({i})"), [152, y + 2, 33, 16]);
            }
        }
        // The guilds, one a line; those the character could join now in black.
        let g0 = desk.rect(F, "lblGuild(0)");
        for (i, gd) in d.guilds.iter().enumerate() {
            let k = format!("lblGuild({i})");
            if i > 0 {
                v.clone_rect("lblGuild(0)", i as u16, [g0[0], g0[1] + g0[3] * i as i32, g0[2], g0[3]]);
            }
            let ok = gd.races & (1 << race) != 0 && gd.alignments & (1 << dr.alignment) != 0 && (0..6).all(|s| dr.stats[s] >= gd.stat_req[s]);
            v.text(&k, gd.name.clone()).fore(&k, if ok { Color32::BLACK } else { Color32::from_gray(128) });
        }
        let ev = desk.window(ctx, F, v, true);
        if let Some(i) = ev.index("cmdAdd") {
            dr.stats[i as usize] += 1;
        }
        if let Some(i) = ev.index("cmdSubtract") {
            dr.stats[i as usize] -= 1;
        }
        let (female, alignment, stats) = (dr.female, dr.alignment, dr.stats);
        // The races' and guilds' statistics, and the tutorial, in the help's own windows.
        match ev.index("cmdHelp") {
            Some(i) => self.help(desk, "stats", Some(14 + i64::from(i))),
            None if ev.click("cmdTutorial") => self.help(desk, "tutorial", Some(3)),
            None => {}
        }
        if ev.click("cmdExit") || ev.closed {
            self.screen = Screen::Menu;
        }
        let name = desk.state(F, "txtName").text.trim().to_string();
        // Enter saves at a keyboard; on a touch screen it only puts the keyboard away.
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if enter && desk.touch {
            desk.focus = None;
        }
        if ev.click("cmdSave") || (enter && !desk.touch && !name.is_empty()) {
            if name.is_empty() {
                return self.boxes.push((d.t(0x921e_0cc4).into(), d.t(0xe39e_7d93).into()));
            }
            if game::saved(&saves()).contains(&name) {
                return self.boxes.push((d.t(0x1606_cd29).into(), format!("{}\"{name}\"{}\n{}", d.t(0x2455_44d2), d.t(0x2e4d_ab6e), d.t(0xe341_caf9))));
            }
            if let Err(e) = Character::new(d, &name, race, female, alignment, stats) {
                return self.say(e);
            }
            // As the original: points left ask first, then the password (cmdSave_Click).
            desk.st.retain(|k, _| !k.starts_with("frmInteract."));
            self.draft.race = race;
            self.draft.name.clone_from(&name);
            self.dialog = Some(Dialog::Pass(Pass::New(name.clone(), None)));
            if left > 0 {
                self.dialog = Some(Dialog::Confirm(d.t(0x7fd4_6ad5).into(), Deed::Points));
            }
        }
    }

    /// The draft saved as a new character with its password, loaded at the Main Menu.
    fn save_new(&mut self, d: &GameData, name: &str, password: &str) {
        let dr = &self.draft;
        match Character::new(d, name, dr.race, dr.female, dr.alignment, dr.stats) {
            Ok(mut ch) => {
                ch.password = password.trim().to_string();
                match ch.save(&saves()) {
                    Ok(()) => {
                        let mut h = Hall::load(&saves(), today());
                        h.seed(d);
                        h.note(ch.guild, format!("\"{}\"{}", ch.name, d.t(0x88f9_b2cb)));
                        let _ = h.save(&saves());
                        self.sync.changed();
                        self.loaded = Some(name.into());
                        self.screen = Screen::Menu;
                    }
                    Err(e) => self.say(format!("Could not save {name}: {e}")),
                }
            }
            Err(e) => self.say(e),
        }
    }

    /// The PASSWORD prompts: creating (twice, and a blank one asks NO PASSWORD), loading
    /// or adding a character, and Change Password.
    fn password(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, p: Pass) -> bool {
        const F: &str = "frmInteract";
        if let Pass::NoPass(name) = &p {
            let mut v = View::default();
            let q = format!("{}\n{}", d.t(0x7812_81ea), d.t(0x89a2_4f8a));
            desk.fit_msg(&q);
            v.text("lblText", q).text("cmdOK", "&Yes").show("cmdNo", true);
            v.caption = Some(d.t(0x0f89_86f3).into());
            let ev = desk.window(ctx, "dlgMsg", v, true);
            if ev.click("cmdOK") {
                self.save_new(d, &name.clone(), "");
            }
            return ev.click("cmdOK") || ev.click("cmdNo") || ev.closed;
        }
        let (caption, prompt) = match &p {
            Pass::New(_, first) | Pass::Change(first) => (if matches!(p, Pass::New(..)) { 0x9e7a_513e } else { 0x39ff_6728 }, if first.is_none() { 0x88bb_a11a } else { 0x7f9c_1ec8 }),
            Pass::Check(_, why) => (if *why == 1 { 0xb5bc_a695 } else { 0xfbee_e2ea }, 0x8063_209e),
            _ => (0x81c9_aa8e, 0xcc04_8ac1),
        };
        let mut v = View::default();
        v.caption = Some(d.t(caption).into());
        v.text("lblQuestion", d.t(prompt)).mask("txtQuestion");
        let ev = desk.window(ctx, F, v, true);
        if !(ev.click("cmdOK") || ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
            return ev.click("cmdCancel") || ev.closed;
        }
        let typed = std::mem::take(&mut desk.state(F, "txtQuestion").text);
        let boxed = |s: &mut Self, title: u32, text: u32| s.boxes.push((d.t(title).into(), d.t(text).into()));
        match p {
            Pass::New(name, None) => self.dialog = Some(Dialog::Pass(Pass::New(name, Some(typed)))),
            Pass::Change(None) => self.dialog = Some(Dialog::Pass(Pass::Change(Some(typed)))),
            Pass::New(name, Some(first)) if first != typed => {
                boxed(self, 0xdf22_b531, 0xf8d3_3109);
                self.dialog = Some(Dialog::Pass(Pass::New(name, None)));
            }
            Pass::Change(Some(first)) if first != typed => {
                boxed(self, 0xdf22_b531, 0xf8d3_3109);
                self.dialog = Some(Dialog::Pass(Pass::Change(None)));
            }
            Pass::New(name, Some(_)) if typed.trim().is_empty() => self.dialog = Some(Dialog::Pass(Pass::NoPass(name))),
            Pass::New(name, Some(_)) => {
                self.save_new(d, &name, &typed);
                return true;
            }
            Pass::Check(name, why) => {
                match load(&name) {
                    Ok(c) if c.password.trim() == typed.trim() && why == 2 => self.open_exchange(d, c),
                    Ok(c) if c.password.trim() == typed.trim() => {
                        if why == 1 {
                            let msg = match self.game.as_mut() {
                                Some(_) if c.panel.hide => format!("\"{}\"{}", c.name, d.t(0x83d1_afb4)),
                                Some(g) => g.add_ally(d, c).err().unwrap_or_default(),
                                None => String::new(),
                            };
                            self.say(msg);
                            self.persist();
                        } else {
                            self.loaded = Some(name);
                        }
                    }
                    _ => boxed(self, 0x9fa0_af51, 0x97f0_ad8c),
                }
                return true;
            }
            Pass::Old => match self.loaded.clone().and_then(|n| load(&n).ok()) {
                Some(c) if c.password.trim() == typed.trim() => self.dialog = Some(Dialog::Pass(Pass::Change(None))),
                _ => {
                    boxed(self, 0x81c9_aa8e, 0x97f0_ad8c);
                    return true;
                }
            },
            Pass::Change(Some(_)) => {
                if let Some(mut c) = self.loaded.clone().and_then(|n| load(&n).ok()) {
                    c.password = typed.trim().to_string();
                    let _ = c.save(&saves());
                    self.sync.changed();
                    boxed(self, 0x81c9_aa8e, 0x6325_90c0);
                }
                return true;
            }
            Pass::NoPass(_) => {}
        }
        desk.state(F, "txtQuestion").text.clear();
        false
    }

    /// Exchange Items' "Exchange Items with who?": another saved character, its password
    /// asked if it has one.
    fn exchange_who(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        const F: &str = "frmInteract";
        let names: Vec<String> = game::saved(&saves()).into_iter().filter(|n| Some(n) != self.loaded.as_ref()).collect();
        let mut v = View::default();
        v.caption = Some(d.t(0x5c49_74d4).into());
        v.text("lblQuestion", d.t(0xa02e_1119)).show("txtQuestion", false).show("cmbCharacter", true).items("cmbCharacter", names.clone());
        let ev = desk.window(ctx, F, v, true);
        if ev.click("cmdOK")
            && let Some(n) = desk.state(F, "cmbCharacter").sel.and_then(|i| names.get(i)).cloned()
            && let Ok(c) = load(&n)
        {
            desk.st.retain(|k, _| !k.starts_with("frmInteract."));
            if c.password.trim().is_empty() {
                self.open_exchange(d, c);
            } else {
                self.dialog = Some(Dialog::Pass(Pass::Check(n, 2)));
            }
            return false;
        }
        ev.click("cmdOK") || ev.click("cmdCancel") || ev.closed
    }

    /// The two must stand on the same level and in the same area.
    fn open_exchange(&mut self, d: &GameData, other: Character) {
        let Some(me) = self.loaded.clone().and_then(|n| load(&n).ok()) else { return };
        let area = |c: &Character| d.floors.get(c.pos.floor.wrapping_sub(1)).map_or(0, |f| f.square(c.pos.x, c.pos.y).area);
        if me.pos.floor != other.pos.floor || area(&me) != area(&other) {
            self.boxes.push((d.t(0x5c49_74d4).into(), d.t(0x533a_2bbc).into()));
            return;
        }
        self.swap = Some(Box::new([me, other]));
        self.dialog = Some(Dialog::Exchange);
    }

    /// frmExchange: the two characters' names, gold and forty items side by side; drag an
    /// item onto the other's to swap them, the gold onto the other's to give some.
    fn exchange(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        const F: &str = "frmExchange";
        let Some(sw) = self.swap.as_mut() else { return true };
        let mut v = View::default();
        let row = desk.rect(F, "pnlItem(0)");
        for side in 0..2usize {
            let c = &sw[side];
            v.text(&format!("pnlName({side})"), format!("\"{}\"", c.name));
            // In the city the bank's gold counts too.
            let gold = if c.pos.floor == 0 { c.gold + c.bank } else { c.gold };
            v.text(&format!("pnlGold({side})"), format!("{}{}", d.t(0x0b0d_f5d1), stats::commas(gold)));
            v.drag(&format!("pnlGold({side})"), win::Drag(5, side, 0));
            let rows = 13usize;
            let top = desk.state(F, &format!("vsbItems({side})")).top.min(40 - rows);
            v.rect(&format!("picItem({side})"), [0, -(top as i32) * row[3], row[2], row[3] * 40]);
            for i in 0..40usize {
                let k = 40 * side + i;
                if k != 0 && k != 40 {
                    v.clone_rect(&format!("pnlItem({})", 40 * side), k as u16, [row[0], row[3] * i as i32, row[2], row[3]]);
                }
                let key = format!("pnlItem({k})");
                v.text(&key, format!("{}. {}", i + 1, c.items.get(i).map_or(String::new(), |h| ddj_rules::magic::item_name(d, h))));
                v.scroll(&key, &format!("vsbItems({side})"), row[3] as f32, 40 - rows);
                if i < c.items.len() {
                    v.drag(&key, win::Drag(4, side, i));
                }
            }
        }
        v.text("pnlPanel(2)", format!("{}{}{}", d.t(0x4afb_bf32), d.t(0x0269_a8c1), d.t(0x0ce7_a580)));
        let ev = desk.window(ctx, F, v, true);
        if let Some((k, win::Drag(kind, side, slot))) = ev.dropped.clone() {
            let target = k.strip_prefix("pnlItem(").and_then(|r| r.trim_end_matches(')').parse::<usize>().ok());
            match (kind, target) {
                (4, Some(t)) if t / 40 != side => {
                    let [a, b] = &mut **sw;
                    let (src, dst) = if side == 0 { (a, b) } else { (b, a) };
                    if let Err(e) = game::exchange(d, src, slot, dst, t % 40) {
                        self.boxes.push((d.t(0x6255_8b51).into(), d.t(e).into()));
                    }
                }
                (5, None) if k == format!("pnlGold({})", 1 - side) => {
                    desk.st.retain(|k, _| !k.starts_with("frmInteract."));
                    desk.state("frmInteract", "txtQuestion").text = "0".into();
                    self.dialog = Some(Dialog::ExchangeGold(side));
                    return false;
                }
                _ => {}
            }
        }
        if ev.click("cmdCommand(0)") {
            for c in sw.iter() {
                let _ = c.save(&saves());
            }
            self.sync.changed();
        }
        let done = ev.click("cmdCommand(0)") || ev.click("cmdCommand(1)") || ev.closed;
        if done {
            self.swap = None;
        }
        done
    }

    /// "How Much Gold?" (Transfer Gold), from `side` to the other; in the city the bank
    /// pays what the purse can't.
    fn exchange_gold(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, side: usize) -> bool {
        const F: &str = "frmInteract";
        let mut v = View::default();
        v.caption = Some(d.t(0x7590_b8a0).into());
        v.text("lblQuestion", d.t(0x73f9_4a19));
        let ev = desk.window(ctx, F, v, true);
        let back = |s: &mut Self| s.dialog = Some(Dialog::Exchange);
        if ev.click("cmdCancel") || ev.closed {
            back(self);
            return false;
        }
        if !(ev.click("cmdOK") || ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
            return false;
        }
        let n: u64 = std::mem::take(&mut desk.state(F, "txtQuestion").text).trim().parse().unwrap_or(0);
        if let Some(sw) = self.swap.as_mut()
            && n > 0
        {
            let src = &sw[side];
            let have = if src.pos.floor == 0 { src.gold + src.bank } else { src.gold };
            if n > have {
                self.boxes.push((d.t(0x7590_b8a0).into(), d.t(0xd91e_67a5).into()));
            } else {
                sw[side].pay(n);
                sw[1 - side].gold += n;
            }
        }
        back(self);
        false
    }

    /// frmGuildLog: "Log of the" guild, newest first ("TODAY: ", "3 DAYS AGO: "); with
    /// more than 50 entries a guild in the whole log, Clean Guild Log keeps the last 50
    /// and may write the rest to GUILDLOG.TXT.
    fn guild_log(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, guild: usize) -> bool {
        const F: &str = "frmGuildLog";
        let Some(g) = self.game.as_mut() else { return true };
        let today = day(&today());
        let lines: Vec<String> = g.hall.log.iter().rev().filter(|e| e.guild == guild).map(|e| {
            let ago = today.zip(day(&e.date)).map_or(0, |(a, b)| a - b);
            let when = match ago {
                ..=0 => d.t(0xc34e_4964).to_string(),
                1 => format!("{ago}{}", d.t(0x4166_4c44)),
                _ => format!("{ago}{}", d.t(0x1764_9e43)),
            };
            format!("{when}{}", e.text)
        }).collect();
        let n = lines.len();
        let mut v = View::default();
        v.caption = Some(format!("{}{}", d.t(0x6451_7b83), d.guilds[guild].name));
        v.items("lstLog", if n == 0 { vec![d.t(0x141c_4a1e).into()] } else { lines }).text("lblEntries", format!("{n}{}", d.t(0x8bd8_1b85)));
        v.show("cmdClean", g.hall.log.len() > 50 * d.guilds.len());
        let ev = desk.window(ctx, F, v, true);
        if ev.click("cmdClean") {
            self.dialog = Some(Dialog::Confirm(format!("{}50{}", d.t(0x9955_5660), d.t(0x838c_8042)), Deed::CleanLog));
            return false;
        }
        ev.click("cmdDone") || ev.closed
    }

    /// The Library (frmLibrary, sub327E): the recorded monsters (or items), sorted, with
    /// "There are 12 Monster Entries."; the selected one's page (fn1BBE) and a monster's
    /// picture. Enter in Search For: picks the one entry starting with what was typed;
    /// `msg` is its message box ("No match was found.", "ambiguous!").
    fn library(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, msg: Option<String>) -> bool {
        const F: &str = "frmLibrary";
        let Some(g) = self.game.as_ref() else { return true };
        if !desk.state(F, "open").on {
            desk.state(F, "open").on = true;
            desk.state(F, "optOption(1)").on = true;
            desk.state(F, "optOption(0)").on = false;
            desk.state(F, "lstList").sel = Some(0);
        }
        let monsters = desk.state(F, "optOption(1)").on;
        let name = |k: usize| if monsters { d.monsters[k].name.clone() } else { d.items[k].name.clone() };
        let mut list: Vec<(String, usize)> = g.library.listed(monsters).map(|(k, _)| (name(k), k)).collect();
        list.sort_by_key(|x| x.0.to_uppercase());
        let n = list.len();
        let names: Vec<String> = if n == 0 { vec![d.t(0x6648_399a).into()] } else { list.iter().map(|x| x.0.clone()).collect() };
        let pick = desk.state(F, "lstList").sel.and_then(|s| list.get(s)).map(|x| x.1);
        let mut v = View::default();
        v.items("lstList", names.clone()).enable("lstList", n > 0).show("picMonster", monsters);
        v.text("lblEntries", format!("{}{n}{}", d.t(0xa1b7_a18d), d.t(if monsters { 0x7474_3577 } else { 0x179c_23a2 })));
        v.text("txtInfo", pick.map_or(String::new(), |k| if monsters { g.library.monster_page(d, k) } else { g.library.item_page(d, k) }));
        if let (true, Some(k), Some(a)) = (monsters, pick, &self.art) {
            let (id, uv) = a.face(&d.monsters[k]);
            v.pic("picMonster", id, uv);
        }
        let ev = desk.window(ctx, F, v, true);
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if let Some(text) = msg {
            let mut v = View::default();
            v.caption = Some(d.t(0xc646_a2c9).into());
            desk.fit_msg(&text);
            v.text("lblText", text);
            let ev = desk.window(ctx, "dlgMsg", v, true);
            if ev.click("cmdOK") || ev.closed || enter {
                self.dialog = Some(Dialog::Library(None));
            }
            return false;
        }
        if let Some(k) = ev.index("optOption") {
            desk.state(F, &format!("optOption({})", 1 - k)).on = false;
            desk.state(F, "lstList").sel = Some(0);
            desk.state(F, "lstList").top = 0;
            desk.state(F, "txtInfo").top = 0;
        }
        if ev.changed.as_deref() == Some("lstList") {
            desk.state(F, "txtInfo").top = 0;
        }
        if enter && desk.focus.as_deref() == Some("frmLibrary.txtSearch") {
            let typed = desk.state(F, "txtSearch").text.to_uppercase();
            let hits: Vec<usize> = (0..names.len()).filter(|&i| names[i].to_uppercase().starts_with(&typed)).collect();
            match hits[..] {
                [one] => {
                    desk.state(F, "lstList").sel = Some(one);
                    desk.state(F, "txtInfo").top = 0;
                    desk.state(F, "txtSearch").text.clear();
                }
                [] => self.dialog = Some(Dialog::Library(Some(d.t(0xbaba_64a4).into()))),
                _ => self.dialog = Some(Dialog::Library(Some(d.t(0x5d0f_f502).into()))),
            }
        }
        let done = ev.click("cmdCommand(0)") || ev.closed;
        if done {
            desk.state(F, "open").on = false;
        }
        done
    }

    /// The window over everything else, if any.
    fn dialog(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) {
        if let Some(g) = self.game.as_mut() {
            self.boxes.extend(g.boxes.drain(..).map(|(t, s)| (d.t(t).into(), s)));
        }
        // The phone shell while playing: a box that only tells shows as a banner for a
        // while (a tap puts it away), its words kept in Messages.
        let now = ctx.input(|i| i.time);
        if desk.shell
            && let Some(g) = self.game.as_mut()
        {
            if let Some(Some(Dialog::Msg(t, s))) = matches!(self.dialog, Some(Dialog::Msg(..))).then(|| self.dialog.take()) {
                self.boxes.insert(0, (t, s));
            }
            for (t, s) in self.boxes.drain(..) {
                g.log.push(s.clone());
                self.toasts.push((t, s.clone(), now + 2.5 + s.len() as f64 * 0.04));
            }
        }
        self.toasts.retain(|t| t.2 > now);
        if let Some(n) = desk.toasts(ctx, &self.toasts) {
            self.toasts.remove(n);
        }
        if let Some(t) = self.toasts.iter().map(|t| t.2).reduce(f64::min) {
            ctx.request_repaint_after_secs((t - now) as f32);
        }
        if self.dialog.is_none() && !self.boxes.is_empty() {
            let (title, text) = self.boxes.remove(0);
            self.dialog = Some(Dialog::Msg(title, text));
        }
        if self.dialog.is_none() && self.game.as_ref().is_some_and(|g| g.teleport_pending.is_some()) {
            self.dialog = Some(Dialog::Teleport);
        }
        let Some(dlg) = self.dialog.clone() else { return };
        let close = match dlg.clone() {
            Dialog::Msg(title, text) => {
                let mut v = View::default();
                v.caption = Some(title);
                desk.fit_msg(&text);
                v.text("lblText", text);
                let ev = desk.window(ctx, "dlgMsg", v, true);
                ev.click("cmdOK") || ev.closed || ctx.input(|i| i.key_pressed(egui::Key::Enter))
            }
            Dialog::Delete(name) => {
                let mut v = View::default();
                v.caption = Some(d.t(0x0c8d_4e89).into());
                let q = format!("{}{name}?", d.t(0x2e25_d27e));
                desk.fit_msg(&q);
                v.text("lblText", q).text("cmdOK", "&Yes").show("cmdNo", true);
                let ev = desk.window(ctx, "dlgMsg", v, true);
                if ev.click("cmdOK") {
                    let _ = fs::remove(saves().join(format!("{name}.json")));
                    self.sync.changed();
                    self.loaded = None;
                    self.boxes.push((d.t(0x7c11_a13d).into(), d.t(0xedbd_5b0e).into()));
                }
                ev.click("cmdOK") || ev.click("cmdNo") || ev.closed
            }
            Dialog::Confirm(q, deed) => {
                let mut v = View::default();
                desk.fit_msg(&q);
                v.text("lblText", q).text("cmdOK", "&Yes").show("cmdNo", true);
                // The original's titles: JOIN GUILD, RAISE CHARACTER, Start Search?, Bind Companion.
                let title = match deed {
                    Deed::Join(..) => Some(0x705d_1626),
                    Deed::Raise(_) => Some(0x80cc_be6c),
                    Deed::Rescue(..) => Some(0x76da_125a),
                    Deed::Bind(_) => Some(0x3e5c_ab8c),
                    Deed::Drop(..) => Some(0x7a33_21db),
                    Deed::Points => Some(0x7450_fa46),
                    Deed::CleanLog => Some(0x8cf3_cd66),
                    Deed::CleanLogText(_) => Some(0xfb87_937f),
                    Deed::Forfeit(_) => Some(0x6a45_dbf2),
                    #[cfg(any(windows, target_os = "android"))]
                    Deed::Update => None,
                };
                v.caption = title.map(|t| d.t(t).into());
                let ev = desk.window(ctx, "dlgMsg", v, true);
                if ev.click("cmdOK") && deed == Deed::Points {
                    self.dialog = Some(Dialog::Pass(Pass::New(self.draft.name.clone(), None)));
                }
                if ev.click("cmdOK")
                    && let Some(g) = self.game.as_mut()
                {
                    match deed {
                        Deed::Points => {}
                        Deed::CleanLog => self.dialog = Some(Dialog::Confirm(format!("{}GUILDLOG.TXT?", d.t(0xbbd2_48ce)), Deed::CleanLogText(true))),
                        Deed::CleanLogText(_) => clean_log(g, d, true),
                        Deed::Raise(i) => g.raise(d, i),
                        Deed::Rescue(i, at) => g.rescue(d, i, at),
                        Deed::Drop(i, slot) => {
                            g.party[i].items.remove(slot);
                        }
                        Deed::Forfeit(i) => {
                            let before = g.party[i].level();
                            match g.party[i].forfeit(d, t) {
                                Err(e) => self.boxes.push((d.t(0x6a45_dbf2).into(), e)),
                                Ok(()) => {
                                    let c = &g.party[i];
                                    let line = format!("\"{}\"{}{}{}", c.name, d.t(0x5ddb_ceff), before.saturating_sub(c.level()), d.t(if c.female { 0xc909_194b } else { 0x425a_93de }));
                                    let guild = c.guild;
                                    g.hall.note(guild, line);
                                }
                            }
                        }
                        Deed::Bind(k) => {
                            if let Err(e) = g.rebind(d, k) {
                                self.boxes.push((d.t(0x3e5c_ab8c).into(), e));
                            }
                        }
                        Deed::Join(i, guild) => match g.party[i].join(d, guild) {
                            Ok(s) => {
                                let line = format!("\"{}\"{}", g.party[i].name, d.t(0x88f9_b2cb));
                                g.hall.note(guild, line);
                                if !s.is_empty() {
                                    g.log.push(s);
                                }
                            }
                            Err(e) => self.boxes.push((d.t(0x8b3c_5ec6).into(), e)),
                        },
                        #[cfg(any(windows, target_os = "android"))]
                        Deed::Update => {}
                    }
                    self.persist();
                }
                #[cfg(any(windows, target_os = "android"))]
                if ev.click("cmdOK") && deed == Deed::Update {
                    self.updater.accept(ctx);
                    self.say(if cfg!(windows) { "Downloading the update. The game will close while it installs, then start again." } else { "Downloading the update. Android will ask to install it." });
                }
                // "No" to the text file still cleans the log.
                if ev.click("cmdNo")
                    && matches!(deed, Deed::CleanLogText(_))
                    && let Some(g) = self.game.as_mut()
                {
                    clean_log(g, d, false);
                }
                ev.click("cmdOK") || ev.click("cmdNo") || ev.closed
            }
            Dialog::About => {
                let ev = desk.window(ctx, "frmAbout", View::default(), true);
                ev.click("cmdOK") || ev.closed
            }
            Dialog::Hall => hall(desk, ctx, d),
            Dialog::Options => self.options(desk, ctx),
            Dialog::Panel => self.panel(desk, ctx, d),
            Dialog::Give(to, gold) => self.give(desk, ctx, d, to, gold),
            Dialog::Pass(p) => self.password(desk, ctx, d, p),
            Dialog::ExchangeWho => self.exchange_who(desk, ctx, d),
            Dialog::GuildLog(guild) => self.guild_log(desk, ctx, d, guild),
            Dialog::Library(msg) => self.library(desk, ctx, d, msg),
            Dialog::Exchange => self.exchange(desk, ctx, d),
            Dialog::ExchangeGold(side) => self.exchange_gold(desk, ctx, d, side),
            Dialog::Load => self.load_dialog(desk, ctx, d),
            Dialog::Import => self.import(desk, ctx),
            Dialog::Recruit => self.recruit(desk, ctx, d),
            Dialog::Seer(b) => self.seer(desk, ctx, d, b),
            Dialog::MorgueWho(b) => self.morgue_who(desk, ctx, d, b),
            Dialog::MorgueWhere(i) => self.morgue_where(desk, ctx, d, i),
            Dialog::Macros(i) => self.macros(desk, ctx, d, i),
            Dialog::Teleport => self.teleport(desk, ctx, d, t),
            Dialog::Sync => self.sync_dialog(desk, ctx),
            Dialog::Clash => self.clash(desk, ctx, d),
        };
        if close && self.dialog.as_ref() == Some(&dlg) {
            self.dialog = None;
        }
    }

    /// The Options window: music and sound. The remake adds importing a copy of the game
    /// where the original kept its (hidden) screen saver button, and cloud saves in place
    /// of its file backups.
    fn options(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        const F: &str = "frmOptions";
        // Opening, the boxes and sliders show the settings; OK keeps them, Cancel not.
        if !desk.state(F, "open").on {
            desk.state(F, "open").on = true;
            for k in 0..12 {
                desk.state(F, &format!("chkCheck({k})")).on = *self.opts.check(k);
            }
            desk.state(F, "hsbVolume(0)").top = self.opts.music_volume;
            desk.state(F, "hsbVolume(1)").top = self.opts.fx_volume;
        }
        let mut v = View::default();
        v.show("cmdCommand(3)", true).text("cmdCommand(3)", "&Import Game...");
        // With a character loaded the original greys out No 3-D and the font buttons
        // (Form_Load); the remake's fonts are fixed, so those and Default stay grey.
        v.enable("chkCheck(0)", self.game.is_none());
        for k in ["cmdCommand(4)", "cmdCommand(5)", "cmdCommand(6)"] {
            v.enable(k, false);
        }
        v.enable("hsbVolume(0)", !desk.state(F, "chkCheck(1)").on).enable("hsbVolume(1)", !desk.state(F, "chkCheck(2)").on);
        let s = &self.sync;
        let status = match () {
            _ if !s.on() => "Cloud saves are off.".into(),
            _ if s.busy() => "Syncing...".into(),
            _ if s.failed => "The last sync failed.".into(),
            _ if s.cfg.last.is_empty() => "Not synced yet.".into(),
            _ => format!("Last synced on {}", s.cfg.last),
        };
        v.text("lblLabel(9)", "Cloud Saves").text("cmdCommand(1)", "Cloud &Saves...").text("cmdCommand(2)", "S&ync Now").enable("cmdCommand(2)", s.on() && !s.busy());
        v.text("lblBackup", status).text("lblLabel(2)", "Courier New").text("lblLabel(3)", "MS Sans Serif");
        let ev = desk.window(ctx, F, v, true);
        if ev.click("cmdCommand(0)") {
            for k in 0..12 {
                *self.opts.check(k) = desk.state(F, &format!("chkCheck({k})")).on;
            }
            self.opts.music_volume = desk.state(F, "hsbVolume(0)").top.min(opts::LOUD);
            self.opts.fx_volume = desk.state(F, "hsbVolume(1)").top.min(opts::LOUD);
            self.opts.save();
            self.apply_audio();
            win::FLAT.store(self.opts.no_3d, std::sync::atomic::Ordering::Relaxed);
        }
        if ev.click("cmdCommand(3)") {
            self.dialog = Some(Dialog::Import);
        }
        if ev.click("cmdCommand(1)") {
            desk.state("dlgSync", "txtServer").text = self.sync.cfg.server.clone();
            desk.state("dlgSync", "txtCode").text = self.sync.cfg.code.clone();
            self.dialog = Some(Dialog::Sync);
        }
        if ev.click("cmdCommand(2)") {
            self.sync.now(ctx, self.game.is_none());
        }
        // The answer comes once this window is closed.
        #[cfg(any(windows, target_os = "android"))]
        if ev.click("cmdUpdate") {
            self.updater.check_now(ctx);
            return true;
        }
        let done = ev.click("cmdCommand(0)") || ev.click("cmdCommand(7)") || ev.closed;
        if done {
            desk.state(F, "open").on = false;
        }
        done
    }

    /// The Control Panel (frmControlPanel): the controlled character's options (OK keeps
    /// them), the windows' places, and the Options window.
    fn panel(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        const F: &str = "frmControlPanel";
        let Some(g) = self.game.as_mut() else { return true };
        let sel = self.sel.min(g.party.len() - 1);
        if !desk.state(F, "open").on {
            desk.state(F, "open").on = true;
            let p = g.party[sel].panel;
            for (k, on) in [(3, p.auto_level), (0, p.hide), (1, p.preload), (2, p.no_perception)] {
                desk.state(F, &format!("chkCheck({k})")).on = on;
            }
        }
        // Window Control: every window, or <ALL>.
        let wins: Vec<(&str, String)> = std::iter::once(("", d.t(0x7efa_ad20).to_string()))
            .chain(["frmCharStat1", if self.screen == Screen::Dungeon { "frmDungeonMain" } else { "frmCityMain" }, "frmCharStat3", "frmParty", "frmAutoMap", "frmCharStat4"].into_iter().map(|f| (f, if f == "frmCharStat1" { format!("\"{}\"", g.party[sel].name) } else { desk.forms.get(f).map_or(String::new(), |x| win::text(&x.props, "Caption")) })))
            .collect();
        let mut v = View::default();
        v.items("lstWindows", wins.iter().map(|w| w.1.clone()).collect());
        let ev = desk.window(ctx, F, v, true);
        let picked = desk.state(F, "lstWindows").sel.and_then(|i| wins.get(i)).map(|w| w.0);
        if ev.click("cmdCommand(2)") || ev.click("cmdCommand(4)") {
            // Reset (and Adjust to Display: the remake always fits the desktop to the
            // screen, so there is no font size to ask about) puts the window, or all of
            // them, back where they start.
            let keep = desk.pos.clone();
            desk.arranged = None;
            stats::arrange(desk);
            if let Some(f) = picked.filter(|f| !f.is_empty()) {
                let at = desk.pos.get(f).copied();
                desk.pos = keep;
                if let Some(at) = at {
                    desk.pos.insert(f.into(), at);
                }
            }
        }
        if ev.click("cmdCommand(6)")
            && let Some(f) = picked.filter(|f| !f.is_empty())
        {
            self.automap_open |= f == "frmAutoMap";
            desk.active = Some(f.into());
        }
        if ev.click("cmdCommand(5)") {
            self.dialog = Some(Dialog::Options);
        }
        if ev.click("cmdCommand(0)") {
            let on = |desk: &mut Desk, k: usize| desk.state(F, &format!("chkCheck({k})")).on;
            let p = ddj_rules::game::Panel { auto_level: on(desk, 3), hide: on(desk, 0), preload: on(desk, 1), no_perception: on(desk, 2) };
            let name = g.party[sel].name.clone();
            g.party[sel].panel = p;
            // Preload Char. names this character in the settings; off, it clears them if
            // they name it.
            if p.preload {
                self.opts.preload = Some(name);
            } else if self.opts.preload.as_deref() == Some(name.as_str()) {
                self.opts.preload = None;
            }
            self.opts.save();
            self.persist();
        }
        let done = ev.click("cmdCommand(0)") || ev.click("cmdCommand(1)") || ev.closed || self.dialog.as_ref().is_some_and(|x| matches!(x, Dialog::Options));
        if done {
            desk.state(F, "open").on = false;
        }
        done
    }

    /// Give (sub145F through fn16A0): "Give to Character", then `Give to "X"`'s "How much
    /// Gold?" and "Which Item #?" (the pack number; #1, the hands, gives none).
    fn give(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, to: Option<usize>, gold: Option<u64>) -> bool {
        const F: &str = "frmInteract";
        let Some(g) = self.game.as_mut() else { return true };
        let from = self.sel.min(g.party.len() - 1);
        let others: Vec<usize> = (0..g.party.len()).filter(|&i| i != from).collect();
        let mut v = View::default();
        match to {
            None => {
                v.caption = Some(d.t(0xcf91_87ae).into());
                v.text("lblQuestion", d.t(0x2113_adde)).show("txtQuestion", false).show("cmbCharacter", true).items("cmbCharacter", others.iter().map(|&i| g.party[i].name.clone()).collect());
            }
            Some(t) => {
                v.caption = Some(format!("{}\"{}\"", d.t(0xae8f_d4c7), g.party[t].name));
                v.text("lblQuestion", d.t(if gold.is_none() { 0xf1e3_bb79 } else { 0x179f_6071 }));
            }
        }
        let ev = desk.window(ctx, F, v, true);
        if !(ev.click("cmdOK") || ctx.input(|i| i.key_pressed(egui::Key::Enter))) {
            return ev.click("cmdCancel") || ev.closed;
        }
        let typed = std::mem::take(&mut desk.state(F, "txtQuestion").text);
        let n: u64 = typed.trim().parse().unwrap_or(0);
        let sorry = |s: &mut Self, text: u32| s.boxes.push((d.t(0x9fa0_af51).into(), d.t(text).into()));
        match (to, gold) {
            (None, _) => {
                if let Some(t) = desk.state(F, "cmbCharacter").sel.and_then(|k| others.get(k)).copied() {
                    desk.st.retain(|k, _| !k.starts_with("frmInteract."));
                    self.dialog = Some(Dialog::Give(Some(t), None));
                }
                false
            }
            (Some(_), None) if n > g.party[from].gold => {
                sorry(self, 0x22bd_be43);
                false
            }
            (Some(t), None) => {
                self.dialog = Some(Dialog::Give(Some(t), Some(n)));
                false
            }
            (Some(t), Some(gold)) => {
                let slot = (n > 0).then(|| n as usize - 1);
                match slot.map(|s| g.party[from].items.get(s)) {
                    Some(None) => {
                        sorry(self, 0x2cc8_b782);
                        false
                    }
                    Some(Some(h)) if h.equipped => {
                        sorry(self, 0x5c4e_caa2);
                        false
                    }
                    _ => {
                        g.give_to(d, from, t, gold, slot);
                        self.persist();
                        true
                    }
                }
            }
        }
    }

    /// Info (sub150A): the item on the character window's Misc tab, brought forward.
    fn show_info(info: &mut Option<(usize, u8, u8)>, desk: &mut Desk, item: usize, align: u8, id: u8) {
        *info = Some((item, align, id));
        desk.state("frmCharStat1", "tabStatTab").tab = 6;
        desk.active = Some("frmCharStat1".into());
    }

    /// The Options window's sound settings, on the player.
    fn apply_audio(&mut self) {
        let o = &self.opts;
        if let Some(a) = self.audio.as_mut() {
            a.apply(!o.no_music, !o.no_fx, o.sound_canvas, o.music_volume as f32 / opts::LOUD as f32, o.fx_volume as f32 / opts::LOUD as f32);
        }
    }

    /// The remake's import window: the player's own copy of the game, from a ZIP, its
    /// MORDOR.WIP or the install folder's files.
    fn import(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        #[allow(unused_mut)]
        let mut v = View::default();
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        v.show("txtPath", false).text("cmdImport", "Choose &Files...").show("cmdFolder", cfg!(target_arch = "wasm32"));
        #[cfg(target_os = "android")]
        v.text("lblText", "The full game's 15 dungeon levels come from your own copy of Mordor. Choose its ZIP, MORDOR.WIP or install folder's files:");
        let ev = desk.window(ctx, "dlgImport", v, true);
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        let (files, path) = (Vec::new(), dropped.first().map(|f| f.path().to_path_buf()).or_else(|| ev.click("cmdImport").then(|| PathBuf::from(desk.state("dlgImport", "txtPath").text.trim()))));
        #[cfg(target_arch = "wasm32")]
        let (files, path) = {
            if !dropped.is_empty() {
                web::read_dropped(dropped, ctx.clone());
            }
            if ev.click("cmdImport") || ev.click("cmdFolder") {
                web::choose(ev.click("cmdFolder"), ctx.clone());
            }
            (web::picked().unwrap_or_default(), None::<PathBuf>)
        };
        // Android's document picker copies the chosen files into the app's storage.
        #[cfg(target_os = "android")]
        let (files, path) = {
            drop(dropped);
            if ev.click("cmdImport") {
                android::choose();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
            (android::picked().unwrap_or_default(), None::<PathBuf>)
        };
        let dir = ddj_extract::data_dir();
        let done = match (path, files.is_empty()) {
            (Some(p), _) => ddj_extract::import(&p, &dir),
            (None, false) => ddj_extract::import_files(files, &dir),
            (None, true) => return ev.click("cmdCancel") || ev.closed,
        };
        let msg = match done.and_then(|_| GameData::load(&dir)) {
            Ok(d) => {
                let n = d.floors.len();
                self.data = Some(d);
                self.art = None;
                if n < 15 { format!("Imported {n} dungeon levels: that's the shareware release.") } else { "Imported the full game.".into() }
            }
            Err(e) => format!("Import failed: {e}"),
        };
        self.dialog = Some(Dialog::Msg("Import Game".into(), msg));
        false
    }

    /// The cloud saves window: the sync server, and the code shared by the player's devices.
    fn sync_dialog(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let mut v = View::default();
        v.enable("cmdNew", !self.sync.busy()).enable("cmdOK", !self.sync.busy()).enable("cmdOff", self.sync.on());
        let ev = desk.window(ctx, "dlgSync", v, true);
        let server = desk.state("dlgSync", "txtServer").text.trim().to_string();
        let code = desk.state("dlgSync", "txtCode").text.trim().to_string();
        if (ev.click("cmdNew") || ev.click("cmdOK")) && server.is_empty() {
            self.dialog = Some(Dialog::Msg("Cloud Saves".into(), "Enter the sync server's address first.".into()));
        } else if ev.click("cmdNew") {
            self.sync.new_code(ctx, &server);
        } else if ev.click("cmdOK") && code.is_empty() {
            self.dialog = Some(Dialog::Msg("Cloud Saves".into(), "Make a new code, or type the one from your other device.".into()));
        } else if ev.click("cmdOK") {
            self.sync.link(ctx, &server, &code, self.game.is_none(), true);
            return true;
        }
        if ev.click("cmdOff") {
            self.sync.off();
            desk.state("dlgSync", "txtCode").text.clear();
        }
        ev.click("cmdCancel") || ev.closed
    }

    /// Each frame: finished syncs, a new code, and the files both sides changed. The
    /// store keeps its newer copy and the Hall of Records the best of each record; a
    /// character waits for the player to choose (once not playing).
    fn sync_frame(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        match self.sync.poll(ctx, self.game.is_some()) {
            Some(Ok(code)) => {
                desk.state("dlgSync", "txtCode").text = code.clone();
                let server = self.sync.cfg.server.clone();
                self.sync.link(ctx, &server, &code, self.game.is_none(), false);
                self.dialog = Some(Dialog::Msg("Cloud Saves".into(), format!("Your sync code is {code}. Type it on your other devices to play these saves there.")));
            }
            Some(Err(m)) => self.say(m),
            None => {}
        }
        let (shared, chars): (Vec<_>, Vec<_>) = std::mem::take(&mut self.sync.clashes).into_iter().partition(|c| ["store.json", "hall.json", "library.json"].contains(&c.name.as_str()));
        self.sync.clashes = chars;
        for c in shared {
            let theirs = if c.name == "hall.json" {
                let mut h = Hall::load(&saves(), today());
                if let Some(o) = c.theirs.as_deref().and_then(|b| serde_json::from_slice::<Hall>(b).ok()) {
                    h.merge(d, o);
                }
                let _ = h.save(&saves());
                false
            } else if c.name == "library.json" {
                let mut l = fs::read(saves().join(&c.name)).ok().and_then(|b| serde_json::from_slice::<Library>(&b).ok()).unwrap_or_default();
                if let Some(o) = c.theirs.as_deref().and_then(|b| serde_json::from_slice::<Library>(b).ok()) {
                    l.merge(o);
                }
                let _ = l.save(&saves());
                false
            } else {
                let saved = |b: &[u8]| serde_json::from_slice::<serde_json::Value>(b).ok().and_then(|v| v["saved"].as_u64()).unwrap_or(0);
                let mine = fs::read(saves().join(&c.name)).map_or(0, |b| saved(&b));
                c.theirs.as_deref().is_some_and(|b| saved(b) > mine)
            };
            self.sync.settle(ctx, c, theirs);
        }
        if self.dialog.is_none() && self.game.is_none() && !self.sync.clashes.is_empty() {
            self.dialog = Some(Dialog::Clash);
        }
    }

    /// "Kira was played here and on another device": which copy stays.
    fn clash(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
        let Some(c) = self.sync.clashes.first() else { return true };
        let name = c.name.trim_end_matches(".json").to_string();
        let about = |b: Option<Vec<u8>>| match b.and_then(|b| serde_json::from_slice::<Character>(&b).ok()) {
            Some(ch) => format!("a level {} {} with {} gold", ch.level(), d.guilds.get(ch.guild).map_or("", |g| g.name.as_str()), stats::commas(ch.gold)),
            None => "deleted".into(),
        };
        let (mine, theirs) = (about(fs::read(saves().join(&c.name)).ok()), about(c.theirs.clone()));
        let mut v = View::default();
        v.text("lblText", format!("\"{name}\" was played on this device and on another since they last synced. Here it is {mine}; on the other device, {theirs}. Which do you keep?"));
        let ev = desk.window(ctx, "dlgClash", v, true);
        if ev.click("cmdOK") || ev.click("cmdNo") {
            let c = self.sync.clashes.remove(0);
            self.sync.settle(ctx, c, ev.click("cmdNo"));
        }
        // Closing the window leaves both copies as they are; the next sync asks again.
        if ev.closed {
            self.sync.clashes.clear();
        }
        self.sync.clashes.is_empty() || ev.closed
    }

    /// The playing screens: the character, objects, messages and party windows, with
    /// the city (and an open building) or the dungeon.
    fn play(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) {
        // The phone shell lays out again once the city has opened a building.
        let shell = |app: &Self, desk: &mut Desk| stats::shell(desk, app.building.unwrap_or(if app.screen == Screen::Town { "frmCityMain" } else { "frmDungeonMain" }), app.screen == Screen::Dungeon);
        if desk.shell {
            shell(self, desk);
        } else {
            stats::arrange(desk);
        }
        if self.screen == Screen::Town {
            self.town(desk, ctx, d, t);
        } else {
            self.dungeon(desk, ctx, d, t);
        }
        if desk.shell {
            shell(self, desk);
        }
        // The city is where the original checks most records (experience, levels, bank).
        if self.screen == Screen::Town && self.game.as_mut().is_some_and(|g| g.records(d)) {
            self.persist();
        }
        if self.game.is_none() {
            return;
        }
        self.party_window(desk, ctx, d, t);
        self.stats_window(desk, ctx, d, t);
        self.objects_window(desk, ctx, d, t);
        self.messages_window(desk, ctx);
        if self.screen == Screen::Dungeon && (self.automap_open || desk.shell) {
            self.automap_window(desk, ctx, d);
        }
        if let Some(b) = self.building {
            self.building(desk, ctx, d, t, b);
        }
        if desk.shell {
            let mut v = View::default();
            v.tab_height("tabShell", 26.0);
            if desk.window(ctx, stats::STRIP, v, false).changed.is_some() {
                desk.active = Some(stats::TABS[desk.state(stats::STRIP, "tabShell").tab][0].into());
                ctx.request_repaint();
            }
        }
    }
}

/// The remake's own windows, in the original's style.
fn add_windows(desk: &mut Desk) {
    // Check for Updates beside Import Game in Options (Windows runs the new Setup; Android
    // hands the new APK to its installer).
    #[cfg(any(windows, target_os = "android"))]
    if let Some(f) = desk.forms.get_mut("frmOptions") {
        let import = f.controls.iter_mut().find(|c| c.name == "cmdCommand" && c.index == Some(3));
        let parent = import.map(|c| {
            c.rect[2] = 96;
            c.parent
        });
        let mut c = ddj_extract::forms::Control { name: "cmdUpdate".into(), kind: "CommandButton".into(), parent: parent.flatten(), rect: [108, 251, 97, 22], ..Default::default() };
        c.props.insert("Caption".into(), ddj_extract::forms::Val::S("Check &Updates".into()));
        f.controls.push(c);
    }
    desk.synth("dlgMsg", "Mordor", 300, 96, &[("Label", "lblText", None, [12, 10, 276, 48], ""), ("CommandButton", "cmdOK", None, [108, 66, 84, 22], "OK"), ("CommandButton", "cmdNo", None, [198, 66, 84, 22], "&No")]);
    desk.hide_by_default("dlgMsg", "cmdNo");
    desk.synth(
        "dlgImport",
        "Import Game",
        360,
        124,
        &[
            ("Label", "lblText", None, [8, 6, 344, 44], "The full game's 15 dungeon levels come from your own copy of Mordor. Drop its ZIP, MORDOR.WIP or install folder's files on this window, or:"),
            ("TextBox", "txtPath", None, [8, 56, 344, 20], ""),
            ("CommandButton", "cmdImport", None, [8, 92, 110, 24], "&Import"),
            ("CommandButton", "cmdFolder", None, [124, 92, 110, 24], "Choose F&older..."),
            ("CommandButton", "cmdCancel", None, [268, 92, 84, 24], "&Cancel"),
        ],
    );
    desk.hide_by_default("dlgImport", "cmdFolder");
    desk.synth(
        "dlgSync",
        "Cloud Saves",
        360,
        150,
        &[
            ("Label", "lblText", None, [8, 6, 344, 40], "Cloud saves keep your characters, the store and the Hall of Records the same on each computer, phone and browser you play on."),
            ("Label", "lblServer", None, [8, 54, 70, 16], "Server:"),
            ("TextBox", "txtServer", None, [80, 52, 272, 20], ""),
            ("Label", "lblCode", None, [8, 82, 70, 16], "Sync code:"),
            ("TextBox", "txtCode", None, [80, 80, 272, 20], ""),
            ("CommandButton", "cmdNew", None, [8, 116, 84, 24], "&New Code"),
            ("CommandButton", "cmdOK", None, [98, 116, 84, 24], "&Sync"),
            ("CommandButton", "cmdOff", None, [188, 116, 84, 24], "Turn &Off"),
            ("CommandButton", "cmdCancel", None, [278, 116, 74, 24], "&Cancel"),
        ],
    );
    desk.synth(
        "dlgClash",
        "Cloud Saves",
        360,
        112,
        &[
            ("Label", "lblText", None, [8, 6, 344, 64], ""),
            ("CommandButton", "cmdOK", None, [140, 80, 104, 24], "&This Device"),
            ("CommandButton", "cmdNo", None, [250, 80, 104, 24], "&Other Device"),
        ],
    );
    party::add_windows(desk);
    stats::add_shell(desk);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Debug web builds draw continuously, so test captures are never stale.
        #[cfg(all(debug_assertions, target_arch = "wasm32"))]
        ctx.request_repaint();
        let tuning = match std::mem::replace(&mut self.tuning, Err(String::new())) {
            Ok(t) => t,
            Err(e) => {
                egui::CentralPanel::default().show(ui, |ui| ui.colored_label(Color32::RED, format!("Bad patch file: {e}")));
                self.tuning = Err(e);
                return;
            }
        };
        if !self.styled {
            art::style(&ctx);
            win::register_fonts(&ctx);
            self.styled = true;
        }
        if !self.audio_tried && (cfg!(not(target_arch = "wasm32")) || ui.input(|i| i.pointer.any_click() || !i.keys_down.is_empty())) {
            self.audio_tried = true;
            self.audio = audio::Audio::new();
            self.apply_audio();
        }
        let Some(data) = self.data.take() else {
            egui::CentralPanel::default().show(ui, |ui| ui.label("The game data could not be read."));
            self.tuning = Ok(tuning);
            return;
        };
        let dir = ddj_extract::data_dir();
        if self.art.is_none() {
            self.art = Art::load(&ctx, &dir);
        }
        if self.desk.is_none() {
            // Data imported before the remake read the original's windows: take them
            // from the bundled shareware's MORDOR.EXE.
            if ddj_extract::forms::load(&dir).is_none() {
                let _ = ddj_extract::import_only(vec![("MORDOR11.ZIP".into(), SHAREWARE.to_vec())], &dir, "MORDOR.EXE");
            }
            self.desk = Desk::new(&ctx, &dir);
            if let Some(desk) = self.desk.as_mut() {
                add_windows(desk);
            }
        }
        let Some(mut desk) = self.desk.take() else {
            egui::CentralPanel::default().show(ui, |ui| ui.label("The original's windows could not be read from MORDOR.EXE."));
            self.data = Some(data);
            self.tuning = Ok(tuning);
            return;
        };
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let mobile = cfg!(any(target_arch = "wasm32", target_os = "android")) || std::env::var_os("DDJ_MOBILE").is_some();
            let layouts = if mobile { stats::layout_sizes(&desk) } else { Vec::new() };
            #[cfg(target_os = "android")]
            {
                desk.keyboard = android::keyboard() / ctx.pixels_per_point();
                desk.insets = android::insets().map(|p| p / ctx.pixels_per_point());
            }
            desk.begin(ui.max_rect(), &layouts);
            self.main_window(&mut desk, ui, &data, &tuning);
            match self.screen {
                Screen::Menu => self.menu(&mut desk, &ctx, &data),
                Screen::Create => {
                    self.menu(&mut desk, &ctx, &data);
                    self.create(&mut desk, &ctx, &data);
                }
                Screen::List => {
                    self.menu(&mut desk, &ctx, &data);
                    self.list(&mut desk, &ctx);
                }
                Screen::Town | Screen::Dungeon => self.play(&mut desk, &ctx, &data, &tuning),
            }
            self.sync_frame(&mut desk, &ctx, &data);
            self.dialog(&mut desk, &ctx, &data, &tuning);
            // F1: the help for the window in front (its HelpContextID), as the original's.
            if self.help.is_none() && ctx.input(|i| i.key_pressed(egui::Key::F1)) {
                let main = match self.screen {
                    Screen::Town => "frmCityMain",
                    Screen::Dungeon => "frmDungeonMain",
                    Screen::Create => "frmCreateChar",
                    _ => "frmMenu",
                };
                let f = desk.active.clone().filter(|f| desk.forms.contains_key(f) && !f.starts_with("frmHelp")).unwrap_or(main.into());
                let id = desk.forms.get(&f).and_then(|f| win::num(&f.props, "HelpContextID")).unwrap_or(0);
                self.help(&mut desk, "main", Some(id));
            }
            if let Some(h) = self.help.as_mut()
                && !h.show(&mut desk, &ctx)
            {
                self.help = None;
            }
            // A newer release, offered once nothing else is asked; a failed download said.
            #[cfg(any(windows, target_os = "android"))]
            {
                if self.dialog.is_none() {
                    match self.updater.offer() {
                        Some(Ok(q)) => self.dialog = Some(Dialog::Confirm(q, Deed::Update)),
                        Some(Err(m)) => self.say(m),
                        None => {}
                    }
                }
                if let Some(e) = self.updater.error() {
                    self.say(e);
                }
            }
        });
        // An import replaces the data during the frame: the windows are read again.
        if self.data.is_none() {
            self.desk = Some(desk);
        }
        // The original's Sound Canvas music for the menu, the city and the dungeon, and
        // its effects for what just happened (at most a few a frame).
        if let Some(a) = self.audio.as_mut() {
            a.music(&dir, match self.screen {
                Screen::Town => audio::CITY,
                Screen::Dungeon => audio::DUNGEON,
                _ => audio::THEME,
            });
            if let Some(g) = &self.game {
                self.heard = self.heard.min(g.log.len());
                let new = &g.log[self.heard..];
                // Each Messages line dings (sub085B) unless "No Msg. Ding"; "No Party
                // Sound" leaves only the controlled character's actions heard.
                if !new.is_empty() && !self.opts.no_ding {
                    a.effect(&dir, "MESSAGE.WAV");
                }
                let others: Vec<&str> = g.party[1..].iter().map(|m| m.name.as_str()).collect();
                let heard = |l: &&String| !self.opts.no_party_fx || !others.iter().any(|n| l.starts_with(n) || l.starts_with(&format!("\"{n}\"")));
                for w in new.iter().filter(heard).filter_map(|l| audio::effect_for(l)).filter(|&w| w != "HELP.WAV" || !self.opts.no_help_sound).take(3) {
                    a.effect(&dir, w);
                }
                self.heard = g.log.len();
            }
        }
        self.data = self.data.take().or(Some(data));
        self.tuning = Ok(tuning);
    }

    /// What changed since the last sync goes up before the game closes.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn on_exit(&mut self) {
        self.sync.flush();
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android::App) {
    android::start(&app);
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    let _ = eframe::run_native("Dejenol Restoration Project", options, Box::new(|_| Ok(Box::new(App::new()))));
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn main() -> eframe::Result {
    eframe::run_native(
        "Dejenol Restoration Project",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 900.0]).with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png")).unwrap_or_default()),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(App::new()))),
    )
}
