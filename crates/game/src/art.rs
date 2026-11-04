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
}

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
        // Only the tiled surfaces repeat; pictures are sampled once.
        let tex = |name: &str, img: ColorImage| ctx.load_texture(name, img, TextureOptions::NEAREST_REPEAT);
        let whole = |name: &str, (w, h, px): Rgba| ctx.load_texture(name, ColorImage::from_rgba_unmultiplied([w, h], &px), TextureOptions::NEAREST);
        Some(Self {
            // The big brick face and the ceiling-to-floor backdrop from the maze sheet.
            wall: tex("wall", crop(&maze, 752, 16, 78, 68)),
            backdrop: tex("backdrop", crop(&maze, 2, 0, 28, 100)),
            door: tex("door", crop(&door, 11, 2, 44, 50)),
            mons: whole("mons", mons),
        })
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
    let mut kinds: Vec<usize> = Vec::new();
    for f in fight.foes.iter().filter(|f| f.hp > 0) {
        if !kinds.contains(&f.monster) {
            kinds.push(f.monster);
        }
    }
    let size = 60.0 * s;
    for (i, &m) in kinds.iter().take(4).enumerate() {
        let (id, uv) = art.portrait(d.monsters[m].picture);
        let x = rect.left() + (rect.width() - size * kinds.len().min(4) as f32 - 6.0 * s * (kinds.len().min(4) as f32 - 1.0)) / 2.0 + i as f32 * (size + 6.0 * s);
        paint.image(id, Rect::from_min_size(pos2(x, rect.top() + 4.0 * s), vec2(size, size)), uv, Color32::WHITE);
    }
}
