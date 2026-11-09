//! Updates for the Windows game: at start, at most once a day, it asks GitHub for the
//! project's latest release. A newer one is offered; once accepted its Setup is downloaded
//! and run silently, and the installer upgrades in place and starts the game again.

use std::{fs, io, path::Path, process::Command, sync::mpsc, thread};

use eframe::egui;

/// The latest release (`DDJ_UPDATE_URL` stands in for testing).
const LATEST: &str = "https://api.github.com/repos/Return-to-Dejenol/DungeonsOfDejremake/releases/latest";

pub struct Release {
    pub version: String,
    setup: String,
}

/// Whether version `tag` ("v0.2.0") is newer than `mine`.
fn newer(tag: &str, mine: &str) -> bool {
    let parts = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(tag) > parts(mine)
}

/// A GET over Windows' own TLS (SChannel) and its trusted roots.
fn get(url: &str) -> Result<ureq::Body, ureq::Error> {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).build().into();
    Ok(agent.get(url).header("User-Agent", "DejenolRestoration").call()?.into_body())
}

/// Asks GitHub for the latest release: one newer than this game, or none.
fn check(ctx: Option<egui::Context>) -> mpsc::Receiver<Result<Option<Release>, String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let url = std::env::var("DDJ_UPDATE_URL").unwrap_or(LATEST.into());
        let json = get(&url).and_then(|mut b| b.read_to_string()).map_err(|e| e.to_string());
        let r = json.and_then(|j| serde_json::from_str::<serde_json::Value>(&j).map_err(|e| e.to_string())).map(|r| {
            let tag = r["tag_name"].as_str().unwrap_or_default().to_owned();
            let setup = r["assets"].as_array().into_iter().flatten().filter_map(|a| a["browser_download_url"].as_str()).find(|u| u.ends_with(".exe"));
            setup.filter(|_| newer(&tag, env!("CARGO_PKG_VERSION"))).map(|s| Release { version: tag.trim_start_matches('v').into(), setup: s.into() })
        });
        let _ = tx.send(r);
        ctx.inspect(egui::Context::request_repaint);
    });
    rx
}

/// Downloads the release's Setup and starts it silently; the game should then close.
pub fn install(r: &Release) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("DejenolRestoration-Setup-{}.exe", r.version));
    let fail = |e: &dyn std::fmt::Display| format!("Could not download version {}: {e}", r.version);
    let mut body = get(&r.setup).map_err(|e| fail(&e))?.into_reader();
    io::copy(&mut body, &mut fs::File::create(&path).map_err(|e| fail(&e))?).map_err(|e| fail(&e))?;
    Command::new(&path).args(["/SILENT", "/SP-", "/NOCANCEL", "/CLOSEAPPLICATIONS"]).spawn().map_err(|e| format!("Could not start the update: {e}"))?;
    Ok(())
}

/// The check, the release it found, and the download once accepted.
#[derive(Default)]
pub struct Updater {
    found: Option<mpsc::Receiver<Result<Option<Release>, String>>>,
    release: Option<Release>,
    /// The player asked (Options): say how the check went, even with no news.
    loud: bool,
    done: Option<mpsc::Receiver<Result<(), String>>>,
}

impl Updater {
    /// Checks at start, unless a check ran today (`today`, kept in `dir`).
    pub fn start(dir: &Path, today: &str) -> Self {
        let stamp = dir.join("update-checked.txt");
        let due = !fs::read_to_string(&stamp).is_ok_and(|d| d == today);
        if due {
            let _ = fs::create_dir_all(dir);
            let _ = fs::write(&stamp, today);
        }
        Self { found: due.then(|| check(None)), ..Default::default() }
    }

    /// Checks now, for the player.
    pub fn check_now(&mut self, ctx: &egui::Context) {
        (self.found, self.loud) = (Some(check(Some(ctx.clone()))), true);
    }

    /// Once the check is back: the question to ask about a newer release (Ok), or what
    /// to tell a player who asked (Err).
    pub fn offer(&mut self) -> Option<Result<String, String>> {
        let r = self.found.as_ref()?.try_recv().ok()?;
        self.found = None;
        let loud = std::mem::take(&mut self.loud);
        match r {
            Ok(Some(r)) => {
                let q = format!("Version {} of the Dejenol Restoration Project is available. Install it now?", r.version);
                self.release = Some(r);
                Some(Ok(q))
            }
            Ok(None) => loud.then(|| Err(format!("You have the latest version ({}).", env!("CARGO_PKG_VERSION")))),
            Err(e) => loud.then(|| Err(format!("Could not check for updates: {e}"))),
        }
    }

    /// Downloads and starts the update in the background, then closes the game.
    pub fn accept(&mut self, ctx: &egui::Context) {
        let (Some(r), ctx) = (self.release.take(), ctx.clone()) else { return };
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let res = install(&r);
            if res.is_ok() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            let _ = tx.send(res);
            ctx.request_repaint();
        });
        self.done = Some(rx);
    }

    /// What went wrong, if the download or the Setup failed.
    pub fn error(&mut self) -> Option<String> {
        self.done.as_ref()?.try_recv().ok()?.err()
    }
}

#[test]
fn compares_versions() {
    assert!(newer("v0.2.0", "0.1.9") && newer("v0.1.10", "0.1.9") && newer("1.0", "0.9.9"));
    assert!(!newer("v0.1.0", "0.1.0") && !newer("v0.0.9", "0.1.0"));
}
