//! The original game's pictures as textures, the 1995 look, and the textured dungeon view.

use std::path::Path;

use ddj_extract::vfs as fs;

use ddj_rules::{
    game::{self, Game, Side},
    mdr::{Floor, GameData},
};
use eframe::egui::{self, Color32, ColorImage, Mesh, Pos2, Rect, Stroke, TextureHandle, TextureOptions, epaint::Vertex, pos2, vec2};

type Rgba = (usize, usize, Vec<u8>);

pub struct Art {
    /// Monster portraits: 60x60 tiles, ten to a row.
    pub mons: TextureHandle,
    wall: TextureHandle,
    backdrop: TextureHandle,
    door: TextureHandle,
    /// The monster box's 45x45 icons: hostile, peaceful, box, chest, Asmodeus's chest,
    /// Asmodeus hostile.
    misc: TextureHandle,
    /// The dungeon view's fog, the water and quicksand strips, and Asmodeus's face.
    fog: TextureHandle,
    water: TextureHandle,
    sand: TextureHandle,
    asmo: TextureHandle,
    /// The AutoMap's 15x15 symbols, one per square flag bit, then the four facing
    /// cursors; black is clear, as the original ORs them onto the map.
    pics: TextureHandle,
}

/// Asmodeus's monster id: his own face, icon and chest.
const ASMODEUS: i16 = 277;

fn crop((w, _, px): &Rgba, x: usize, y: usize, cw: usize, ch: usize) -> ColorImage {
    let mut out = Vec::with_capacity(cw * ch * 4);
    for row in y..y + ch {
        out.extend_from_slice(&px[(row * w + x) * 4..(row * w + x + cw) * 4]);
    }
    ColorImage::from_rgba_unmultiplied([cw, ch], &out)
}

impl Art {
    pub fn load(ctx: &egui::Context, dir: &Path) -> Option<Self> {
        let dll = fs::read(dir.join("MG11.DLL")).ok()?;
        let mg = |n| ddj_extract::mg_bitmap(&dll, n);
        let (mons, maze, door) = (mg("MONS")?, mg("MAZE")?, mg("DOOR")?);
        let (water, sand) = (mg("WATR")?, mg("QSND")?);
        // Only the tiled surfaces repeat; pictures are sampled once.
        let tex = |name: &str, img: ColorImage| ctx.load_texture(name, img, TextureOptions::NEAREST_REPEAT);
        let whole = |name: &str, (w, h, px): Rgba| ctx.load_texture(name, ColorImage::from_rgba_unmultiplied([w, h], &px), TextureOptions::NEAREST);
        Some(Self {
            // The big brick face and the ceiling-to-floor backdrop from the maze sheet.
            wall: tex("wall", crop(&maze, 752, 16, 78, 68)),
            backdrop: tex("backdrop", crop(&maze, 2, 0, 28, 100)),
            door: tex("door", crop(&door, 11, 2, 44, 50)),
            mons: whole("mons", mons),
            misc: whole("misc", mg("MISC")?),
            fog: tex("fog", crop(&mg("FOG")?, 0, 0, 50, 50)),
            // The original copies the strips' top 18 rows.
            water: whole("water", (50, 18, water.2[..50 * 18 * 4].to_vec())),
            sand: whole("sand", (50, 18, sand.2[..50 * 18 * 4].to_vec())),
            asmo: whole("asmo", mg("ASMO")?),
            pics: whole("pics", mg("PICS").map(|(w, h, mut px)| {
                px.chunks_mut(4).filter(|c| c[..3] == [0, 0, 0]).for_each(|c| c[3] = 0);
                (w, h, px)
            })?),
        })
    }

    /// A monster's face as the Library's picMonster draws it: Asmodeus's own (fn0C3B),
    /// else its portrait.
    pub fn face(&self, m: &ddj_rules::mdr::Monster) -> (egui::TextureId, Rect) {
        if m.id == ASMODEUS { (self.asmo.id(), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))) } else { self.portrait(m.picture) }
    }

    /// A monster's portrait as (texture, uv rect).
    pub fn portrait(&self, picture: i16) -> (egui::TextureId, Rect) {
        let tile = f32::from(picture.max(1) - 1);
        (self.mons.id(), Rect::from_min_size(pos2((tile % 10.0) * 0.1, (tile / 10.0).floor() * 60.0 / 540.0), vec2(0.1, 60.0 / 540.0)))
    }
}

/// The original's gray Windows 3.1 look: gray panels, black text, navy selection.
pub fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    let gray = Color32::from_gray(192);
    v.panel_fill = gray;
    v.window_fill = gray;
    v.faint_bg_color = Color32::from_gray(200);
    v.extreme_bg_color = Color32::from_rgb(255, 255, 225);
    v.selection.bg_fill = Color32::from_rgb(0, 0, 128);
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::BLACK);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.fg_stroke = Stroke::new(1.0, Color32::BLACK);
        w.bg_fill = Color32::from_gray(212);
        w.weak_bg_fill = Color32::from_gray(212);
        w.bg_stroke = Stroke::new(1.0, Color32::from_gray(64));
        w.corner_radius = 0.into();
    }
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(228);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(128));
    ctx.set_visuals(v);
}

/// A textured quad (corners clockwise from top-left), cut into vertical strips so
/// the perspective trapezoids don't warp diagonally. `reps` repeats the texture.
fn quad(paint: &egui::Painter, tex: &TextureHandle, c: [Pos2; 4], reps: f32, tint: Color32) {
    const STRIPS: usize = 8;
    let mut m = Mesh::with_texture(tex.id());
    for i in 0..=STRIPS {
        let t = i as f32 / STRIPS as f32;
        let (top, bottom) = (c[0].lerp(c[1], t), c[3].lerp(c[2], t));
        m.vertices.push(Vertex { pos: top, uv: pos2(t * reps, 0.0), color: tint });
        m.vertices.push(Vertex { pos: bottom, uv: pos2(t * reps, 1.0), color: tint });
        if i > 0 {
            let b = (2 * i) as u32;
            m.add_triangle(b - 2, b - 1, b);
            m.add_triangle(b - 1, b, b + 1);
        }
    }
    paint.add(m);
}

fn side(f: &Floor, (x, y): (usize, usize), dir: u8) -> Side {
    game::side(f, x, y, dir)
}

/// The first-person view into `rect`: up to four squares ahead, nearer walls brighter.
pub fn dungeon_view(paint: &egui::Painter, rect: Rect, art: Option<&Art>, d: &GameData, g: &Game) {
    let p = g.pos();
    let f = &d.floors[p.floor - 1];
    let Some(art) = art else {
        paint.rect_filled(rect, 0.0, Color32::BLACK);
        return;
    };
    let flags = f.square(p.x, p.y).flags;
    let full = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    // As mdrmazShowMaze: Asmodeus fills the view when his group is in front; fog hides
    // it (four tiles); water and quicksand lie across its bottom 18 hundredths.
    if g.fight.as_ref().is_some_and(|fi| fi.groups_from(0).first().and_then(|&gr| fi.front(gr)).is_some_and(|x| d.monsters[fi.foes[x].monster].id == ASMODEUS)) {
        paint.image(art.asmo.id(), rect, full, Color32::WHITE);
        return;
    }
    if flags & ddj_rules::mdr::square::FOG != 0 {
        let mut m = Mesh::with_texture(art.fog.id());
        m.add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, pos2(2.0, 2.0)), Color32::WHITE);
        paint.add(m);
        return;
    }
    view(paint, rect, art, f, p);
    for (bit, tex) in [(ddj_rules::mdr::square::WATER, &art.water), (ddj_rules::mdr::square::QUICKSAND, &art.sand)] {
        if flags & bit != 0 {
            let strip = Rect::from_min_max(pos2(rect.left(), rect.bottom() - rect.height() * 0.18), rect.max);
            let mut m = Mesh::with_texture(tex.id());
            m.add_rect_with_uv(strip, Rect::from_min_max(Pos2::ZERO, pos2(2.0, 1.0)), Color32::WHITE);
            paint.add(m);
        }
    }
}

/// The walls and doors ahead.
fn view(paint: &egui::Painter, rect: Rect, art: &Art, f: &Floor, p: game::Pos) {
    let mut m = Mesh::with_texture(art.backdrop.id());
    m.add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    paint.add(m);
    let scale = [1.0, 0.62, 0.38, 0.24, 0.15];
    let at = |k: usize| Rect::from_center_size(rect.center(), rect.size() * scale[k]);
    let mut squares = vec![(p.x, p.y)];
    while squares.len() < 4 && side(f, *squares.last().unwrap(), p.facing) == Side::Open {
        match game::ahead(f, squares.last().unwrap().0, squares.last().unwrap().1, p.facing) {
            Some(s) => squares.push(s),
            None => break,
        }
    }
    let tint = |k: usize| Color32::from_gray(255 - 45 * k.min(4) as u8);
    let face = |c: [Pos2; 4], s: Side, k: usize, reps: f32| {
        quad(paint, &art.wall, c, reps, tint(k));
        if s == Side::Door {
            // The door fills the middle of the wall, from the floor up.
            let lerp = |a: Pos2, b: Pos2, t: f32| a.lerp(b, t);
            let (tl, tr) = (lerp(c[0], c[1], 0.25), lerp(c[0], c[1], 0.75));
            let (bl, br) = (lerp(c[3], c[2], 0.25), lerp(c[3], c[2], 0.75));
            quad(paint, &art.door, [lerp(bl, tl, 0.8), lerp(br, tr, 0.8), br, bl], 1.0, tint(k));
        }
    };
    for (k, &sq) in squares.iter().enumerate().rev() {
        let (near, far) = (at(k), at(k + 1));
        let front = side(f, sq, p.facing);
        if front != Side::Open {
            face([far.left_top(), far.right_top(), far.right_bottom(), far.left_bottom()], front, k + 1, 2.0);
        }
        for (dir, x0, x1) in [((p.facing + 3) % 4, near.left(), far.left()), ((p.facing + 1) % 4, near.right(), far.right())] {
            let s = side(f, sq, dir);
            let c = if s == Side::Open {
                // A side passage: the far wall of the opening, seen face on.
                let (a, b) = (x0.min(x1), x0.max(x1));
                [pos2(a, far.top()), pos2(b, far.top()), pos2(b, far.bottom()), pos2(a, far.bottom())]
            } else if x1 > x0 {
                [pos2(x0, near.top()), pos2(x1, far.top()), pos2(x1, far.bottom()), pos2(x0, near.bottom())]
            } else {
                [pos2(x1, far.top()), pos2(x0, near.top()), pos2(x0, near.bottom()), pos2(x1, far.bottom())]
            };
            let s = if s == Side::Open { Side::Wall } else { s };
            face(c, s, k + usize::from(side(f, sq, dir) == Side::Open), 1.0);
        }
    }
}

/// The monster box: one portrait for each kind of foe still standing, left to right.
pub fn monsters(paint: &egui::Painter, rect: Rect, s: f32, art: Option<&Art>, d: &GameData, g: &Game) {
    let (Some(art), Some(fight)) = (art, &g.fight) else { return };
    // Each group's picture is the monster it passes for (MONGEN's @24).
    let mut kinds: Vec<usize> = Vec::new();
    for f in fight.foes.iter().filter(|f| f.hp > 0) {
        let m = fight.shown.get(usize::from(f.group)).map_or(f.monster, |s| s.as_m);
        if !kinds.contains(&m) {
            kinds.push(m);
        }
    }
    let size = 60.0 * s;
    for (i, &m) in kinds.iter().take(4).enumerate() {
        let (id, uv) = art.portrait(d.monsters[m].picture);
        let x = rect.left() + (rect.width() - size * kinds.len().min(4) as f32 - 6.0 * s * (kinds.len().min(4) as f32 - 1.0)) / 2.0 + i as f32 * (size + 6.0 * s);
        paint.image(id, Rect::from_min_size(pos2(x, rect.top() + 4.0 * s), vec2(size, size)), uv, Color32::WHITE);
    }
}

/// The monster box's icons, as monShowAlignment and monShowChest: hostile or peaceful
/// top left while monsters stand, the box or chest top right while there is one.
pub fn box_icons(paint: &egui::Painter, rect: Rect, s: f32, art: Option<&Art>, d: &GameData, g: &Game) {
    let Some(art) = art else { return };
    let icon = |x: f32, tile: f32| {
        paint.image(art.misc.id(), Rect::from_min_size(pos2(x, rect.top() + 8.0 * s), vec2(45.0 * s, 45.0 * s)), Rect::from_min_size(pos2(tile / 6.0, 0.0), vec2(1.0 / 6.0, 1.0)), Color32::WHITE);
    };
    let asmo = |m: usize| d.monsters[m].id == ASMODEUS;
    if let Some(f) = g.fight.as_ref()
        && let Some(x) = f.groups_from(0).first().and_then(|&gr| f.front(gr))
    {
        icon(rect.left() + 3.0 * s, if f.peaceful { 1.0 } else if asmo(f.foes[x].monster) { 5.0 } else { 0.0 });
    }
    if let Some(c) = g.fight.as_ref().and_then(|f| f.chest.as_ref()).or(g.chest.as_ref()) {
        icon(rect.right() - 51.0 * s, if asmo(c.monster) { 4.0 } else if c.chest { 3.0 } else { 2.0 });
    }
}

/// The AutoMap (frmAutoMap's medPlotMaze and medDrawCursor): the gray map, visited squares
/// black, each known flag's symbol at a 14-pixel step (walls and doors on the square's
/// east and north edges), the facing cursor, and the map's west and south edges beside
/// visited squares. Centred on the party (modAdjustMap), or on the map's middle when lost.
/// At its default size the original's window shows only this; the level bar and labels
/// come into view only when it is made bigger than the whole map.
pub fn automap(paint: &egui::Painter, r: Rect, s: f32, art: Option<&Art>, d: &GameData, g: &Game, floor: usize, blink: bool) {
    let (Some(art), pos) = (art, g.pos()) else { return };
    let Some(f) = floor.checked_sub(1).and_then(|i| d.floors.get(i)) else { return };
    let map = g.lead().map.get(floor - 1).map_or(&[][..], Vec::as_slice);
    let paint = paint.with_clip_rect(r);
    let (step, h) = (14.0 * s, f.height as f32);
    let (cx, cy) = if g.lost.any() { (f.width as f32 / 2.0 - 1.0, h / 2.0) } else { (pos.x as f32, h - 1.0 - pos.y as f32) };
    let origin = r.center() - vec2(cx * 14.0 + 3.75, cy * 14.0 + 3.75) * s;
    // A square's top left, `x` east and `y` north of the south-west corner.
    let at = |x: f32, y: f32| origin + vec2(x, h - 1.0 - y) * step;
    let tile = |n: usize, p: Pos2, inset: f32| {
        let uv = Rect::from_min_size(pos2((n as f32 * 15.0 + inset) / 405.0, inset / 15.0), vec2((15.0 - 2.0 * inset) / 405.0, (15.0 - 2.0 * inset) / 15.0));
        paint.image(art.pics.id(), Rect::from_min_size(p + vec2(inset, inset) * s, vec2(15.0 - 2.0 * inset, 15.0 - 2.0 * inset) * s), uv, Color32::WHITE);
    };
    let known = |x: usize, y: usize| map.get(y * f.width + x).copied().unwrap_or(0);
    for y in 0..f.height {
        for x in 0..f.width {
            if known(x, y) & ddj_rules::automap::VISITED != 0 {
                paint.rect_filled(Rect::from_min_size(at(x as f32, y as f32) - vec2(s, s), vec2(16.0 * s, 16.0 * s)), 0.0, Color32::BLACK);
            }
        }
    }
    for y in 0..f.height {
        for x in 0..f.width {
            let (v, p) = (known(x, y), at(x as f32, y as f32));
            for n in (0..23).filter(|n| v & (1 << n) != 0) {
                tile(n, p + if n < 6 { if n % 2 == 0 { vec2(7.0 * s, 0.0) } else { vec2(0.0, -7.0 * s) } } else { vec2(0.0, 0.0) }, 0.0);
            }
            if v & ddj_rules::automap::VISITED != 0 && x == 0 {
                tile(0, at(-1.0, y as f32) + vec2(7.0 * s, 0.0), 0.0);
            }
            if v & ddj_rules::automap::VISITED != 0 && y == 0 {
                tile(1, at(x as f32, -1.0) - vec2(0.0, 7.0 * s), 0.0);
            }
        }
    }
    // The cursor is copied, not ORed, and blinks every half second (timBlinker).
    if !g.lost.any() && floor == pos.floor {
        let ctx = paint.ctx();
        if blink {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        if !blink || ctx.input(|i| i.time * 2.0) as u64 % 2 == 0 {
            let p = at(pos.x as f32, pos.y as f32);
            paint.rect_filled(Rect::from_min_size(p + vec2(2.0, 2.0) * s, vec2(11.0, 11.0) * s), 0.0, Color32::BLACK);
            tile(23 + pos.facing as usize, p, 2.0);
        }
    }
}
