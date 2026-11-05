//! The original's windows, drawn from the layouts the importer read out of MORDOR.EXE.
//! The game ran maximized on an 800x600 Windows 95 desktop; that desktop is drawn here
//! and scaled evenly to the screen with sharp pixels. Child windows sit in the main
//! window's MDI area with their title bars; each control is drawn as Windows and the
//! game's VBXs (THREED, VideoSoft) drew it. MS Sans Serif comes from the system's own
//! bitmap font when it has one (Windows); elsewhere a similar TrueType face stands in.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use ddj_extract::forms::{Control, Form, Val};
use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, TextureHandle, TextureId, Vec2, pos2, vec2};

/// The virtual screen, in original pixels.
pub const SCREEN: Vec2 = vec2(800.0, 600.0);
const CAPTION: f32 = 18.0;
const FACE: Color32 = Color32::from_rgb(192, 192, 192);
const SHADOW: Color32 = Color32::from_rgb(128, 128, 128);
const NAVY: Color32 = Color32::from_rgb(0, 0, 128);

/// A VB colour: 0x00BBGGRR, or 0x800000nn for a system colour.
pub fn vb(v: i64) -> Color32 {
    let v = v as u32;
    if v & 0x8000_0000 != 0 {
        return match v & 0xFF {
            5 | 0x0e | 0x14 => Color32::WHITE,
            0x0c | 0x10 | 0x11 => SHADOW,
            0x0d | 2 => NAVY,
            0x0f | 4 | 0x16 => FACE,
            1 => Color32::from_rgb(0, 128, 128),
            _ => Color32::BLACK,
        };
    }
    Color32::from_rgb(v as u8, (v >> 8) as u8, (v >> 16) as u8)
}

/// A drag in progress: what is carried (kind, then two numbers the screen chooses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag(pub u8, pub usize, pub usize);

type PaintFn<'a> = Box<dyn FnOnce(&Painter, Rect, f32) + 'a>;

/// A control of a window the remake builds: kind, name, index, rect, caption.
pub type Synth<'s> = (&'s str, &'s str, Option<u16>, [i32; 4], &'s str);

/// What the game changes on a form this frame, by control key (`name` or `name(i)`).
#[derive(Default)]
pub struct View<'a> {
    pub caption: Option<String>,
    text: HashMap<String, String>,
    shown: HashMap<String, bool>,
    enabled: HashMap<String, bool>,
    fore: HashMap<String, Color32>,
    back: HashMap<String, Color32>,
    items: HashMap<String, Vec<String>>,
    pics: HashMap<String, (TextureId, Rect)>,
    drags: HashMap<String, Drag>,
    rects: HashMap<String, [i32; 4]>,
    bolds: std::collections::HashSet<String>,
    paints: Vec<(String, PaintFn<'a>)>,
    /// Control-array members the original loaded at run time: (base key, new index,
    /// rect in the parent, or None to keep the base's).
    clones: Vec<(String, u16, [i32; 4])>,
}

pub fn key(name: &str, i: Option<u16>) -> String {
    match i {
        Some(i) => format!("{name}({i})"),
        None => name.to_string(),
    }
}

impl<'a> View<'a> {
    pub fn text(&mut self, k: &str, s: impl Into<String>) -> &mut Self {
        self.text.insert(k.into(), s.into());
        self
    }
    pub fn show(&mut self, k: &str, on: bool) -> &mut Self {
        self.shown.insert(k.into(), on);
        self
    }
    pub fn enable(&mut self, k: &str, on: bool) -> &mut Self {
        self.enabled.insert(k.into(), on);
        self
    }
    pub fn fore(&mut self, k: &str, c: Color32) -> &mut Self {
        self.fore.insert(k.into(), c);
        self
    }
    pub fn back(&mut self, k: &str, c: Color32) -> &mut Self {
        self.back.insert(k.into(), c);
        self
    }
    pub fn items(&mut self, k: &str, v: Vec<String>) -> &mut Self {
        self.items.insert(k.into(), v);
        self
    }
    pub fn pic(&mut self, k: &str, tex: TextureId, uv: Rect) -> &mut Self {
        self.pics.insert(k.into(), (tex, uv));
        self
    }
    /// Sets a control's font bold, as the original's code did at run time.
    pub fn bold(&mut self, k: &str) -> &mut Self {
        self.bolds.insert(k.into());
        self
    }
    /// Moves or resizes a control, as the original's code did at run time.
    pub fn rect(&mut self, k: &str, r: [i32; 4]) -> &mut Self {
        self.rects.insert(k.into(), r);
        self
    }
    pub fn drag(&mut self, k: &str, d: Drag) -> &mut Self {
        self.drags.insert(k.into(), d);
        self
    }
    /// Draws into a control after its background (the maze, the monsters).
    pub fn paint(&mut self, k: &str, f: impl FnOnce(&Painter, Rect, f32) + 'a) -> &mut Self {
        self.paints.push((k.into(), Box::new(f)));
        self
    }
    /// Loads array member `i` of `base` (`name(0)`) at `rect` in the same container,
    /// as the original did at run time.
    pub fn clone_rect(&mut self, base: &str, i: u16, rect: [i32; 4]) -> &mut Self {
        self.clones.push((base.into(), i, rect));
        self
    }
}

/// What the player did on a form this frame.
#[derive(Default, Debug)]
pub struct Ev {
    pub clicked: Option<String>,
    pub double: Option<String>,
    /// A list or combo whose selection changed, or a tab strip's new tab.
    pub changed: Option<String>,
    pub dropped: Option<(String, Drag)>,
    /// The window's close box.
    pub closed: bool,
}

impl Ev {
    pub fn click(&self, k: &str) -> bool {
        self.clicked.as_deref() == Some(k)
    }
    /// The array index of a clicked member of `name`.
    pub fn index(&self, name: &str) -> Option<u16> {
        let c = self.clicked.as_deref()?.strip_prefix(name)?.strip_prefix('(')?;
        c.strip_suffix(')')?.parse().ok()
    }
}

/// Per-control state the original kept in its controls: list selection and scroll,
/// text, check value, current tab.
#[derive(Default, Clone)]
pub struct St {
    pub sel: Option<usize>,
    pub top: usize,
    pub text: String,
    pub on: bool,
    pub tab: usize,
}

/// A bitmap font size from a Windows .FON file.
struct Fnt {
    height: f32,
    first: u8,
    glyphs: Vec<(f32, f32)>,
    tex: TextureHandle,
    atlas_w: f32,
}

/// Reads the FNT (version 2/3) fonts out of a .FON file, by point size.
fn load_fon(ctx: &egui::Context, b: &[u8]) -> Vec<(u16, Fnt)> {
    let u16_at = |o: usize| b.get(o..o + 2).map_or(0, |v| u16::from_le_bytes([v[0], v[1]]));
    let u32_at = |o: usize| b.get(o..o + 4).map_or(0, |v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
    let ne = u32_at(0x3c) as usize;
    let rt = ne + usize::from(u16_at(ne + 0x24));
    let (shift, mut p, mut out) = (u16_at(rt), rt + 2, Vec::new());
    while ne > 0 && u16_at(p) != 0 && p < b.len() {
        let (kind, n) = (u16_at(p), usize::from(u16_at(p + 2)));
        for e in (0..n).map(|j| p + 8 + 12 * j) {
            let f = &b[(usize::from(u16_at(e)) << shift).min(b.len())..];
            if kind != 0x8008 || f.len() < 118 {
                continue;
            }
            let g16 = |o: usize| f.get(o..o + 2).map_or(0, |v| u16::from_le_bytes([v[0], v[1]]));
            let (v3, h, first, last) = (g16(0) == 0x300, usize::from(g16(88)), f[95], f[96]);
            let (table, step) = if v3 { (148, 6) } else { (118, 4) };
            let chars: Vec<(usize, usize)> = (0..=usize::from(last - first))
                .map(|c| {
                    let e = table + step * c;
                    let off = if v3 { f.get(e + 2..e + 6).map_or(0, |v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]) as usize) } else { usize::from(g16(e + 2)) };
                    (usize::from(g16(e)), off)
                })
                .collect();
            let w: usize = chars.iter().map(|c| c.0 + 1).sum();
            let mut px = vec![Color32::TRANSPARENT; w * h];
            let (mut x0, mut glyphs) = (0, Vec::new());
            for &(cw, off) in &chars {
                for x in 0..cw {
                    for y in 0..h {
                        if f.get(off + (x / 8) * h + y).is_some_and(|byte| byte & (0x80 >> (x % 8)) != 0) {
                            px[y * w + x0 + x] = Color32::WHITE;
                        }
                    }
                }
                glyphs.push((x0 as f32, cw as f32));
                x0 += cw + 1;
            }
            let tex = ctx.load_texture(format!("fon{}", g16(68)), egui::ColorImage { size: [w, h], pixels: px, source_size: vec2(w as f32, h as f32) }, egui::TextureOptions::NEAREST);
            out.push((g16(68), Fnt { height: h as f32, first, glyphs, tex, atlas_w: w as f32 }));
        }
        p += 8 + 12 * n;
    }
    out
}

/// A control's font.
#[derive(Clone)]
pub struct Ft {
    pub name: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

/// TrueType faces from the system (Windows), registered as egui families; families with
/// no file fall back to egui's own fonts so they always exist.
pub fn register_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let dir = Path::new("C:/Windows/Fonts");
    for (family, files) in [("ms", ["micross.ttf", "micross.ttf"]), ("cour", ["cour.ttf", "courbd.ttf"]), ("times", ["times.ttf", "timesbd.ttf"]), ("arial", ["arial.ttf", "arialbd.ttf"]), ("symbol", ["symbol.ttf", "symbol.ttf"])] {
        for (bold, file) in files.iter().enumerate() {
            let name = format!("{family}{bold}");
            let mut list = Vec::new();
            if let Ok(data) = std::fs::read(dir.join(file)) {
                defs.font_data.insert(name.clone(), egui::FontData::from_owned(data).into());
                list.push(name.clone());
            }
            let fallback = if family == "cour" { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
            list.extend(defs.families[&fallback].clone());
            defs.families.insert(egui::FontFamily::Name(name.into()), list);
        }
    }
    ctx.set_fonts(defs);
}

/// The MDI desktop: the forms, their pictures, open windows and control state.
pub struct Desk {
    pub forms: BTreeMap<String, Form>,
    dir: std::path::PathBuf,
    pics: HashMap<String, Option<TextureHandle>>,
    fon: Vec<(u16, Fnt)>,
    /// Where each window's client area sits (virtual pixels), once placed or moved.
    pub pos: HashMap<String, Pos2>,
    pub st: HashMap<String, St>,
    /// The text box typed into (`form.key`).
    pub focus: Option<String>,
    combo: Option<String>,
    s: f32,
    org: Pos2,
    /// The main window's MDI area, in virtual pixels.
    pub client: Rect,
    /// The window last clicked, drawn with an active title bar.
    pub active: Option<String>,
    ctx: egui::Context,
}

impl Desk {
    pub fn new(ctx: &egui::Context, dir: &Path) -> Option<Self> {
        let forms = ddj_extract::forms::load(dir)?;
        // Windows has the bitmap font; a debug web build may be served one for testing.
        let fon = std::fs::read("C:/Windows/Fonts/sserife.fon").or_else(|_| ddj_extract::vfs::read("fonts/sserife.fon")).map(|b| load_fon(ctx, &b)).unwrap_or_default();
        Some(Self {
            forms,
            dir: dir.to_path_buf(),
            pics: HashMap::new(),
            fon,
            pos: HashMap::new(),
            st: HashMap::new(),
            focus: None,
            combo: None,
            s: 1.0,
            org: Pos2::ZERO,
            client: Rect::from_min_size(pos2(0.0, 50.0), vec2(800.0, 550.0)),
            active: None,
            ctx: ctx.clone(),
        })
    }

    /// Fits the 800x600 desktop to the screen; call once a frame before any window.
    pub fn begin(&mut self, screen: Rect) -> Painter {
        // A window not laid out yet has no size; keep a usable scale until it has.
        self.s = (screen.width() / SCREEN.x).min(screen.height() / SCREEN.y).max(0.1);
        self.org = screen.center() - SCREEN * self.s / 2.0;
        let p = self.ctx.layer_painter(egui::LayerId::background());
        p.rect_filled(screen, 0.0, Color32::BLACK);
        p
    }

    /// Virtual pixels to screen points.
    pub fn to_screen(&self, r: Rect) -> Rect {
        Rect::from_min_size(self.org + r.min.to_vec2() * self.s, r.size() * self.s)
    }

    pub fn state(&mut self, form: &str, k: &str) -> &mut St {
        self.st.entry(format!("{form}.{k}")).or_default()
    }

    fn pic(&mut self, form: &str, n: i64) -> Option<(TextureId, Vec2)> {
        let file = format!("{form}_{n}.BMP");
        let ctx = self.ctx.clone();
        let dir = self.dir.clone();
        let t = self.pics.entry(file.clone()).or_insert_with(|| {
            let (w, h, px) = ddj_extract::bmp(&ddj_extract::vfs::read(dir.join(&file)).ok()?)?;
            Some(ctx.load_texture(file, egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::NEAREST_REPEAT))
        });
        t.as_ref().map(|t| (t.id(), t.size_vec2()))
    }

    /// A control's design rectangle in its container.
    pub fn rect(&self, form: &str, k: &str) -> [i32; 4] {
        self.forms.get(form).and_then(|f| f.controls.iter().find(|c| key(&c.name, c.index) == k)).map_or([0; 4], |c| c.rect)
    }

    /// Adds a window of the remake's own, built like the original's: controls as
    /// (kind, name, index, rect, caption) in a fixed dialog `w` x `h`.
    pub fn synth(&mut self, name: &str, caption: &str, w: i32, h: i32, ctls: &[Synth]) {
        let mut f = Form { name: name.into(), rect: [0, 0, w, h], ..Default::default() };
        f.props.insert("Caption".into(), Val::S(caption.into()));
        f.props.insert("BorderStyle".into(), Val::N(3));
        for &(kind, n, index, rect, cap) in ctls {
            let mut c = Control { name: n.into(), index, kind: kind.into(), rect, ..Default::default() };
            c.props.insert(if kind == "TextBox" { "Text" } else { "Caption" }.into(), Val::S(cap.into()));
            f.controls.push(c);
        }
        self.forms.insert(name.into(), f);
    }

    /// Makes a control start hidden, as if designed so.
    pub fn hide_by_default(&mut self, form: &str, k: &str) {
        if let Some(c) = self.forms.get_mut(form).and_then(|f| f.controls.iter_mut().find(|c| key(&c.name, c.index) == k)) {
            c.props.insert("Visible".into(), Val::N(0));
        }
    }

    /// The window's client size, in virtual pixels.
    pub fn size(&self, form: &str) -> Vec2 {
        self.forms.get(form).map_or(vec2(100.0, 100.0), |f| vec2(f.rect[2] as f32, f.rect[3] as f32))
    }

    /// Frame thickness and whether it has a title bar (Windows leaves it off a window
    /// with no caption and no control box).
    fn frame(f: &Form) -> (f32, bool) {
        let cap = !(text(&f.props, "Caption").is_empty() && num(&f.props, "ControlBox") == Some(0));
        match num(&f.props, "BorderStyle").unwrap_or(2) {
            0 => (0.0, false),
            1 => (1.0, cap),
            3 => (3.0, cap),
            _ => (4.0, cap),
        }
    }

    /// The window's outer size for a client size.
    pub fn outer(&self, form: &str) -> Vec2 {
        let Some(f) = self.forms.get(form) else { return Vec2::ZERO };
        let (b, cap) = Self::frame(f);
        self.size(form) + vec2(2.0 * b, 2.0 * b + if cap { CAPTION } else { 0.0 })
    }

    /// Draws the main window (maximized: its title bar, then its client area, the
    /// toolbar on top) and returns its events.
    pub fn main(&mut self, ui: &mut egui::Ui, title: &str, v: View) -> Ev {
        let Some(f) = self.forms.get("frmMain").cloned() else { return Ev::default() };
        let p = ui.painter().clone();
        let cv = Cv { p: p.clone(), o: self.org, s: self.s };
        cv.fill(0.0, 0.0, SCREEN.x, SCREEN.y, SHADOW);
        cv.fill(0.0, 0.0, SCREEN.x, CAPTION, NAVY);
        self.label_text(&cv, Rect::from_min_size(pos2(4.0, 0.0), vec2(700.0, CAPTION)), &Ft { name: "MS Sans Serif".into(), size: 8.25, bold: true, italic: false }, title, Color32::WHITE, 0, true, false);
        let bar = f.controls.iter().find(|c| c.name == "picToolbar").map_or(32.0, |c| c.rect[3] as f32);
        self.client = Rect::from_min_size(pos2(0.0, CAPTION + bar), vec2(SCREEN.x, SCREEN.y - CAPTION - bar));
        let bar = if v.shown.get("picToolbar") == Some(&true) { bar } else { 0.0 };
        self.client = Rect::from_min_size(pos2(0.0, CAPTION + bar), vec2(SCREEN.x, SCREEN.y - CAPTION - bar));
        // The MDI area is tiled with the stone picture the original kept on picPalette.
        let stone = f.controls.iter().find(|c| c.name == "picPalette").and_then(|c| num(&c.props, "Picture")).and_then(|n| self.pic("frmMain", n));
        if let Some((tex, size)) = stone {
            let cl = self.client;
            let mut m = egui::Mesh::with_texture(tex);
            m.add_rect_with_uv(cv.r(cl), Rect::from_min_max(Pos2::ZERO, pos2(cl.width() / size.x, cl.height() / size.y)), Color32::WHITE);
            p.add(m);
        }
        let mut ev = Ev::default();
        let origin = pos2(0.0, CAPTION);
        self.draw(ui, &f, origin, vec2(SCREEN.x, bar), v, &mut ev);
        ev
    }

    /// Keeps a dialog above the others (the help's windows over the game's).
    pub fn raise(&self, form: &str) {
        self.ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(("win", form))));
    }

    /// Draws an MDI child window (or, with `dialog`, a window above everything, centred
    /// unless placed) and returns what the player did on it.
    pub fn window(&mut self, ctx: &egui::Context, form: &str, mut v: View, dialog: bool) -> Ev {
        let Some(f) = self.forms.get(form).cloned() else { return Ev::default() };
        let (b, cap) = Self::frame(&f);
        let outer = self.outer(form);
        let at = *self.pos.entry(form.to_string()).or_insert_with(|| {
            if dialog { (SCREEN - outer).to_pos2() / 2.0 } else { pos2(f.rect[0] as f32, f.rect[1] as f32) }
        });
        let bounds = if dialog { Rect::from_min_size(Pos2::ZERO, SCREEN) } else { self.client };
        let at = pos2(at.x.clamp(bounds.left(), (bounds.right() - outer.x).max(bounds.left())), at.y.clamp(bounds.top(), (bounds.bottom() - outer.y).max(bounds.top())));
        let mut ev = Ev::default();
        let order = if dialog { egui::Order::Foreground } else { egui::Order::Middle };
        let clip = self.to_screen(bounds);
        let screen_at = self.to_screen(Rect::from_min_size(at, outer));
        let resp = egui::Area::new(egui::Id::unique(("win", form))).order(order).fixed_pos(screen_at.min).constrain(false).show(ctx, |ui| {
            ui.set_clip_rect(clip);
            let (r, bg) = ui.allocate_exact_size(screen_at.size(), Sense::click());
            let cv = Cv { p: ui.painter().clone(), o: r.min, s: self.s };
            let active = dialog || self.active.as_deref() == Some(form);
            // The frame, title bar and close box.
            cv.edge(0.0, 0.0, outer.x, outer.y, true);
            cv.fill(2.0, 2.0, outer.x - 4.0, outer.y - 4.0, FACE);
            let mut title_drag = None;
            if cap {
                let bar = Rect::from_min_size(pos2(b, b), vec2(outer.x - 2.0 * b, CAPTION - 1.0));
                cv.fill(bar.left(), bar.top(), bar.width(), bar.height(), if active { NAVY } else { SHADOW });
                let caption = v.caption.clone().unwrap_or_else(|| text(&f.props, "Caption"));
                let tc = if active { Color32::WHITE } else { FACE };
                self.label_text(&cv, bar.translate(vec2(3.0, 0.0)), &Ft { name: "MS Sans Serif".into(), size: 8.25, bold: true, italic: false }, &caption, tc, 0, true, false);
                let resp = ui.interact(cv.r(bar), ui.scope_id().with("title"), Sense::drag());
                if resp.dragged() {
                    title_drag = Some(resp.drag_delta() / self.s);
                }
                if num(&f.props, "ControlBox").unwrap_or(1) != 0 {
                    let x = Rect::from_min_size(pos2(bar.right() - 18.0, bar.top() + 2.0), vec2(16.0, 14.0));
                    let hit = ui.interact(cv.r(x), ui.scope_id().with("close"), Sense::click());
                    cv.edge(x.left(), x.top(), x.width(), x.height(), !hit.is_pointer_button_down_on());
                    for k in 0..2 {
                        let (a, z) = (x.left() + 4.0 + k as f32, x.right() - 5.0 + k as f32);
                        cv.line(pos2(a, x.top() + 3.0), pos2(z, x.bottom() - 4.0), Color32::BLACK);
                        cv.line(pos2(z, x.top() + 3.0), pos2(a, x.bottom() - 4.0), Color32::BLACK);
                    }
                    ev.closed = hit.clicked();
                    let sizable = num(&f.props, "BorderStyle").unwrap_or(2) == 2;
                    for (n, prop) in [(1.0, "MaxButton"), (2.0, "MinButton")] {
                        if !sizable || num(&f.props, prop) == Some(0) {
                            continue;
                        }
                        let bx = Rect::from_min_size(pos2(x.left() - 2.0 - 16.0 * n, x.top()), x.size());
                        cv.edge(bx.left(), bx.top(), bx.width(), bx.height(), true);
                        if prop == "MaxButton" {
                            cv.frame(bx.left() + 3.0, bx.top() + 2.0, 9.0, 9.0, Color32::BLACK);
                            cv.fill(bx.left() + 3.0, bx.top() + 3.0, 9.0, 1.0, Color32::BLACK);
                        } else {
                            cv.fill(bx.left() + 4.0, bx.bottom() - 5.0, 6.0, 2.0, Color32::BLACK);
                        }
                    }
                }
            }
            let origin = pos2(b, b + if cap { CAPTION } else { 0.0 });
            let mut inner = Ev::default();
            v.caption = None;
            self.draw_in(ui, &cv, &f, origin, self.size(form), &mut v, &mut inner);
            ev = Ev { closed: ev.closed, ..inner };
            (bg.clicked() || bg.drag_started() || ui.rect_contains_pointer(r) && ui.input(|i| i.pointer.any_pressed()), title_drag)
        });
        let (pressed, moved) = resp.inner;
        if pressed || ev.clicked.is_some() {
            self.active = Some(form.to_string());
        }
        if let Some(d) = moved {
            self.pos.insert(form.to_string(), at + d);
        }
        ev
    }

    fn draw(&mut self, ui: &mut egui::Ui, f: &Form, origin: Pos2, size: Vec2, mut v: View, ev: &mut Ev) {
        let cv = Cv { p: ui.painter().clone(), o: self.org, s: self.s };
        self.draw_in(ui, &cv, f, origin, size, &mut v, ev);
    }

    /// The form's client area and its controls, back to front.
    #[allow(clippy::too_many_arguments)]
    fn draw_in(&mut self, ui: &mut egui::Ui, cv: &Cv, f: &Form, origin: Pos2, size: Vec2, v: &mut View, ev: &mut Ev) {
        let back = num(&f.props, "BackColor").map_or(FACE, vb);
        cv.fill(origin.x, origin.y, size.x, size.y, back);
        // Run-time members of control arrays join their base's container.
        let mut ctls: Vec<Control> = f.controls.clone();
        for (base, i, rect) in std::mem::take(&mut v.clones) {
            if let Some(c) = f.controls.iter().find(|c| key(&c.name, c.index) == base) {
                ctls.push(Control { index: Some(i), rect, ..c.clone() });
            }
        }
        self.children(ui, cv, &f.name, &ctls, None, origin, Rect::from_min_size(origin, size), v, ev);
    }

    #[allow(clippy::too_many_arguments)]
    fn children(&mut self, ui: &mut egui::Ui, cv: &Cv, form: &str, ctls: &[Control], parent: Option<usize>, origin: Pos2, clip: Rect, v: &mut View, ev: &mut Ev) {
        // Graphical controls paint on the container, under its windowed controls; later
        // records lie further back.
        let graphical = |c: &Control| matches!(c.kind.as_str(), "Label" | "Image" | "Shape" | "Line");
        let mine: Vec<usize> = (0..ctls.len()).filter(|&i| ctls[i].parent == parent).collect();
        let order: Vec<usize> = mine.iter().rev().filter(|&&i| graphical(&ctls[i])).chain(mine.iter().rev().filter(|&&i| !graphical(&ctls[i]))).copied().collect();
        for i in order {
            let c = &ctls[i];
            let k = key(&c.name, c.index);
            let shown = v.shown.get(&k).copied().unwrap_or(num(&c.props, "Visible").unwrap_or(1) != 0);
            if !shown || matches!(c.kind.as_str(), "Timer" | "msgblaster" | "ctl3d" | "commondialog" | "Menu") {
                continue;
            }
            let cr = v.rects.get(&k).copied().unwrap_or(c.rect);
            let r = Rect::from_min_size(origin + vec2(cr[0] as f32, cr[1] as f32), vec2(cr[2] as f32, cr[3] as f32));
            let cv2 = Cv { p: cv.p.with_clip_rect(cv.r(clip).intersect(cv.p.clip_rect())), o: cv.o, s: cv.s };
            let inner = self.control(ui, &cv2, form, c, &k, r, v, ev);
            if let Some(inner) = inner
                && ctls.iter().any(|x| x.parent == Some(i))
            {
                self.children(ui, &cv2, form, ctls, Some(i), inner.min, inner.intersect(clip), v, ev);
            }
        }
    }

    /// Draws one control; returns its client rectangle when it can hold others.
    #[allow(clippy::too_many_arguments)]
    fn control(&mut self, ui: &mut egui::Ui, cv: &Cv, form: &str, c: &Control, k: &str, r: Rect, v: &mut View, ev: &mut Ev) -> Option<Rect> {
        let pr = &c.props;
        let caption = v.text.get(k).cloned().unwrap_or_else(|| text(pr, if c.kind == "TextBox" { "Text" } else { "Caption" }));
        let enabled = v.enabled.get(k).copied().unwrap_or(num(pr, "Enabled").unwrap_or(1) != 0);
        let fore = v.fore.get(k).copied().unwrap_or_else(|| num(pr, "ForeColor").map_or(Color32::BLACK, vb));
        let ink = if enabled { fore } else { SHADOW };
        let ft = Ft { bold: font(pr, &c.kind).bold || v.bolds.contains(k), ..font(pr, &c.kind) };
        let align = num(pr, "Alignment").unwrap_or(0) as u8;
        let id = ui.scope_id().with((form, k));
        let back_of = |dflt: Color32| v.back.get(k).copied().unwrap_or_else(|| num(pr, "BackColor").map_or(dflt, vb));
        let sense = if v.drags.contains_key(k) { Sense::click_and_drag() } else { Sense::click() };
        let resp = ui.interact(cv.r(r), id, sense);
        if let Some(d) = v.drags.get(k) {
            resp.dnd_set_drag_payload(*d);
        }
        if let Some(d) = resp.dnd_release_payload::<Drag>() {
            ev.dropped = Some((k.to_string(), *d));
        }
        let clicked = enabled && resp.clicked();
        if clicked {
            ev.clicked = Some(k.to_string());
        }
        if enabled && resp.double_clicked() {
            ev.double = Some(k.to_string());
        }
        let down = enabled && resp.is_pointer_button_down_on();
        let paint_here = |cv: &Cv, rr: Rect, v: &mut View| {
            while let Some(p) = v.paints.iter().position(|(pk, _)| pk == k) {
                let (_, f) = v.paints.remove(p);
                f(&cv.p.with_clip_rect(cv.r(rr).intersect(cv.p.clip_rect())), cv.r(rr), cv.s);
            }
        };
        let (x, y, w, h) = (r.left(), r.top(), r.width(), r.height());
        match c.kind.as_str() {
            "Label" => {
                if num(pr, "BackStyle").unwrap_or(1) != 0 {
                    cv.fill(x, y, w, h, back_of(FACE));
                }
                let mut t = r;
                if num(pr, "BorderStyle").unwrap_or(0) == 1 {
                    cv.box1(x, y, w, h, SHADOW, Color32::WHITE);
                    t = t.shrink(1.0);
                }
                self.label_text(cv, t, &ft, &caption, ink, align, false, true);
                paint_here(cv, r, v);
                None
            }
            "CommandButton" | "sscommand" => {
                cv.fill(x, y, w, h, FACE);
                if c.kind == "sscommand" && num(pr, "Outline").unwrap_or(1) != 0 {
                    cv.frame(x, y, w, h, Color32::BLACK);
                    cv.button(x + 1.0, y + 1.0, w - 2.0, h - 2.0, down);
                } else {
                    cv.button(x, y, w, h, down);
                }
                let off = if down { 1.0 } else { 0.0 };
                if let Some(n) = num(pr, "Picture")
                    && let Some((tex, sz)) = self.pic(form, n)
                {
                    let at = r.center() - sz / 2.0 + vec2(off, off);
                    cv.p.image(tex, cv.r(Rect::from_min_size(at.floor(), sz)), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), if enabled { Color32::WHITE } else { Color32::from_gray(160) });
                }
                if !enabled && !caption.is_empty() {
                    self.label_text(cv, r.translate(vec2(1.0, 1.0)), &ft, &caption, Color32::WHITE, 2, true, true);
                }
                self.label_text(cv, r.translate(vec2(off, off)), &ft, &caption, ink, 2, true, true);
                None
            }
            "Image" => {
                let pic = v.pics.get(k).copied().or_else(|| num(pr, "Picture").and_then(|n| self.pic(form, n)).map(|(t, _)| (t, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)))));
                if let Some((tex, uv)) = pic {
                    let size = if num(pr, "Stretch").unwrap_or(0) != 0 { r.size() } else { num(pr, "Picture").and_then(|n| self.pic(form, n)).map_or(r.size(), |(_, s)| s) };
                    cv.p.image(tex, cv.r(Rect::from_min_size(r.min, size)), uv, Color32::WHITE);
                }
                if num(pr, "BorderStyle").unwrap_or(0) == 1 {
                    cv.frame(x, y, w, h, Color32::BLACK);
                }
                paint_here(cv, r, v);
                None
            }
            "PictureBox" => {
                cv.fill(x, y, w, h, back_of(Color32::WHITE));
                let b = if num(pr, "BorderStyle").unwrap_or(1) == 1 {
                    cv.frame(x, y, w, h, Color32::BLACK);
                    1.0
                } else {
                    0.0
                };
                let pic = v.pics.get(k).copied().or_else(|| num(pr, "Picture").and_then(|n| self.pic(form, n)).map(|(t, s)| (t, Rect::from_min_size(Pos2::ZERO, s))));
                if let Some((tex, uv)) = pic {
                    let size = if uv.max.x > 1.5 { uv.size() } else { r.size() - vec2(2.0 * b, 2.0 * b) };
                    let uv = if uv.max.x > 1.5 { Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)) } else { uv };
                    cv.p.image(tex, cv.r(Rect::from_min_size(r.min + vec2(b, b), size)), uv, Color32::WHITE);
                }
                let inner = r.shrink(b);
                paint_here(cv, inner, v);
                Some(inner)
            }
            "Frame" | "ssframe" => {
                cv.fill(x, y, w, h, back_of(FACE));
                let top = y + 6.0;
                cv.etched(x, top, w, h - 6.0);
                if !caption.is_empty() {
                    let tw = self.width(&ft, &strip_amp(&caption)) + 4.0;
                    let lr = Rect::from_min_size(pos2(x + 8.0, y - 1.0), vec2(tw, 14.0));
                    cv.fill(lr.left(), lr.top(), lr.width(), lr.height(), back_of(FACE));
                    self.label_text(cv, lr.translate(vec2(2.0, 0.0)), &ft, &caption, ink, 0, false, true);
                }
                Some(r)
            }
            "videosoftelastic" | "sspanel" => {
                cv.fill(x, y, w, h, back_of(FACE));
                let mut rr = r;
                let ow = num(pr, if c.kind == "sspanel" { "BevelWidth" } else { "BevelOuterWidth" }).unwrap_or(1).max(1) as f32;
                let gap = num(pr, "BorderWidth").unwrap_or(if c.kind == "sspanel" { 3 } else { 2 }) as f32;
                let iw = num(pr, if c.kind == "sspanel" { "BevelWidth" } else { "BevelInnerWidth" }).unwrap_or(1).max(1) as f32;
                let outer = num(pr, "BevelOuter").unwrap_or(if c.kind == "sspanel" { 2 } else { 1 });
                let inner = num(pr, "BevelInner").unwrap_or(0);
                for (style, width, after) in [(outer, ow, gap), (inner, iw, 0.0)] {
                    if style != 0 {
                        cv.bevel(rr, style, width, c.kind == "sspanel");
                        rr = rr.shrink(width);
                    }
                    rr = rr.shrink(if style != 0 { after } else { 0.0 });
                }
                if let Some(pct) = num(pr, "FloodPercent").filter(|_| num(pr, "FloodType").unwrap_or(0) != 0) {
                    let fw = rr.width() * pct as f32 / 100.0;
                    cv.fill(rr.left(), rr.top(), fw, rr.height(), num(pr, "FloodColor").map_or(Color32::BLUE, vb));
                }
                // CaptionPos: left, centre or right (by threes), each top, middle or bottom.
                if !caption.is_empty() {
                    let pos = num(pr, "CaptionPos").unwrap_or(if c.kind == "sspanel" { 4 } else { 1 });
                    let a = [0, 2, 1][(pos / 3).clamp(0, 2) as usize];
                    let lh = self.line_height(&ft);
                    let tr = match pos % 3 {
                        0 => Rect::from_min_size(rr.min, vec2(rr.width(), lh)),
                        2 => Rect::from_min_max(pos2(rr.left(), rr.bottom() - lh), rr.max),
                        _ => rr,
                    };
                    self.label_text(cv, tr, &ft, &caption, ink, a, true, true);
                }
                paint_here(cv, r, v);
                Some(r)
            }
            "videosoftindextab" => {
                // TabsPerPage equal tabs a row; the current tab's row comes to the front, and
                // only the current tab's caption is bold.
                let tabs: Vec<String> = caption.split('|').map(str::to_string).collect();
                let st = self.state(form, k).tab.min(tabs.len().saturating_sub(1));
                let per = (num(pr, "TabsPerPage").unwrap_or(tabs.len() as i64).max(1) as usize).min(tabs.len().max(1));
                let rows = tabs.len().div_ceil(per);
                let (th, tw) = (17.0, ((w - 4.0) / per as f32).floor());
                let front = st / per;
                cv.fill(x, y, w, h, FACE);
                cv.edge(x, y + th * rows as f32, w, h - th * rows as f32, true);
                let mut hit = None;
                let order: Vec<usize> = (0..rows).filter(|&r| r != front).chain([front]).collect();
                for (slot, &row) in order.iter().enumerate() {
                    for (col, t) in tabs.iter().enumerate().skip(row * per).take(per).map(|(n, t)| (n - row * per, t)) {
                        let n = row * per + col;
                        let cur = n == st;
                        let tr = Rect::from_min_size(pos2(x + 2.0 + col as f32 * tw - if cur { 2.0 } else { 0.0 }, y + slot as f32 * th + if cur { 0.0 } else { 2.0 }), vec2(tw + if cur { 4.0 } else { 0.0 }, th + if cur { 1.0 } else { -2.0 }));
                        cv.fill(tr.left(), tr.top(), tr.width(), tr.height(), FACE);
                        cv.line(pos2(tr.left(), tr.bottom()), pos2(tr.left(), tr.top() + 2.0), Color32::WHITE);
                        cv.line(pos2(tr.left() + 2.0, tr.top()), pos2(tr.right() - 2.0, tr.top()), Color32::WHITE);
                        cv.line(pos2(tr.right() - 1.0, tr.top() + 2.0), pos2(tr.right() - 1.0, tr.bottom()), Color32::BLACK);
                        cv.line(pos2(tr.right() - 2.0, tr.top() + 2.0), pos2(tr.right() - 2.0, tr.bottom()), SHADOW);
                        self.label_text(cv, tr, &Ft { bold: cur, ..ft.clone() }, t, Color32::BLACK, 2, true, false);
                        if ui.interact(cv.r(tr), id.with(n), Sense::click()).clicked() {
                            hit = Some(n);
                        }
                    }
                }
                if let Some(n) = hit {
                    self.state(form, k).tab = n;
                    ev.changed = Some(k.to_string());
                }
                None
            }
            "TextBox" => {
                cv.fill(x, y, w, h, back_of(Color32::WHITE));
                cv.sunken(x, y, w, h);
                let focused = self.focus.as_deref() == Some(&format!("{form}.{k}"));
                if clicked {
                    self.focus = Some(format!("{form}.{k}"));
                }
                let st = self.state(form, k);
                if v.text.contains_key(k) {
                    st.text = caption.clone();
                }
                if focused {
                    ui.input(|i| {
                        for e in &i.events {
                            match e {
                                egui::Event::Text(t) => st.text.push_str(t),
                                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                                    st.text.pop();
                                }
                                _ => {}
                            }
                        }
                    });
                }
                let shown = st.text.clone() + if focused && (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0 { "|" } else { "" };
                self.label_text(cv, Rect::from_min_size(pos2(x + 4.0, y), vec2(w - 6.0, h)), &ft, &shown, ink, 0, true, false);
                if focused {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                }
                None
            }
            "ListBox" | "ComboBox" if c.kind == "ListBox" => {
                cv.fill(x, y, w, h, back_of(Color32::WHITE));
                cv.sunken(x, y, w, h);
                let items = v.items.get(k).cloned().unwrap_or_default();
                let lh = self.line_height(&ft);
                let rows = ((h - 4.0) / lh).floor().max(1.0) as usize;
                let scroll = items.len() > rows;
                let iw = w - 4.0 - if scroll { 16.0 } else { 0.0 };
                let st = self.state(form, k).clone();
                let top = st.top.min(items.len().saturating_sub(rows));
                let mut sel = st.sel;
                for (n, it) in items.iter().enumerate().skip(top).take(rows) {
                    let ir = Rect::from_min_size(pos2(x + 2.0, y + 2.0 + (n - top) as f32 * lh), vec2(iw, lh));
                    let picked = sel == Some(n);
                    if picked {
                        cv.fill(ir.left(), ir.top(), ir.width(), ir.height(), NAVY);
                    }
                    self.label_text(cv, ir.translate(vec2(2.0, 0.0)), &ft, it, if picked { Color32::WHITE } else { ink }, 0, true, false);
                    let hit = ui.interact(cv.r(ir), id.with(n), Sense::click());
                    if hit.clicked() && enabled {
                        sel = Some(n);
                        ev.changed = Some(k.to_string());
                    }
                    if hit.double_clicked() && enabled {
                        ev.double = Some(k.to_string());
                    }
                }
                let mut top = top;
                if scroll {
                    top = self.scrollbar(ui, cv, id, Rect::from_min_size(pos2(x + w - 18.0, y + 2.0), vec2(16.0, h - 4.0)), top, items.len() - rows, rows);
                }
                if resp.hovered() {
                    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
                    if wheel != 0.0 {
                        top = (top as f32 - wheel.signum()).clamp(0.0, items.len().saturating_sub(rows) as f32) as usize;
                    }
                }
                let st = self.state(form, k);
                st.sel = sel;
                st.top = top;
                None
            }
            "ComboBox" => {
                cv.fill(x, y, w, h, Color32::WHITE);
                cv.sunken(x, y, w, h);
                let items = v.items.get(k).cloned().unwrap_or_default();
                let sel = self.state(form, k).sel;
                let shown = sel.and_then(|s| items.get(s)).cloned().unwrap_or(caption.clone());
                self.label_text(cv, Rect::from_min_size(pos2(x + 4.0, y), vec2(w - 22.0, h)), &ft, &shown, ink, 0, true, false);
                let br = Rect::from_min_size(pos2(x + w - 18.0, y + 2.0), vec2(16.0, h - 4.0));
                cv.fill(br.left(), br.top(), br.width(), br.height(), FACE);
                cv.button(br.left(), br.top(), br.width(), br.height(), down);
                cv.arrow(br.center(), 2);
                let me = format!("{form}.{k}");
                if clicked {
                    self.combo = if self.combo.as_deref() == Some(&me) { None } else { Some(me.clone()) };
                }
                if self.combo.as_deref() == Some(&me) {
                    let lh = self.line_height(&ft);
                    let list = Rect::from_min_size(pos2(x, y + h), vec2(w, lh * items.len().min(8) as f32 + 2.0));
                    let screen = cv.r(list);
                    let s = self.s;
                    let mut picked = None;
                    egui::Area::new(id.with("drop")).order(egui::Order::Tooltip).fixed_pos(screen.min).show(ui.ctx(), |ui| {
                        let (rr, _) = ui.allocate_exact_size(screen.size(), Sense::hover());
                        let cv = Cv { p: ui.painter().clone(), o: rr.min - list.min.to_vec2() * s, s };
                        cv.fill(list.left(), list.top(), list.width(), list.height(), Color32::WHITE);
                        cv.frame(list.left(), list.top(), list.width(), list.height(), Color32::BLACK);
                        for (n, it) in items.iter().enumerate().take(8) {
                            let ir = Rect::from_min_size(pos2(list.left() + 1.0, list.top() + 1.0 + n as f32 * lh), vec2(list.width() - 2.0, lh));
                            let hit = ui.interact(cv.r(ir), id.with(("it", n)), Sense::click());
                            if hit.hovered() {
                                cv.fill(ir.left(), ir.top(), ir.width(), ir.height(), NAVY);
                            }
                            self.label_text(&cv, ir.translate(vec2(2.0, 0.0)), &ft, it, if hit.hovered() { Color32::WHITE } else { Color32::BLACK }, 0, true, false);
                            if hit.clicked() {
                                picked = Some(n);
                            }
                        }
                    });
                    if let Some(n) = picked {
                        self.state(form, k).sel = Some(n);
                        self.combo = None;
                        ev.changed = Some(k.to_string());
                    }
                }
                None
            }
            "CheckBox" | "OptionButton" | "sscheck" | "ssoption" => {
                if c.kind == "CheckBox" || c.kind == "OptionButton" {
                    cv.fill(x, y, w, h, back_of(FACE));
                }
                if clicked {
                    let st = self.state(form, k);
                    st.on = !st.on || c.kind.contains("ption");
                }
                let on = self.state(form, k).on;
                let right = align == 1;
                let bx = if right { x + w - 13.0 } else { x };
                let by = (y + (h - 13.0) / 2.0).floor();
                if c.kind.contains("ption") {
                    cv.p.circle_filled(cv.pt(pos2(bx + 6.0, by + 6.0)), 5.5 * cv.s, Color32::WHITE);
                    cv.p.circle_stroke(cv.pt(pos2(bx + 6.0, by + 6.0)), 5.5 * cv.s, Stroke::new(cv.s, SHADOW));
                    if on {
                        cv.p.circle_filled(cv.pt(pos2(bx + 6.0, by + 6.0)), 2.0 * cv.s, Color32::BLACK);
                    }
                } else {
                    cv.fill(bx, by, 13.0, 13.0, Color32::WHITE);
                    cv.sunken(bx, by, 13.0, 13.0);
                    if on {
                        for d in 0..3 {
                            let d = d as f32;
                            cv.line(pos2(bx + 3.0, by + 5.0 + d), pos2(bx + 5.0, by + 7.0 + d), Color32::BLACK);
                            cv.line(pos2(bx + 5.0, by + 7.0 + d), pos2(bx + 10.0, by + 2.0 + d), Color32::BLACK);
                        }
                    }
                }
                let tr = if right { Rect::from_min_size(r.min, vec2(w - 17.0, h)) } else { Rect::from_min_size(pos2(x + 17.0, y), vec2(w - 17.0, h)) };
                self.label_text(cv, tr, &ft, &caption, ink, if right { 1 } else { 0 }, true, true);
                if clicked {
                    ev.changed = Some(k.to_string());
                }
                None
            }
            "VScrollBar" | "HScrollBar" => {
                let st = self.state(form, k).clone();
                let max = num(pr, "Max").unwrap_or(32767).max(1) as usize;
                let top = self.scrollbar(ui, cv, id, r, st.top, max, num(pr, "LargeChange").unwrap_or(1).max(1) as usize);
                if top != st.top {
                    self.state(form, k).top = top;
                    ev.changed = Some(k.to_string());
                }
                None
            }
            "Shape" => {
                let col = num(pr, "BorderColor").map_or(Color32::BLACK, vb);
                if num(pr, "BackStyle").unwrap_or(0) == 1 {
                    cv.fill(x, y, w, h, back_of(Color32::WHITE));
                }
                cv.frame(x, y, w, h, col);
                None
            }
            "Line" => {
                let p = |a: &str| num(pr, a).unwrap_or(0) as f32 / 15.0;
                cv.line(pos2(p("X1"), p("Y1")) + r.min.to_vec2(), pos2(p("X2"), p("Y2")) + r.min.to_vec2(), num(pr, "BorderColor").map_or(Color32::BLACK, vb));
                None
            }
            "mmcontrol" => {
                let bw = w / 9.0;
                for n in 0..9 {
                    let bx = x + n as f32 * bw;
                    cv.fill(bx, y, bw, h, FACE);
                    cv.button(bx, y, bw, h, false);
                }
                None
            }
            _ => {
                paint_here(cv, r, v);
                Some(r)
            }
        }
    }

    /// A Windows 95 scroll bar; returns the new position (0..=max) after clicks.
    #[allow(clippy::too_many_arguments)]
    fn scrollbar(&mut self, ui: &mut egui::Ui, cv: &Cv, id: egui::Id, r: Rect, pos: usize, max: usize, page: usize) -> usize {
        let vertical = r.height() > r.width();
        let (len, thick) = if vertical { (r.height(), r.width()) } else { (r.width(), r.height()) };
        let along = |a: f32, l: f32| if vertical { Rect::from_min_size(pos2(r.left(), r.top() + a), vec2(thick, l)) } else { Rect::from_min_size(pos2(r.left() + a, r.top()), vec2(l, thick)) };
        cv.fill(r.left(), r.top(), r.width(), r.height(), Color32::from_gray(224));
        let mut pos = pos.min(max);
        for (n, (a, dir)) in [(0.0, -1i64), (len - thick, 1)].into_iter().enumerate() {
            let br = along(a, thick);
            let hit = ui.interact(cv.r(br), id.with(("arrow", n)), Sense::click());
            cv.fill(br.left(), br.top(), br.width(), br.height(), FACE);
            cv.button(br.left(), br.top(), br.width(), br.height(), hit.is_pointer_button_down_on());
            cv.arrow(br.center(), match (vertical, dir) { (true, -1) => 0, (true, _) => 2, (false, -1) => 3, _ => 1 });
            if hit.clicked() {
                pos = (pos as i64 + dir).clamp(0, max as i64) as usize;
            }
        }
        let track = len - 2.0 * thick;
        if max > 0 && track > 8.0 {
            let thumb = (track * page as f32 / (max + page) as f32).max(8.0);
            let at = thick + (track - thumb) * pos as f32 / max as f32;
            let tr = along(at, thumb);
            cv.fill(tr.left(), tr.top(), tr.width(), tr.height(), FACE);
            cv.button(tr.left(), tr.top(), tr.width(), tr.height(), false);
            let drag = ui.interact(cv.r(tr), id.with("thumb"), Sense::drag());
            if drag.dragged() {
                let d = if vertical { drag.drag_delta().y } else { drag.drag_delta().x } / cv.s;
                let st = self.st.entry(format!("drag{id:?}")).or_default();
                st.text = format!("{}", st.text.parse::<f32>().unwrap_or(0.0) + d);
                let moved = st.text.parse::<f32>().unwrap_or(0.0);
                let step = (track - thumb) / max as f32;
                if moved.abs() >= step {
                    pos = (pos as f32 + (moved / step).trunc()).clamp(0.0, max as f32) as usize;
                    st.text = format!("{}", moved % step);
                }
            }
        }
        pos
    }

    fn fon(&self, ft: &Ft) -> Option<&Fnt> {
        if !ft.name.eq_ignore_ascii_case("MS Sans Serif") {
            return None;
        }
        let pt = (ft.size.round() as u16).max(8);
        self.fon.iter().min_by_key(|(p, _)| p.abs_diff(pt)).map(|(_, f)| f)
    }

    fn ttf(&self, ft: &Ft) -> FontId {
        let fam = match ft.name.to_ascii_lowercase().as_str() {
            n if n.starts_with("courier") => "cour",
            n if n.starts_with("times") => "times",
            n if n.starts_with("arial") => "arial",
            "symbol" => "symbol",
            _ => "ms",
        };
        FontId::new((ft.size * 96.0 / 72.0 * self.s).max(1.0), egui::FontFamily::Name(format!("{fam}{}", u8::from(ft.bold)).into()))
    }

    pub fn line_height(&self, ft: &Ft) -> f32 {
        self.fon(ft).map_or((ft.size * 96.0 / 72.0 * 1.2).round(), |f| f.height)
    }

    /// A string's width in virtual pixels.
    pub fn width(&self, ft: &Ft, s: &str) -> f32 {
        match self.fon(ft) {
            Some(f) => s.chars().map(|ch| glyph(f, ch).1 + if ft.bold { 1.0 } else { 0.0 }).sum(),
            None => self.ctx.fonts_mut(|fo| fo.layout_no_wrap(s.to_string(), self.ttf(ft), Color32::WHITE).size().x) / self.s,
        }
    }

    /// Draws a caption in `r`: aligned left (0), right (1) or centred (2), top or
    /// vertically centred, word-wrapped, with `&` marking the underlined access key.
    #[allow(clippy::too_many_arguments)]
    pub fn label_text(&self, cv: &Cv, r: Rect, ft: &Ft, s: &str, col: Color32, align: u8, middle: bool, amp: bool) {
        let s = if ft.name.eq_ignore_ascii_case("Symbol") { symbol(s) } else { s.to_string() };
        let (plain, under) = if amp { accel(&s) } else { (s.clone(), None) };
        let lh = self.line_height(ft);
        // Word wrap at the control's width.
        let mut lines: Vec<(String, usize)> = Vec::new();
        let mut start = 0;
        for para in plain.split('\n') {
            let mut line = String::new();
            let mut line_start = start;
            for word in para.split_inclusive(' ') {
                if !line.is_empty() && self.width(ft, (line.clone() + word).trim_end()) > r.width() {
                    lines.push((std::mem::take(&mut line), line_start));
                    line_start = start;
                }
                line.push_str(word);
                start += word.chars().count();
            }
            lines.push((line, line_start));
            start += 1;
        }
        let total = lines.len() as f32 * lh;
        let mut y = if middle { r.top() + ((r.height() - total) / 2.0).floor() } else { r.top() };
        let clip = cv.p.with_clip_rect(cv.r(r.expand(1.0)).intersect(cv.p.clip_rect()));
        for (line, at) in lines {
            let line = line.trim_end_matches(' ');
            let lw = self.width(ft, line);
            let x = match align {
                1 => r.right() - lw,
                2 => r.left() + ((r.width() - lw) / 2.0).floor(),
                _ => r.left(),
            };
            let mut cx = x;
            for (n, ch) in line.chars().enumerate() {
                let cw = match self.fon(ft) {
                    Some(f) => {
                        let (gx, gw) = glyph(f, ch);
                        let uv = Rect::from_min_max(pos2(gx / f.atlas_w, 0.0), pos2((gx + gw) / f.atlas_w, 1.0));
                        for b in 0..=u8::from(ft.bold) {
                            let gr = cv.r(Rect::from_min_size(pos2(cx + f32::from(b), y), vec2(gw, f.height)));
                            let mut m = egui::Mesh::with_texture(f.tex.id());
                            m.add_rect_with_uv(gr, uv, col);
                            if ft.italic {
                                // Top edge leans right by a quarter of the height.
                                let lean = gr.height() / 4.0;
                                for v in &mut m.vertices {
                                    v.pos.x += lean * (gr.bottom() - v.pos.y) / gr.height();
                                }
                            }
                            clip.add(m);
                        }
                        gw + f32::from(u8::from(ft.bold))
                    }
                    None => {
                        let g = clip.layout_no_wrap(ch.to_string(), self.ttf(ft), col);
                        let w = g.size().x / cv.s;
                        clip.galley(cv.pt(pos2(cx, y + (lh - g.size().y / cv.s) / 2.0)), g, col);
                        w
                    }
                };
                if under == Some(at + n) {
                    cv.fill(cx, y + lh - 2.0, cw, 1.0, col);
                }
                cx += cw;
            }
            y += lh;
        }
    }
}

fn glyph(f: &Fnt, ch: char) -> (f32, f32) {
    let c = ddj_extract::hlp::CP1252.iter().find(|p| p.1 == ch).map_or(u32::from(ch), |p| u32::from(p.0));
    let i = if (u32::from(f.first)..=255).contains(&c) { (c - u32::from(f.first)) as usize } else { usize::from(b'?' - f.first) };
    f.glyphs.get(i).copied().unwrap_or((0.0, 0.0))
}

/// The Symbol font's arrows as Unicode.
fn symbol(s: &str) -> String {
    s.chars()
        .map(|c| match u32::from(c) {
            0xAC => '←',
            0xAD => '↑',
            0xAE => '→',
            0xAF => '↓',
            _ => c,
        })
        .collect()
}

fn strip_amp(s: &str) -> String {
    accel(s).0
}

/// A caption without its `&` markers, and the position of the access key.
fn accel(s: &str) -> (String, Option<usize>) {
    let (mut out, mut under, mut it) = (String::new(), None, s.chars().peekable());
    while let Some(c) = it.next() {
        if c == '&' {
            if it.peek() == Some(&'&') {
                it.next();
                out.push('&');
            } else if under.is_none() {
                under = Some(out.chars().count());
            }
            continue;
        }
        out.push(c);
    }
    (out, under)
}

pub fn num(p: &BTreeMap<String, Val>, k: &str) -> Option<i64> {
    match p.get(k)? {
        Val::N(n) => Some(*n),
        Val::F(f) => Some(*f as i64),
        Val::S(_) => None,
    }
}

pub fn text(p: &BTreeMap<String, Val>, k: &str) -> String {
    match p.get(k) {
        Some(Val::S(s)) => s.clone(),
        _ => String::new(),
    }
}

/// A control's font; VB's own controls default to bold, the VBXs' to regular.
fn font(p: &BTreeMap<String, Val>, kind: &str) -> Ft {
    let size = match p.get("FontSize") {
        Some(Val::F(f)) => *f as f32,
        _ => 8.25,
    };
    let flags = num(p, "FontFlags");
    // VB3's default font is bold; a stored font is bold when bit 0 or 7 is set (bit 1: italic).
    Ft { name: Some(text(p, "FontName")).filter(|n| !n.is_empty()).unwrap_or_else(|| "MS Sans Serif".into()), size, bold: flags.map_or(kind.starts_with(char::is_uppercase), |f| f & 0x81 != 0), italic: flags.is_some_and(|f| f & 2 != 0) }
}

/// Drawing in virtual pixels.
pub struct Cv {
    pub p: Painter,
    /// Screen position of the virtual origin.
    pub o: Pos2,
    pub s: f32,
}

impl Cv {
    pub fn pt(&self, p: Pos2) -> Pos2 {
        self.o + p.to_vec2() * self.s
    }
    pub fn r(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.pt(r.min), self.pt(r.max))
    }
    pub fn fill(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.p.rect_filled(self.r(Rect::from_min_size(pos2(x, y), vec2(w, h))), 0.0, c);
    }
    pub fn line(&self, a: Pos2, b: Pos2, c: Color32) {
        if a.x == b.x || a.y == b.y {
            let r = Rect::from_two_pos(a, b);
            self.fill(r.left(), r.top(), r.width().max(1.0), r.height().max(1.0), c);
        } else {
            self.p.line_segment([self.pt(a), self.pt(b)], Stroke::new(self.s, c));
        }
    }
    /// One-pixel lines: top and left in `tl`, bottom and right in `br`.
    pub fn box1(&self, x: f32, y: f32, w: f32, h: f32, tl: Color32, br: Color32) {
        self.fill(x, y, w, 1.0, tl);
        self.fill(x, y, 1.0, h, tl);
        self.fill(x, y + h - 1.0, w, 1.0, br);
        self.fill(x + w - 1.0, y, 1.0, h, br);
    }
    pub fn frame(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.box1(x, y, w, h, c, c);
    }
    /// The raised 3D edge of windows and buttons (or sunken).
    pub fn edge(&self, x: f32, y: f32, w: f32, h: f32, raised: bool) {
        if raised {
            self.box1(x, y, w, h, FACE, Color32::BLACK);
            self.box1(x + 1.0, y + 1.0, w - 2.0, h - 2.0, Color32::WHITE, SHADOW);
        } else {
            self.sunken(x, y, w, h);
        }
    }
    pub fn sunken(&self, x: f32, y: f32, w: f32, h: f32) {
        self.box1(x, y, w, h, SHADOW, Color32::WHITE);
        self.box1(x + 1.0, y + 1.0, w - 2.0, h - 2.0, Color32::BLACK, FACE);
    }
    pub fn button(&self, x: f32, y: f32, w: f32, h: f32, down: bool) {
        if down {
            self.box1(x, y, w, h, Color32::BLACK, Color32::WHITE);
            self.box1(x + 1.0, y + 1.0, w - 2.0, h - 2.0, SHADOW, FACE);
        } else {
            self.edge(x, y, w, h, true);
        }
    }
    pub fn etched(&self, x: f32, y: f32, w: f32, h: f32) {
        self.box1(x, y, w - 1.0, h - 1.0, SHADOW, SHADOW);
        self.box1(x + 1.0, y + 1.0, w - 1.0, h - 1.0, Color32::WHITE, Color32::WHITE);
    }
    /// The VBX panels' bevels: 1 raised, 2 inset (SSPanel: 1 inset, 2 raised), 3 a
    /// groove, 4 a ridge.
    fn bevel(&self, r: Rect, style: i64, width: f32, ss: bool) {
        let style = if ss { [0, 2, 1][style.clamp(0, 2) as usize] } else { style };
        for k in 0..width as i32 {
            let rr = r.shrink(k as f32);
            let (x, y, w, h) = (rr.left(), rr.top(), rr.width(), rr.height());
            match style {
                1 => self.box1(x, y, w, h, Color32::WHITE, SHADOW),
                2 => self.box1(x, y, w, h, SHADOW, Color32::WHITE),
                3 => self.etched(x, y, w, h),
                _ => {
                    self.box1(x, y, w - 1.0, h - 1.0, Color32::WHITE, Color32::WHITE);
                    self.box1(x + 1.0, y + 1.0, w - 1.0, h - 1.0, SHADOW, SHADOW);
                }
            }
        }
    }
    /// A small black arrow: 0 up, 1 right, 2 down, 3 left.
    pub fn arrow(&self, c: Pos2, dir: u8) {
        let c = c.floor();
        for k in 0..4 {
            let k = k as f32;
            let (a, b) = match dir {
                0 => (pos2(c.x - 3.0 + k, c.y - 2.0 + k), pos2(c.x + 4.0 - k, c.y - 2.0 + k)),
                2 => (pos2(c.x - 3.0 + k, c.y - 1.0 + k), pos2(c.x + 4.0 - k, c.y - 1.0 + k)),
                1 => (pos2(c.x - 1.0 + k, c.y - 3.0 + k), pos2(c.x - 1.0 + k, c.y + 4.0 - k)),
                _ => (pos2(c.x + 1.0 - k, c.y - 3.0 + k), pos2(c.x + 1.0 - k, c.y + 4.0 - k)),
            };
            if a.y == b.y {
                self.fill(a.x, a.y, b.x - a.x, 1.0, Color32::BLACK);
            } else {
                self.fill(a.x, a.y, 1.0, b.y - a.y, Color32::BLACK);
            }
        }
    }
}
