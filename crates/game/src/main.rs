#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use ddj_extract::vfs as fs;
use web_time::Instant;

use ddj_rules::{
    Tuning,
    game::{self, Character, Game},
    hall::Hall,
    mdr::GameData,
};
use eframe::egui::{self, Color32};

mod art;
mod audio;
mod dungeon;
mod help;
mod party;
mod stats;
mod town;
#[cfg(not(target_arch = "wasm32"))]
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
    /// A question for the Seer (character or monster name).
    Seer,
    Macros(usize),
}

/// A service done once the player confirms it.
#[derive(Clone, Copy, PartialEq)]
enum Deed {
    Raise(usize),
    Rescue(usize),
    /// Download and install a newer release.
    #[cfg(not(target_arch = "wasm32"))]
    Update,
}

struct Draft {
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
    desk: Option<Desk>,
    styled: bool,
    screen: Screen,
    dialog: Option<Dialog>,
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
    #[cfg(not(target_arch = "wasm32"))]
    updater: update::Updater,
}

/// Today as the original's short date (m/d/yyyy, UTC): Hinnant's days-to-civil.
fn today() -> String {
    let z = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64 + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{m}/{}/{}", doy - (153 * mp + 2) / 5 + 1, yoe + z.div_euclid(146_097) * 400 + i64::from(m <= 2))
}

/// The Hall of Records (frmHall, sub32F6): the six stat records down the left, the six
/// feats down the right, each a title over who set it and with what.
fn hall(desk: &mut Desk, ctx: &egui::Context, d: &GameData) -> bool {
    const F: &str = "frmHall";
    const STATS: [&str; 6] = ["Strength", "Intelligence", "Wisdom", "Constitution", "Charisma", "Dexterity"];
    const TITLES: [&str; 12] = [
        "Strongest", "Smartest", "Wisest", "Healthiest", "Most Attractive", "Quickest",
        "Deadliest Creature Defeated", "Most Experienced Explorer", "Wealthiest Explorer", "Master of Fighting", "Master of Magic", "Master of Thieving",
    ];
    let h = Hall::load(&saves(), String::new());
    let mut v = View::default();
    for (j, r) in h.records.iter().enumerate() {
        let (side, i) = (if j < 6 { "Record" } else { "History" }, (j % 6) as u16);
        let (s0, c0) = (desk.rect(F, &format!("lbl{side}Stat(0)")), desk.rect(F, &format!("lbl{side}Char(0)")));
        let top = (s0[3] + c0[3]) * i32::from(i);
        v.clone_rect(&format!("lbl{side}Stat(0)"), i, [s0[0], s0[1] + top, s0[2], s0[3]]);
        v.clone_rect(&format!("lbl{side}Char(0)"), i, [c0[0], c0[1] + top, c0[2], c0[3]]);
        let set = r.date.as_ref().map_or("Set by Original RecordHolder ".into(), |t| format!("Set on {t} by "));
        let what = match j {
            0..6 => format!("With a {} {}", r.value, STATS[j]),
            6 => format!("Defeated a \"{}\"", d.monsters.iter().find(|m| i64::from(m.id) == r.value).map_or("(Unknown)", |m| &m.name)),
            7 => format!("Having {} total experience", stats::commas(r.value)),
            8 => format!("Having a total of {} gold in the Bank", stats::commas(r.value)),
            9 => format!("Having a combined fighting skill of {}", r.value),
            10 => format!("Having a spell knowledge && power of {}", r.value),
            _ => format!("Having a total thieving skill of {}", r.value),
        };
        v.text(&format!("lbl{side}Stat({i})"), TITLES[j]).text(&format!("lbl{side}Char({i})"), format!("{set}\"{}\"\n{what}", r.name));
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
        Self {
            tuning: load_tuning(),
            data,
            art: None,
            desk: None,
            styled: false,
            screen: Screen::Menu,
            dialog: None,
            loaded: None,
            building: None,
            draft: Draft { race: 0, female: false, alignment: 1, stats: [0; 6] },
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
            #[cfg(not(target_arch = "wasm32"))]
            updater: update::Updater::start(&saves(), &today()),
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
        let Some(g) = &self.game else { return };
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
        if let Some(e) = err {
            self.say(e);
        }
    }

    /// Starts play with a character, bringing its allies along when they can come.
    fn start(&mut self, ch: Character, d: &GameData) {
        let allies = ch.allies.clone();
        let mut g = Game::new(ch);
        g.store = ddj_rules::store::Store::load(&saves(), d);
        g.hall = Hall::load(&saves(), today());
        for name in allies {
            if let Ok(a) = load(&name)
                && let Err(e) = g.add_ally(a)
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
        let title = format!("Mordor 1.1{}", if d.floors.len() < 15 { " (Limited Version)" } else { "" });
        let mut v = View::default();
        // The toolbar: hidden in design, shown across the top while playing, with the
        // party's gold and the experience still needed at its right.
        v.show("picToolbar", self.game.is_some()).rect("picToolbar", [0, 0, 800, 32]).show("mciMusic", false).show("picPalette", false);
        if let Some(g) = &self.game {
            let c = &g.party[self.sel.min(g.party.len() - 1)];
            let gold: u64 = g.party.iter().map(|m| m.gold).sum();
            let need = c.xp_needed(d, t).saturating_sub(c.xp[c.guild]);
            let (lg, le) = (desk.rect("frmMain", "lblGold"), desk.rect("frmMain", "lblExperience"));
            v.rect("lblGold", [796 - lg[2], lg[1], lg[2], lg[3]]).rect("lblExperience", [796 - le[2], le[1], le[2], le[3]]);
            v.text("lblGold", format!("You have a total of {} Gold.", stats::commas(gold))).text("lblExperience", format!("You need {} more experience for level {}.", stats::commas(need), c.level() + 1));
        }
        let playing = self.game.is_some();
        for i in [1, 2, 3, 5, 7] {
            v.enable(&format!("cmdTool({i})"), playing);
        }
        for i in [4, 6, 8] {
            v.enable(&format!("cmdTool({i})"), false);
        }
        let ev = desk.main(ui, &title, v);
        match ev.index("cmdTool") {
            // The HelpLesson; F5: Objects & Spells; F8: the AutoMap; 'A': add a character; F10: options.
            Some(0) => self.help(desk, "main", None),
            Some(1) => desk.active = Some("frmCharStat3".into()),
            Some(2) => desk.active = Some("frmAutoMap".into()),
            Some(3) => self.dialog = Some(Dialog::Recruit),
            Some(5) => {
                // 'P': pool the party's gold to the character shown.
                if let Some(g) = self.game.as_mut() {
                    let sel = self.sel.min(g.party.len() - 1);
                    let pooled: u64 = g.party.iter_mut().enumerate().filter(|(i, _)| *i != sel).map(|(_, m)| std::mem::take(&mut m.gold)).sum();
                    g.party[sel].gold += pooled;
                    let msg = format!("Pool of party's on-hand gold to \"{}\" completed!", g.party[sel].name);
                    g.log.push(msg);
                    self.persist();
                }
            }
            Some(7) => self.dialog = Some(Dialog::Options),
            _ => {}
        }
    }

    /// The opening menu (frmMenu); behind the character list and creation it only shows.
    /// As the original's sub2D2E: with a character loaded the first button runs it and
    /// the load button unloads; Delete, Exchange and Password show only then.
    fn menu(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        let mut v = View::default();
        match &self.loaded {
            Some(name) => v.text("cmdCommand(0)", format!("&Run \"{name}\"")).text("cmdCommand(7)", "&Unload Character"),
            None => v.text("cmdCommand(0)", "&Create a Character").text("cmdCommand(7)", "&Load Character"),
        };
        for i in [1, 4, 5] {
            v.show(&format!("cmdCommand({i})"), self.loaded.is_some());
        }
        for i in [1, 4] {
            v.enable(&format!("cmdCommand({i})"), false);
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
                    self.draft = Draft { race: 0, female: false, alignment: 1, stats };
                    desk.st.retain(|k, _| !k.starts_with("frmCreateChar."));
                    self.screen = Screen::Create;
                }
            },
            Some(7) if self.loaded.is_some() => self.loaded = None,
            Some(7) => self.dialog = Some(Dialog::Load),
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
    fn load_dialog(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let names = game::saved(&saves());
        // The first character comes up chosen, as in the original.
        let st = desk.state("frmInteract", "cmbCharacter");
        st.sel = st.sel.filter(|&i| i < names.len()).or((!names.is_empty()).then_some(0));
        let mut v = View::default();
        v.caption = Some("Load Character".into());
        v.text("lblQuestion", "Load which character?").show("txtQuestion", false).show("cmbCharacter", true).items("cmbCharacter", names.clone());
        let ev = desk.window(ctx, "frmInteract", v, true);
        if ev.click("cmdOK") {
            self.loaded = desk.state("frmInteract", "cmbCharacter").sel.and_then(|i| names.get(i)).cloned().or(self.loaded.take());
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
        let left = game::FREE_POINTS - dr.stats.iter().zip(base).map(|(s, b)| s - b).sum::<i16>();
        let mut v = View::default();
        v.items("cmbRace", d.races.iter().map(|r| r.name.clone()).collect());
        v.items("cmbSex", vec!["Male".into(), "Female".into()]);
        v.items("cmbAlignment", aligns.iter().map(|&a| ["Good", "Neutral", "Evil"][a as usize].to_string()).collect());
        v.text("lblPoints", left.to_string());
        for (i, b) in base.iter().enumerate() {
            v.text(&format!("pnlPanel({i})"), dr.stats[i].to_string()).text(&format!("lblStatInfo({i})"), format!("({}-{})", r.stat_min[i], r.stat_max[i]));
            v.enable(&format!("cmdAdd({i})"), left > 0 && dr.stats[i] < r.stat_max[i]).enable(&format!("cmdSubtract({i})"), dr.stats[i] > *b.max(&r.stat_min[i]));
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
        if ev.click("cmdSave") || (ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !name.is_empty()) {
            if game::saved(&saves()).contains(&name) {
                self.say("That name is taken.");
                return;
            }
            match Character::new(d, &name, race, female, alignment, stats) {
                Ok(ch) => {
                    self.loaded = Some(name.clone());
                    self.start(ch, d);
                    self.building = Some("frmCityStore");
                    self.say("Welcome to Mordor! The store has the standard starting kit.");
                    self.persist();
                }
                Err(e) => self.say(e),
            }
        }
    }

    /// The window over everything else, if any.
    fn dialog(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData) {
        let Some(dlg) = self.dialog.clone() else { return };
        let close = match dlg.clone() {
            Dialog::Msg(title, text) => {
                let mut v = View::default();
                v.caption = Some(title);
                v.text("lblText", text);
                let ev = desk.window(ctx, "dlgMsg", v, true);
                ev.click("cmdOK") || ev.closed || ctx.input(|i| i.key_pressed(egui::Key::Enter))
            }
            Dialog::Delete(name) => {
                let mut v = View::default();
                v.caption = Some("Delete Character".into());
                v.text("lblText", format!("Delete \"{name}\" for good?")).text("cmdOK", "&Yes").show("cmdNo", true);
                let ev = desk.window(ctx, "dlgMsg", v, true);
                if ev.click("cmdOK") {
                    let _ = fs::remove(saves().join(format!("{name}.json")));
                    self.loaded = None;
                }
                ev.click("cmdOK") || ev.click("cmdNo") || ev.closed
            }
            Dialog::Confirm(q, deed) => {
                let mut v = View::default();
                v.text("lblText", q).text("cmdOK", "&Yes").show("cmdNo", true);
                let ev = desk.window(ctx, "dlgMsg", v, true);
                if ev.click("cmdOK")
                    && let Some(g) = self.game.as_mut()
                {
                    match deed {
                        Deed::Raise(i) => g.raise(d, i),
                        Deed::Rescue(i) => g.rescue(i),
                        #[cfg(not(target_arch = "wasm32"))]
                        Deed::Update => {}
                    }
                    self.persist();
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ev.click("cmdOK") && deed == Deed::Update {
                    self.updater.accept(ctx);
                    self.say("Downloading the update. The game will close while it installs, then start again.");
                }
                ev.click("cmdOK") || ev.click("cmdNo") || ev.closed
            }
            Dialog::About => {
                let ev = desk.window(ctx, "frmAbout", View::default(), true);
                ev.click("cmdOK") || ev.closed
            }
            Dialog::Hall => hall(desk, ctx, d),
            Dialog::Options => self.options(desk, ctx),
            Dialog::Load => self.load_dialog(desk, ctx),
            Dialog::Import => self.import(desk, ctx),
            Dialog::Recruit => self.recruit(desk, ctx),
            Dialog::Seer => self.seer(desk, ctx, d),
            Dialog::Macros(i) => self.macros(desk, ctx, d, i),
        };
        if close && self.dialog.as_ref() == Some(&dlg) {
            self.dialog = None;
        }
    }

    /// The Options window: music and sound. The remake adds importing a copy of the game
    /// where the original kept its (hidden) screen saver button.
    fn options(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        const F: &str = "frmOptions";
        if let Some(a) = &self.audio {
            desk.state(F, "chkCheck(1)").on = !a.music_on;
            desk.state(F, "chkCheck(2)").on = !a.sound_on;
        }
        let mut v = View::default();
        v.show("cmdCommand(3)", true).text("cmdCommand(3)", "&Import Game...");
        for k in [
            "chkCheck(0)", "chkCheck(3)", "chkCheck(4)", "chkCheck(5)", "chkCheck(6)", "chkCheck(7)", "chkCheck(8)", "chkCheck(9)", "chkCheck(10)", "chkCheck(11)", "cmdCommand(1)", "cmdCommand(2)", "cmdCommand(4)", "cmdCommand(5)", "cmdCommand(6)",
            "hsbVolume(0)", "hsbVolume(1)",
        ] {
            v.enable(k, false);
        }
        v.text("lblBackup", "").text("lblLabel(2)", "Courier New").text("lblLabel(3)", "MS Sans Serif");
        let ev = desk.window(ctx, F, v, true);
        if let Some(a) = self.audio.as_mut() {
            a.music_on = !desk.state(F, "chkCheck(1)").on;
            a.sound_on = !desk.state(F, "chkCheck(2)").on;
        }
        if ev.click("cmdCommand(3)") {
            self.dialog = Some(Dialog::Import);
        }
        ev.click("cmdCommand(0)") || ev.click("cmdCommand(7)") || ev.closed
    }

    /// The remake's import window: the player's own copy of the game, from a ZIP, its
    /// MORDOR.WIP or the install folder's files.
    fn import(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        #[allow(unused_mut)]
        let mut v = View::default();
        #[cfg(target_arch = "wasm32")]
        v.show("txtPath", false).text("cmdImport", "Choose &Files...").show("cmdFolder", true);
        let ev = desk.window(ctx, "dlgImport", v, true);
        #[cfg(not(target_arch = "wasm32"))]
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

    /// The playing screens: the character, objects, messages and party windows, with
    /// the city (and an open building) or the dungeon.
    fn play(&mut self, desk: &mut Desk, ctx: &egui::Context, d: &GameData, t: &Tuning) {
        stats::arrange(desk);
        if self.screen == Screen::Town {
            self.town(desk, ctx, d, t);
        } else {
            self.dungeon(desk, ctx, d, t);
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
        if self.screen == Screen::Dungeon {
            self.automap_window(desk, ctx, d);
        }
        if let Some(b) = self.building {
            self.building(desk, ctx, d, t, b);
        }
    }
}

/// The remake's own windows, in the original's style.
fn add_windows(desk: &mut Desk) {
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
    party::add_windows(desk);
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
            desk.begin(ui.max_rect());
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
            self.dialog(&mut desk, &ctx, &data);
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
            #[cfg(not(target_arch = "wasm32"))]
            {
                if self.dialog.is_none()
                    && let Some(q) = self.updater.offer()
                {
                    self.dialog = Some(Dialog::Confirm(q, Deed::Update));
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
                for w in g.log[self.heard..].iter().filter_map(|l| audio::effect_for(l)).take(3) {
                    a.effect(&dir, w);
                }
                self.heard = g.log.len();
            }
        }
        self.data = self.data.take().or(Some(data));
        self.tuning = Ok(tuning);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
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
