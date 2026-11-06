//! Cloud saves (docs/saves.md): the save files copied to and from a sync server
//! (crates/sync-server) under the player's sync code. The files here stay the source of
//! truth. `base` keeps each file's hash from when both sides last matched, which tells
//! which side changed it since: that side's copy wins, and when both did the player
//! chooses (the store and the Hall of Records merge instead, in main.rs).

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use ddj_extract::vfs as fs;
use eframe::egui;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use web_time::{Duration, Instant};

type Files = BTreeMap<String, Vec<u8>>;
type Hashes = BTreeMap<String, String>;

/// What sync.json (beside the saves folder) keeps.
#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub server: String,
    pub code: String,
    base: Hashes,
    /// When the last sync went through (m/d/yyyy).
    pub last: String,
}

/// A file both sides changed: the other device's copy (none: deleted there).
pub struct Clash {
    pub name: String,
    pub theirs: Option<Vec<u8>>,
    hash: Option<String>,
}

#[derive(Default)]
struct Outcome {
    /// What came down (none: deleted elsewhere).
    got: Vec<(String, Option<Vec<u8>>)>,
    clashes: Vec<Clash>,
    base: Hashes,
    error: Option<String>,
}

fn hash(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

fn set(base: &mut Hashes, name: &str, h: Option<String>) {
    match h {
        Some(h) => base.insert(name.into(), h),
        None => base.remove(name),
    };
}

/// The body of a 200, else the server's message as the error.
fn ok((status, body): (u16, Vec<u8>)) -> Result<Vec<u8>, String> {
    if status == 200 { Ok(body) } else { Err(String::from_utf8_lossy(&body).into_owned()) }
}

#[cfg(not(target_arch = "wasm32"))]
async fn http(method: &str, url: &str, code: &str, base: Option<&str>, body: Option<Vec<u8>>) -> Result<(u16, Vec<u8>), String> {
    #[allow(unused_mut)]
    let mut cfg = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(30)));
    #[cfg(windows)]
    {
        use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
        cfg = cfg.tls_config(TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build());
    }
    let agent: ureq::Agent = cfg.build().into();
    let mut req = ureq::http::Request::builder().method(method).uri(url).header("Authorization", format!("Bearer {code}"));
    if let Some(b) = base {
        req = req.header("X-Base", b);
    }
    let res = match body {
        Some(b) => req.body(b).map(|r| agent.run(r)),
        None => req.body(()).map(|r| agent.run(r)),
    };
    let res = res.map_err(|e| e.to_string())?.map_err(|e| format!("Could not reach the server: {e}"))?;
    let status = res.status().as_u16();
    Ok((status, res.into_body().read_to_vec().map_err(|e| e.to_string())?))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export async function http(method, url, code, base, body) {
    const headers = { Authorization: "Bearer " + code };
    if (base != null) headers["X-Base"] = base;
    try {
        const r = await fetch(url, { method, headers, body });
        return [r.status, new Uint8Array(await r.arrayBuffer())];
    } catch (e) {
        return [0, new TextEncoder().encode("Could not reach the server: " + e)];
    }
}"#)]
extern "C" {
    #[wasm_bindgen(js_name = http)]
    fn http_js(method: &str, url: &str, code: &str, base: Option<String>, body: Option<js_sys::Uint8Array>) -> js_sys::Promise;
}

#[cfg(target_arch = "wasm32")]
async fn http(method: &str, url: &str, code: &str, base: Option<&str>, body: Option<Vec<u8>>) -> Result<(u16, Vec<u8>), String> {
    let p = http_js(method, url, code, base.map(str::to_owned), body.map(|b| js_sys::Uint8Array::from(&b[..])));
    let a = js_sys::Array::from(&wasm_bindgen_futures::JsFuture::from(p).await.map_err(|e| format!("{e:?}"))?);
    let (status, body) = (a.get(0).as_f64().unwrap_or(0.0) as u16, js_sys::Uint8Array::new(&a.get(1)).to_vec());
    if status == 0 { Err(String::from_utf8_lossy(&body).into_owned()) } else { Ok((status, body)) }
}

/// One pass over every file. `full` also brings down what changed elsewhere and finds
/// the files both sides changed; without it only this side's changes go up.
async fn sync(server: String, code: String, local: Files, base: Hashes, full: bool) -> Outcome {
    let mut o = Outcome { base: base.clone(), ..Default::default() };
    if let Err(e) = pass(&server, &code, &local, &base, full, &mut o).await {
        o.error = Some(e);
    }
    o
}

async fn pass(server: &str, code: &str, local: &Files, base: &Hashes, full: bool, o: &mut Outcome) -> Result<(), String> {
    let remote: Hashes = serde_json::from_slice(&ok(http("GET", &format!("{server}/files"), code, None, None).await?)?).map_err(|e| e.to_string())?;
    let names: BTreeSet<&String> = local.keys().chain(remote.keys()).chain(base.keys()).collect();
    for n in names {
        let url = format!("{server}/files/{}", utf8_percent_encode(n, NON_ALPHANUMERIC));
        let (l, r, b) = (local.get(n).map(|x| hash(x)), remote.get(n), base.get(n));
        if l.as_ref() == r {
            set(&mut o.base, n, l);
        } else if l.as_ref() == b || r == b {
            if r == b {
                let (status, msg) = http(if l.is_some() { "PUT" } else { "DELETE" }, &url, code, Some(b.map_or("none", String::as_str)), local.get(n).cloned()).await?;
                match status {
                    200 => set(&mut o.base, n, l),
                    // Changed again since the list: the next full sync sorts it out.
                    409 => {}
                    _ => return Err(String::from_utf8_lossy(&msg).into_owned()),
                }
            } else if full {
                let theirs = if r.is_some() { Some(ok(http("GET", &url, code, None, None).await?)?) } else { None };
                set(&mut o.base, n, theirs.as_deref().map(hash));
                o.got.push((n.clone(), theirs));
            }
        } else if full {
            let theirs = if r.is_some() { Some(ok(http("GET", &url, code, None, None).await?)?) } else { None };
            o.clashes.push(Clash { name: n.clone(), hash: theirs.as_deref().map(hash), theirs });
        }
    }
    Ok(())
}

/// Runs a future that never waits (native requests block) to its end.
#[cfg(not(target_arch = "wasm32"))]
fn block_on<T>(f: impl Future<Output = T>) -> T {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

/// Work in the background: a thread natively, the browser's event loop on the web.
#[cfg(not(target_arch = "wasm32"))]
struct Task<T>(std::sync::mpsc::Receiver<T>);
#[cfg(not(target_arch = "wasm32"))]
fn spawn<T: Send + 'static>(ctx: &egui::Context, f: impl Future<Output = T> + Send + 'static) -> Task<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(block_on(f));
        ctx.request_repaint();
    });
    Task(rx)
}
#[cfg(not(target_arch = "wasm32"))]
impl<T> Task<T> {
    fn done(&self) -> Option<T> {
        self.0.try_recv().ok()
    }
}

#[cfg(target_arch = "wasm32")]
struct Task<T>(std::rc::Rc<std::cell::RefCell<Option<T>>>);
#[cfg(target_arch = "wasm32")]
fn spawn<T: 'static>(ctx: &egui::Context, f: impl Future<Output = T> + 'static) -> Task<T> {
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (s, ctx) = (std::rc::Rc::clone(&slot), ctx.clone());
    wasm_bindgen_futures::spawn_local(async move {
        *s.borrow_mut() = Some(f.await);
        ctx.request_repaint();
    });
    Task(slot)
}
#[cfg(target_arch = "wasm32")]
impl<T> Task<T> {
    fn done(&self) -> Option<T> {
        self.0.borrow_mut().take()
    }
}

fn path() -> PathBuf {
    ddj_extract::home().join("sync.json")
}

/// "host" or "host/" as "https://host".
fn server_url(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    if s.contains("://") { s.into() } else { format!("https://{s}") }
}

/// The save files here, and their hashes.
fn local() -> (Files, Hashes) {
    let files: Files = fs::read_dir(crate::saves()).unwrap_or_default().into_iter().filter(|p| p.extension().is_some_and(|e| e == "json")).filter_map(|p| Some((p.file_name()?.to_str()?.to_owned(), fs::read(&p).ok()?))).collect();
    let hashes = files.iter().map(|(n, b)| (n.clone(), hash(b))).collect();
    (files, hashes)
}

pub struct Sync {
    pub cfg: Config,
    /// The running sync, the hashes it started from, and whether it's full.
    task: Option<(Task<Outcome>, Hashes, bool)>,
    new_code: Option<Task<Result<String, String>>>,
    /// Saves changed since the last sync started.
    dirty: bool,
    began: bool,
    since: Instant,
    /// The player asked for the running sync: say how it went.
    loud: bool,
    pub failed: bool,
    /// Files both sides changed, for the player (or a merge) to settle.
    pub clashes: Vec<Clash>,
}

impl Sync {
    pub fn load() -> Self {
        let cfg = fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { cfg, task: None, new_code: None, dirty: false, began: false, since: Instant::now(), loud: false, failed: false, clashes: Vec::new() }
    }

    fn save(&self) {
        let _ = fs::write(path(), serde_json::to_vec_pretty(&self.cfg).unwrap_or_default());
    }

    pub fn on(&self) -> bool {
        !self.cfg.code.is_empty()
    }

    pub fn busy(&self) -> bool {
        self.task.is_some() || self.new_code.is_some()
    }

    /// The saves changed: they go up within a few seconds.
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    /// Starts a sync, if none is running (else this side's changes go up after it).
    pub fn start(&mut self, ctx: &egui::Context, full: bool) {
        if !self.on() {
            return;
        }
        if self.task.is_some() {
            self.dirty = true;
            return;
        }
        let (files, hashes) = local();
        (self.dirty, self.since) = (false, Instant::now());
        let task = spawn(ctx, sync(self.cfg.server.clone(), self.cfg.code.clone(), files, self.cfg.base.clone(), full));
        self.task = Some((task, hashes, full));
    }

    /// A sync the player asked for: its result is said.
    pub fn now(&mut self, ctx: &egui::Context, full: bool) {
        self.start(ctx, full);
        self.loud = self.task.is_some();
    }

    /// Uses sync code `code` on `server` from now on, and syncs (`loud`: saying how it went).
    pub fn link(&mut self, ctx: &egui::Context, server: &str, code: &str, full: bool, loud: bool) {
        let code = code.trim().to_uppercase();
        if code != self.cfg.code {
            // Another code's files: everything here is new to it.
            self.cfg.base.clear();
            self.clashes.clear();
        }
        (self.cfg.server, self.cfg.code) = (server_url(server), code);
        self.save();
        self.start(ctx, full);
        self.loud = loud && self.task.is_some();
    }

    /// Asks `server` for a new sync code; `poll` hands it back.
    pub fn new_code(&mut self, ctx: &egui::Context, server: &str) {
        let server = server_url(server);
        self.cfg.server = server.clone();
        self.new_code = Some(spawn(ctx, async move { ok(http("POST", &format!("{server}/new"), "", None, None).await?).map(|b| String::from_utf8_lossy(&b).into_owned()) }));
    }

    /// Stops syncing; the files stay here and on the server.
    pub fn off(&mut self) {
        self.cfg = Config { server: self.cfg.server.clone(), ..Default::default() };
        self.clashes.clear();
        self.save();
    }

    /// Each frame: starts the first sync, takes in finished work, and sends changes up
    /// once they have settled a few seconds. While `playing` what changed elsewhere
    /// waits for the next full sync, as the party's files are in use. Returns a new
    /// sync code (Ok) or something to tell the player.
    pub fn poll(&mut self, ctx: &egui::Context, playing: bool) -> Option<Result<String, String>> {
        if !self.began {
            self.began = true;
            self.start(ctx, true);
        }
        let mut said = None;
        if let Some(r) = self.new_code.as_ref().and_then(Task::done) {
            self.new_code = None;
            said = Some(r.map_err(|e| format!("Could not get a sync code: {e}")));
        }
        if let Some(o) = self.task.as_ref().and_then(|t| t.0.done()) {
            let (_, snap, full) = self.task.take().expect("running");
            if let Some(m) = self.apply(o, &snap, full, playing) {
                said = Some(Err(m));
            }
        }
        if self.dirty && self.task.is_none() {
            let wait = Duration::from_secs(10).saturating_sub(self.since.elapsed());
            if wait.is_zero() { self.start(ctx, false) } else { ctx.request_repaint_after(wait) }
        }
        said
    }

    fn apply(&mut self, o: Outcome, snap: &Hashes, full: bool, playing: bool) -> Option<String> {
        let old = std::mem::replace(&mut self.cfg.base, o.base);
        for (n, theirs) in o.got {
            let p = crate::saves().join(&n);
            // Changed here meanwhile, or in use: it stays for the next sync.
            if playing || fs::read(&p).ok().map(|b| hash(&b)).as_ref() != snap.get(&n) {
                set(&mut self.cfg.base, &n, old.get(&n).cloned());
                continue;
            }
            let _ = fs::create_dir_all(crate::saves());
            let _ = match theirs {
                Some(b) => fs::write(&p, b),
                None => fs::remove(&p),
            };
        }
        if full && !playing {
            self.clashes = o.clashes;
        }
        self.failed = o.error.is_some();
        if !self.failed {
            self.cfg.last = crate::today();
        }
        self.save();
        let loud = std::mem::take(&mut self.loud);
        match o.error {
            Some(e) => loud.then(|| format!("Cloud saves could not sync: {e}")),
            None => loud.then(|| "Cloud saves are up to date.".into()),
        }
    }

    /// Settles a clash: this device's copy goes up, or the other's comes down.
    pub fn settle(&mut self, ctx: &egui::Context, c: Clash, theirs: bool) {
        if theirs {
            let p = crate::saves().join(&c.name);
            let _ = match &c.theirs {
                Some(b) => fs::create_dir_all(crate::saves()).and_then(|()| fs::write(&p, b)),
                None => fs::remove(&p),
            };
        }
        set(&mut self.cfg.base, &c.name, c.hash);
        self.save();
        self.start(ctx, false);
    }

    /// On closing: waits for a running sync, then sends up what changed.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn flush(&mut self) {
        if let Some((t, snap, full)) = self.task.take()
            && let Ok(o) = t.0.recv_timeout(std::time::Duration::from_secs(15))
        {
            self.apply(o, &snap, full, true);
        }
        if self.dirty && self.on() {
            let (files, snap) = local();
            let o = block_on(sync(self.cfg.server.clone(), self.cfg.code.clone(), files, self.cfg.base.clone(), false));
            self.apply(o, &snap, false, true);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// Two devices against a real ddj-sync (set DDJ_SYNC_TEST to its URL, e.g.
    /// http://127.0.0.1:8737, with the server running).
    #[test]
    fn two_devices() {
        let Ok(server) = std::env::var("DDJ_SYNC_TEST") else { return };
        let code = ok(block_on(http("POST", &format!("{server}/new"), "", None, None)).unwrap()).map(|b| String::from_utf8(b).unwrap()).unwrap();
        let run = |local: &Files, base: &Hashes, full| block_on(sync(server.clone(), code.clone(), local.clone(), base.clone(), full));
        let f = |v: &[(&str, &str)]| -> Files { v.iter().map(|(n, b)| (n.to_string(), b.as_bytes().to_vec())).collect() };
        // Device A sends two characters up.
        let a = f(&[("Kira.json", "a1"), ("Ge'nal Two.json", "g1")]);
        let o = run(&a, &Hashes::new(), true);
        assert!(o.error.is_none() && o.got.is_empty() && o.clashes.is_empty(), "{:?}", o.error);
        let base_a = o.base;
        assert_eq!(base_a.len(), 2);
        // Device B, empty, brings both down.
        let o = run(&Files::new(), &Hashes::new(), true);
        assert_eq!(o.got.len(), 2);
        let mut b: Files = o.got.into_iter().map(|(n, x)| (n, x.unwrap())).collect();
        let base_b = o.base;
        assert_eq!(b, a);
        // B plays Kira and deletes Ge'nal; a push sends both changes.
        b.insert("Kira.json".into(), b"a2".to_vec());
        b.remove("Ge'nal Two.json");
        let o = run(&b, &base_b, false);
        assert!(o.error.is_none() && o.base.len() == 1);
        let base_b = o.base;
        // A, unchanged since, takes them in.
        let o = run(&a, &base_a, true);
        let got: BTreeMap<_, _> = o.got.into_iter().collect();
        assert_eq!(got["Kira.json"].as_deref(), Some(&b"a2"[..]));
        assert_eq!(got["Ge'nal Two.json"], None);
        let (mut a, base_a) = (f(&[("Kira.json", "a2")]), o.base);
        // Both play Kira: A pushes first; B's push is refused, and its full sync clashes.
        a.insert("Kira.json".into(), b"a3".to_vec());
        b.insert("Kira.json".into(), b"b3".to_vec());
        assert_eq!(run(&a, &base_a, false).base["Kira.json"], hash(b"a3"));
        let o = run(&b, &base_b, false);
        assert_eq!(o.base["Kira.json"], hash(b"a2"));
        let o = run(&b, &base_b, true);
        assert_eq!(o.clashes.len(), 1);
        assert_eq!(o.clashes[0].theirs.as_deref(), Some(&b"a3"[..]));
        // A wrong code is told apart from a dead server.
        let o = block_on(sync(server.clone(), "NOPE".into(), Files::new(), Hashes::new(), true));
        assert!(o.error.unwrap().contains("no saves"));
    }
}
