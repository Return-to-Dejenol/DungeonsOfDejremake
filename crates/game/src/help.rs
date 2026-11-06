//! The HelpLesson: MORDOR.HLP in a Windows Help 3.1 window, as the original opened it (the
//! menu's and toolbar's help buttons, F1 for the window in front, and character
//! creation's statistics and tutorial buttons). The main window has WinHelp's button bar;
//! the help's secondary windows (glossary, tutorial, statistics) open where it places them.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;

use ddj_extract::forms::Val;
use ddj_extract::hlp::{Block, Help, Item, Pic};
use eframe::egui::{self, Color32, Painter, Rect, TextureHandle, TextureId, pos2, vec2};

use crate::win::{Cv, Desk, Ft, View};

/// Help units (half points, 10 twips) in pixels.
const U: f32 = 2.0 / 3.0;
/// The scroll bar's step, in pixels.
const STEP: f32 = 8.0;
/// The button bar's height and the topic's margin.
const BAR: i32 = 28;
const MARGIN: f32 = 6.0;
const BUTTONS: [&str; 6] = ["&Contents", "&Search", "&Back", "His&tory", "&<<", "&>>"];
const SEARCH: &str = "frmHelpSearch";
const HISTORY: &str = "frmHelpHistory";

/// A piece of a laid-out topic: text in one font, or a picture, maybe a jump.
struct Run {
    r: Rect,
    text: String,
    font: usize,
    pic: Option<TextureId>,
    link: Option<(usize, usize)>,
}

/// A topic laid out: the non-scrolling region and the rest, each with its height.
type Laid = ((Vec<Run>, f32), (Vec<Run>, f32));

struct Pane {
    win: usize,
    topic: usize,
    laid: Option<Laid>,
}

pub struct Viewer {
    help: Help,
    panes: Vec<Pane>,
    back: Vec<usize>,
    history: Vec<usize>,
    pics: HashMap<Pic, Option<TextureHandle>>,
    /// The Search window (with the topics of the keyword shown) or History, if open.
    search: Option<Vec<usize>>,
    typed: String,
    show_history: bool,
    /// Wheel movement not yet a whole scroll step.
    wheel: f32,
}

/// A help font as the remake draws it.
fn ft(help: &Help, n: usize) -> Ft {
    let Some(f) = help.fonts.get(n) else { return Ft { name: "Arial".into(), size: 10.0, bold: false, italic: false } };
    let name = match f.face.as_str() {
        "Helv" | "MS Sans Serif" | "System" => "MS Sans Serif",
        "Courier New" | "Courier" => "Courier New",
        "Times New Roman" | "Tms Rmn" | "Bookman Old Style" => "Times New Roman",
        _ => "Arial",
    };
    Ft { name: name.into(), size: f.size, bold: f.bold || f.face == "System", italic: f.italic }
}

/// Ends a line: its pieces sit on a common bottom, aligned left (0), right (1) or centred.
fn flush(line: &mut Vec<Run>, out: &mut Vec<Run>, y: &mut f32, lh: f32, align: u8, right: f32) {
    let h = line.iter().map(|r| r.r.height()).fold(lh, f32::max);
    let free = (right - line.last().map_or(right, |r| r.r.right())).max(0.0);
    let shift = match align {
        1 => free,
        2 => (free / 2.0).floor(),
        _ => 0.0,
    };
    for mut r in line.drain(..) {
        r.r = r.r.translate(vec2(shift, *y + h - r.r.height()));
        out.push(r);
    }
    *y += h;
}

/// Lays out paragraphs `width` pixels wide, word-wrapped, around pictures at the margins.
fn layout(desk: &Desk, ctx: &egui::Context, help: &Help, pics: &mut HashMap<Pic, Option<TextureHandle>>, blocks: &[Block], width: f32) -> (Vec<Run>, f32) {
    let (mut out, mut line) = (Vec::new(), Vec::<Run>::new());
    let (mut y, mut bottom, mut font, mut link) = (0.0f32, 0.0f32, 0, None);
    let mut row: Option<(usize, f32)> = None;
    // Pictures at a margin: (bottom, at the right, width taken).
    let mut floats: Vec<(f32, bool, f32)> = Vec::new();
    for b in blocks {
        // A table row's cells start level; the row ends below its tallest cell.
        let (x0, w) = match b.cell {
            Some((id, x, w)) => {
                if row.map(|r| r.0) != Some(id) {
                    row = Some((id, y.max(bottom)));
                }
                y = row.map_or(y, |r| r.1);
                (MARGIN + x as f32 * U, w as f32 * U)
            }
            None => {
                if row.take().is_some() {
                    y = y.max(bottom);
                }
                (MARGIN, width - 2.0 * MARGIN)
            }
        };
        let f = &b.fmt;
        let (left, right) = (x0 + f.left as f32 * U, x0 + w - f.right as f32 * U);
        let room = |y: f32, floats: &[(f32, bool, f32)]| {
            let taken = |side: bool| floats.iter().filter(|fl| fl.1 == side && fl.0 > y).map(|fl| fl.2).sum::<f32>();
            (left + taken(false), right - taken(true))
        };
        let (mut x, mut start) = (left, true);
        for it in &b.items {
            let lh = desk.line_height(&ft(help, font));
            if start && matches!(it, Item::Text(_) | Item::Pic(..) | Item::Tab) {
                y += f.above as f32 * U;
                x = room(y, &floats).0 + f.first as f32 * U;
                start = false;
            }
            match it {
                Item::Font(n) => font = usize::from(*n),
                Item::Link(t, w) => link = Some((*t, *w)),
                Item::EndLink => link = None,
                Item::Tab => {
                    let stop = f.tabs.iter().map(|&t| x0 + t as f32 * U).find(|&t| t > x + 0.5);
                    x = stop.unwrap_or_else(|| x0 + ((x - x0) / 48.0 + 1.0).floor() * 48.0);
                }
                Item::Break | Item::Par => {
                    let rm = room(y, &floats).1;
                    flush(&mut line, &mut out, &mut y, lh, f.align, rm);
                    x = room(y, &floats).0;
                    if *it == Item::Par {
                        y += f.below as f32 * U;
                        start = true;
                    }
                }
                Item::Text(s) => {
                    let t = ft(help, font);
                    for word in s.split_inclusive(' ') {
                        let (ww, adv) = (desk.width(&t, word.trim_end_matches(' ')), desk.width(&t, word));
                        if !line.is_empty() && x + ww > room(y, &floats).1 {
                            let rm = room(y, &floats).1;
                            flush(&mut line, &mut out, &mut y, lh, f.align, rm);
                            x = room(y, &floats).0;
                        }
                        // Words of one font and jump join into one piece.
                        match line.last_mut() {
                            Some(r) if r.pic.is_none() && r.font == font && r.link == link && (r.r.right() - x).abs() < 0.01 => {
                                r.text.push_str(word);
                                r.r.max.x += adv;
                            }
                            _ => line.push(Run { r: Rect::from_min_size(pos2(x, 0.0), vec2(adv, lh)), text: word.into(), font, pic: None, link }),
                        }
                        x += adv;
                    }
                }
                Item::Pic(p, side) => {
                    let tex = pics.entry(p.clone()).or_insert_with(|| {
                        let (w, h, px) = help.picture(p)?;
                        Some(ctx.load_texture(format!("help{p:?}"), egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::NEAREST))
                    });
                    let Some(tex) = tex else { continue };
                    let size = tex.size_vec2();
                    let (l, r) = room(y, &floats);
                    if *side == 0 {
                        if !line.is_empty() && x + size.x > r {
                            flush(&mut line, &mut out, &mut y, lh, f.align, r);
                            x = room(y, &floats).0;
                        }
                        line.push(Run { r: Rect::from_min_size(pos2(x, 0.0), size), text: String::new(), font, pic: Some(tex.id()), link });
                        x += size.x;
                    } else {
                        let at = if *side == 1 { l } else { r - size.x };
                        out.push(Run { r: Rect::from_min_size(pos2(at, y), size), text: String::new(), font, pic: Some(tex.id()), link });
                        floats.push((y + size.y, *side == 2, size.x + 6.0));
                        if line.is_empty() && *side == 1 {
                            x = x.max(at + size.x + 6.0);
                        }
                    }
                }
            }
        }
        if !line.is_empty() {
            let lh = desk.line_height(&ft(help, font));
            let rm = room(y, &floats).1;
            flush(&mut line, &mut out, &mut y, lh, f.align, rm);
        }
        bottom = bottom.max(y);
    }
    let end = floats.iter().map(|f| f.0).fold(y.max(bottom), f32::max);
    (out, end)
}

/// Draws laid-out pieces moved down `dy`, those meeting `clip` (both in the pane's pixels).
fn draw(desk: &Desk, cv: &Cv, help: &Help, runs: &[Run], dy: f32, clip: Rect) {
    for r in runs {
        let rr = r.r.translate(vec2(0.0, dy));
        if !rr.intersects(clip) {
            continue;
        }
        if let Some(t) = r.pic {
            cv.p.image(t, cv.r(rr), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            continue;
        }
        let f = help.fonts.get(r.font);
        let col = if r.link.is_some() { Color32::from_rgb(0, 128, 0) } else { f.and_then(|f| f.color).map_or(Color32::BLACK, |c| Color32::from_rgb(c[0], c[1], c[2])) };
        let t = ft(help, r.font);
        desk.label_text(cv, Rect::from_min_size(rr.min, vec2(rr.width() + 99.0, rr.height())), &t, &r.text, col, 0, false, false);
        if r.link.is_some() || f.is_some_and(|f| f.under) {
            cv.fill(rr.left(), rr.bottom() - 2.0, desk.width(&t, r.text.trim_end()), 1.0, col);
        }
    }
}

impl Viewer {
    /// The HelpLesson from the imported copy, else from the shareware the game carries.
    pub fn load(dir: &Path, shareware: &[u8]) -> Option<Self> {
        let file = dir.join("MORDOR.HLP");
        if ddj_extract::vfs::read(&file).is_err() {
            let _ = ddj_extract::import_only(vec![("MORDOR11.ZIP".into(), shareware.to_vec())], dir, "MORDOR.HLP");
        }
        let help = Help::parse(&ddj_extract::vfs::read(&file).ok()?)?;
        Some(Self { help, panes: Vec::new(), back: Vec::new(), history: Vec::new(), pics: HashMap::new(), search: None, typed: String::new(), show_history: false, wheel: 0.0 })
    }

    /// The topic for a window's HelpContextID (the contents when it has none).
    pub fn context(&self, id: i64) -> usize {
        i32::try_from(id).ok().and_then(|id| self.help.map.get(&id)).copied().unwrap_or(self.help.contents)
    }

    pub fn contents(&self) -> usize {
        self.help.contents
    }

    /// A secondary window by name (`stats`, `tutorial`), else the main one.
    pub fn window(&self, name: &str) -> usize {
        self.help.windows.iter().position(|w| w.name == name).unwrap_or(0)
    }

    /// Shows a topic in a window, opening it where the help places it.
    pub fn go(&mut self, desk: &mut Desk, win: usize, topic: usize) {
        let form = format!("frmHelp{win}");
        match self.panes.iter_mut().find(|p| p.win == win) {
            Some(p) => {
                if win == 0 && p.topic != topic {
                    self.back.push(p.topic);
                }
                (p.topic, p.laid) = (topic, None);
            }
            None => self.panes.push(Pane { win, topic, laid: None }),
        }
        // Made where the help places it; a secondary window widens to show a whole table,
        // where WinHelp let the player resize it.
        if win > 0 || !desk.forms.contains_key(&form) {
            let w = &self.help.windows[win.min(self.help.windows.len() - 1)];
            let px = |v: i32, of: i32| (v * of / 1024).clamp(0, of);
            let (x, y, ow, oh) = (px(w.rect[0], 800), px(w.rect[1], 600), px(w.rect[2] + 1, 800), px(w.rect[3] + 1, 600));
            let table = self.help.topics[topic].body.iter().filter_map(|b| b.cell).map(|(_, x, w)| ((x + w) as f32 * U + 2.0 * MARGIN) as i32 + 25).max();
            let (cw, ch) = (ow.max(120).max(table.unwrap_or(0)).min(800) - 6, oh.max(80) - 24);
            let top = if win == 0 { BAR } else { 0 };
            let mut ctls: Vec<crate::win::Synth> = vec![("VScrollBar", "vsbTopic", None, [cw - 17, top + 1, 16, ch - top - 2], ""), ("PictureBox", "picTopic", None, [0, top, cw, ch - top], "")];
            if win == 0 {
                ctls.extend(BUTTONS.iter().enumerate().map(|(i, b)| ("CommandButton", "cmdButton", Some(i as u16), [3 + 63 * i as i32, 3, 60, 22], *b)));
            }
            let caption = if win == 0 { self.help.title.clone() } else { w.caption.clone() };
            desk.synth(&form, &caption, cw, ch, &ctls);
            desk.pos.entry(form.clone()).or_insert(pos2(x as f32, y as f32));
        }
        desk.state(&form, "vsbTopic").top = 0;
        if win == 0 {
            self.history.retain(|&t| t != topic);
            self.history.insert(0, topic);
            self.history.truncate(40);
        }
        desk.active = Some(form);
    }

    /// Draws the open help windows; false once the main one (or the last) is closed.
    pub fn show(&mut self, desk: &mut Desk, ctx: &egui::Context) -> bool {
        let mut nav = None;
        let mut i = 0;
        while i < self.panes.len() {
            match self.pane(desk, ctx, i) {
                Ok(n) => {
                    nav = nav.or(n);
                    i += 1;
                }
                Err(()) if self.panes[i].win == 0 => return false,
                Err(()) => {
                    self.panes.remove(i);
                }
            }
        }
        if self.search.is_some() {
            nav = nav.or(self.search_window(desk, ctx));
        }
        if self.show_history {
            nav = nav.or(self.history_window(desk, ctx));
        }
        if let Some((w, t)) = nav {
            self.go(desk, w, t);
        }
        !self.panes.is_empty()
    }

    /// One window: Ok with where a jump or button goes, or Err once closed.
    fn pane(&mut self, desk: &mut Desk, ctx: &egui::Context, i: usize) -> Result<Option<(usize, usize)>, ()> {
        let (win, topic) = (self.panes[i].win, self.panes[i].topic);
        let form = format!("frmHelp{win}");
        let size = desk.size(&form);
        let top = if win == 0 { BAR as f32 } else { 0.0 };
        // Inside the topic area's border.
        let (w, h) = (size.x - 2.0, size.y - top - 2.0);
        if self.panes[i].laid.is_none() {
            let t = &self.help.topics[topic];
            let head = layout(desk, ctx, &self.help, &mut self.pics, &t.head, w);
            let head_h = if head.0.is_empty() { 0.0 } else { head.1 + 4.0 };
            let mut body = layout(desk, ctx, &self.help, &mut self.pics, &t.body, w);
            if body.1 + 4.0 > h - head_h {
                body = layout(desk, ctx, &self.help, &mut self.pics, &t.body, w - 17.0);
            }
            self.panes[i].laid = Some((head, body));
        }
        let Some(((head, hh), (body, bh))) = &self.panes[i].laid else { return Ok(None) };
        let head_h = if head.is_empty() { 0.0 } else { hh + 4.0 };
        let view = h - head_h;
        let max = ((bh + 4.0 - view) / STEP).ceil().max(0.0) as usize;
        if let Some(c) = desk.forms.get_mut(&form).and_then(|f| f.controls.iter_mut().find(|c| c.name == "vsbTopic")) {
            c.props.insert("Max".into(), Val::N(max as i64));
            c.props.insert("LargeChange".into(), Val::N((view / STEP).max(1.0) as i64));
        }
        let scroll = desk.state(&form, "vsbTopic").top.min(max);
        let slot: Cell<Option<(Painter, Rect, f32)>> = Cell::new(None);
        let mut v = View::default();
        v.paint("picTopic", |p, r, s| slot.set(Some((p.clone(), r, s))));
        v.show("vsbTopic", max > 0).rect("vsbTopic", [size.x as i32 - 17, (top + 1.0 + head_h) as i32, 16, view as i32]);
        if win == 0 {
            let t = &self.help.topics[topic];
            v.enable("cmdButton(2)", !self.back.is_empty()).enable("cmdButton(4)", t.prev.is_some()).enable("cmdButton(5)", t.next.is_some());
        }
        let ev = desk.window(ctx, &form, v, true);
        desk.raise(&form);
        if ev.closed {
            return Err(());
        }
        let mut nav = None;
        if let Some((p, r, s)) = slot.take() {
            let cv = Cv { p: p.clone(), o: r.min, s };
            let back = |c: Option<[u8; 3]>, d: Color32| c.map_or(d, |c| Color32::from_rgb(c[0], c[1], c[2]));
            let wi = &self.help.windows[win.min(self.help.windows.len() - 1)];
            if head_h > 0.0 {
                cv.fill(0.0, 0.0, w, head_h - 1.0, back(wi.head, Color32::WHITE));
                cv.fill(0.0, head_h - 1.0, w, 1.0, Color32::BLACK);
                draw(desk, &cv, &self.help, head, 2.0, Rect::from_min_size(pos2(0.0, 0.0), vec2(w, head_h)));
            }
            let body_r = Rect::from_min_max(pos2(0.0, head_h), pos2(w, h));
            let bcv = Cv { p: p.with_clip_rect(cv.r(body_r).intersect(p.clip_rect())), o: r.min, s };
            if wi.back.is_some() {
                bcv.fill(0.0, head_h, w, view, back(wi.back, Color32::WHITE));
            }
            let dy = head_h + 2.0 - scroll as f32 * STEP;
            draw(desk, &bcv, &self.help, body, dy, body_r);
            // Jumps: the hand over hot text, and a click follows it.
            let ours = |pos| ctx.layer_id_at(pos) == Some(p.layer_id());
            if let Some(pos) = ctx.pointer_hover_pos().filter(|&pos| r.contains(pos) && ours(pos)) {
                let at = ((pos - r.min) / s).to_pos2();
                let hit = |runs: &[Run], dy: f32| runs.iter().find(|x| x.link.is_some() && x.r.translate(vec2(0.0, dy)).contains(at)).and_then(|x| x.link);
                let link = if at.y < head_h { hit(head, 2.0) } else { hit(body, dy) };
                if let Some((t, lw)) = link {
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    if ev.click("picTopic") {
                        nav = Some((if lw == 0 { win } else { lw }, t));
                    }
                }
                if at.y >= head_h {
                    self.wheel += ctx.input(|i| i.smooth_scroll_delta.y) / s / STEP;
                    let n = self.wheel.trunc();
                    self.wheel -= n;
                    desk.state(&form, "vsbTopic").top = (scroll as f32 - n).clamp(0.0, max as f32) as usize;
                }
            }
        }
        let (prev, next) = (self.help.topics[topic].prev, self.help.topics[topic].next);
        match ev.index("cmdButton") {
            Some(0) => nav = Some((0, self.help.contents)),
            Some(1) => {
                self.search = Some(Vec::new());
                desk.focus = Some(format!("{SEARCH}.txtWord"));
            }
            Some(2) => {
                if let Some(b) = self.back.pop() {
                    self.go(desk, 0, b);
                    self.back.pop();
                }
            }
            Some(3) => self.show_history = true,
            Some(4) => nav = prev.map(|p| (0, p)),
            Some(5) => nav = next.map(|n| (0, n)),
            _ => {}
        }
        Ok(nav)
    }

    /// WinHelp's Search: a keyword typed or picked, its topics, then one to go to.
    fn search_window(&mut self, desk: &mut Desk, ctx: &egui::Context) -> Option<(usize, usize)> {
        if !desk.forms.contains_key(SEARCH) {
            desk.synth(SEARCH, "Search", 380, 280, &[
                ("Label", "lblWord", None, [8, 8, 264, 28], "Type a &word, or select one from the list.\nThen choose Show Topics."),
                ("TextBox", "txtWord", None, [8, 40, 264, 20], ""),
                ("ListBox", "lstWords", None, [8, 62, 264, 100], ""),
                ("CommandButton", "cmdCancel", None, [282, 8, 90, 24], "Cancel"),
                ("CommandButton", "cmdShow", None, [282, 40, 90, 24], "&Show Topics"),
                ("Label", "lblTopic", None, [8, 174, 264, 16], "Select a &topic, then choose Go To."),
                ("ListBox", "lstTopics", None, [8, 192, 264, 80], ""),
                ("CommandButton", "cmdGo", None, [282, 192, 90, 24], "&Go To"),
            ]);
        }
        let typed = desk.state(SEARCH, "txtWord").text.clone();
        if typed != self.typed {
            let low = typed.to_lowercase();
            if let Some(i) = self.help.keywords.iter().position(|k| k.0.to_lowercase().starts_with(&low)).filter(|_| !low.is_empty()) {
                let st = desk.state(SEARCH, "lstWords");
                (st.sel, st.top) = (Some(i), i);
            }
            self.typed = typed;
        }
        let found = self.search.clone().unwrap_or_default();
        let mut v = View::default();
        v.items("lstWords", self.help.keywords.iter().map(|k| k.0.clone()).collect());
        v.items("lstTopics", found.iter().map(|&t| self.help.topics[t].title.clone()).collect());
        v.enable("cmdGo", !found.is_empty());
        let ev = desk.window(ctx, SEARCH, v, true);
        desk.raise(SEARCH);
        let word = desk.state(SEARCH, "lstWords").sel.and_then(|i| self.help.keywords.get(i)).cloned();
        if ev.changed.as_deref() == Some("lstWords")
            && let Some((w, _)) = &word
        {
            desk.state(SEARCH, "txtWord").text = w.clone();
            self.typed = w.clone();
        }
        if ev.click("cmdShow") || ev.double.as_deref() == Some("lstWords") {
            self.search = Some(word.map(|w| w.1).unwrap_or_default());
            desk.state(SEARCH, "lstTopics").sel = Some(0);
        }
        let pick = desk.state(SEARCH, "lstTopics").sel.and_then(|i| found.get(i)).copied();
        if ev.click("cmdCancel") || ev.closed {
            self.search = None;
        } else if (ev.click("cmdGo") || ev.double.as_deref() == Some("lstTopics")) && pick.is_some() {
            self.search = None;
            return pick.map(|t| (0, t));
        }
        None
    }

    /// WinHelp's History: the topics seen, latest first.
    fn history_window(&mut self, desk: &mut Desk, ctx: &egui::Context) -> Option<(usize, usize)> {
        if !desk.forms.contains_key(HISTORY) {
            desk.synth(HISTORY, "Windows Help History", 240, 200, &[("ListBox", "lstHistory", None, [0, 0, 240, 200], "")]);
            desk.pos.insert(HISTORY.into(), pos2(552.0, 40.0));
        }
        let mut v = View::default();
        v.items("lstHistory", self.history.iter().map(|&t| self.help.topics[t].title.clone()).collect());
        let ev = desk.window(ctx, HISTORY, v, true);
        desk.raise(HISTORY);
        if ev.closed {
            self.show_history = false;
        }
        let pick = desk.state(HISTORY, "lstHistory").sel.and_then(|i| self.history.get(i)).copied();
        (ev.double.as_deref() == Some("lstHistory")).then_some(pick.map(|t| (0, t))).flatten()
    }
}
