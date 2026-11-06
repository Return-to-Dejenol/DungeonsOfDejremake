//! The browser build: start-up, and choosing the player's own game files.

use std::cell::RefCell;

use eframe::egui;
use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[wasm_bindgen(inline_js = r#"
export const pick = dir => new Promise(ok => {
    const i = document.createElement("input");
    i.type = "file";
    i.multiple = true;
    i.webkitdirectory = dir;
    i.onchange = async () => ok(await Promise.all([...i.files].map(async f => [f.name, new Uint8Array(await f.arrayBuffer())])));
    i.click();
});
"#)]
extern "C" {
    fn pick(dir: bool) -> Promise;
}

/// Files as (name, contents).
type Files = Vec<(String, Vec<u8>)>;

thread_local!(static PICKED: RefCell<Option<Files>> = const { RefCell::new(None) });

/// Opens the browser's file (or folder) chooser; the files arrive through `picked`.
pub fn choose(dir: bool, ctx: egui::Context) {
    spawn_local(async move {
        let Ok(v) = JsFuture::from(pick(dir)).await else { return };
        let files = Array::from(&v).iter().map(|p| Array::from(&p)).map(|p| (p.get(0).as_string().unwrap_or_default(), Uint8Array::new(&p.get(1)).to_vec())).collect();
        PICKED.set(Some(files));
        ctx.request_repaint();
    });
}

/// Reads files dropped on the page; they arrive through `picked`.
pub fn read_dropped(files: Vec<std::sync::Arc<dyn egui::DroppedFile + Send + Sync>>, ctx: egui::Context) {
    spawn_local(async move {
        let mut got = Vec::new();
        for f in &files {
            if let Ok(b) = f.bytes_async().await {
                got.push((f.path().to_string_lossy().into_owned(), b));
            }
        }
        PICKED.set(Some(got));
        ctx.request_repaint();
    });
}

pub fn picked() -> Option<Files> {
    PICKED.take()
}

/// Loads the stored files, then runs the game on the page's canvas. The SoundFont
/// downloads alongside; music starts once it's in.
pub fn start() {
    spawn_local(ddj_extract::vfs::fetch("sound/GeneralUser-GS.sf2", "sound/GeneralUser-GS.sf2"));
    spawn_local(async {
        #[cfg(debug_assertions)]
        ddj_extract::vfs::fetch("fonts/sserife.fon", "fonts/sserife.fon").await;
        ddj_extract::vfs::mount().await;
        let canvas = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("game")).and_then(|c| c.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        let Some(canvas) = canvas else { return web_sys::console::error_1(&"no canvas".into()) };
        if let Err(e) = eframe::WebRunner::new().start(canvas, eframe::WebOptions::default(), Box::new(|_| Ok(Box::new(crate::App::new())))).await {
            web_sys::console::error_1(&e);
        }
    });
}
