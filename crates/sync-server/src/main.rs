//! The cloud save server (docs/saves.md): `ddj-sync [folder] [address]`. Each sync code's
//! files sit in a folder named by the code's SHA-256, so the folder names don't give the
//! codes away. Plain HTTP: put it behind something that adds TLS (Tailscale Funnel, Caddy).
//!
//! - `POST /new`: a fresh sync code.
//! - `GET /files`: `{"name": "sha256", ...}` for the code in `Authorization: Bearer <code>`.
//! - `GET /files/<name>`, and `PUT` / `DELETE` it with `X-Base` set to the hash the client
//!   last saw ("none" if it had none); 409 with the current hash when it has changed since.

use std::{collections::BTreeMap, fs, io::Read, path::Path};

use percent_encoding::percent_decode_str;
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_FILE: usize = 4 << 20;
const MAX_FILES: usize = 256;
const MAX_CODES: usize = 100_000;

fn hash(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// A code as typed: letters and digits only, upper case, O read as 0 and I or L as 1.
fn normalize(code: &str) -> String {
    code.chars().filter(char::is_ascii_alphanumeric).map(|c| match c.to_ascii_uppercase() {
        'O' => '0',
        'I' | 'L' => '1',
        c => c,
    }).collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().unwrap_or("cloud-saves".into());
    let addr = args.next().unwrap_or("127.0.0.1:8737".into());
    fs::create_dir_all(&root).expect("could not make the save folder");
    let server = Server::http(&addr).unwrap_or_else(|e| panic!("could not listen on {addr}: {e}"));
    eprintln!("Serving cloud saves from {root} on http://{addr}");
    for mut req in server.incoming_requests() {
        let (status, body) = handle(Path::new(&root), &mut req);
        let mut res = Response::from_data(body).with_status_code(status);
        for (k, v) in [("Access-Control-Allow-Origin", "*"), ("Access-Control-Allow-Headers", "Authorization, X-Base"), ("Access-Control-Allow-Methods", "GET, PUT, DELETE, POST")] {
            res.add_header(Header::from_bytes(k, v).expect("valid header"));
        }
        let _ = req.respond(res);
    }
}

fn header(r: &Request, n: &'static str) -> Option<String> {
    r.headers().iter().find(|h| h.field.equiv(n)).map(|h| h.value.as_str().to_owned())
}

fn handle(root: &Path, req: &mut Request) -> (u16, Vec<u8>) {
    let path = req.url().split('?').next().unwrap_or_default().to_owned();
    let say = |s: u16, m: &str| (s, m.as_bytes().to_vec());
    match (req.method(), path.as_str()) {
        (Method::Options, _) => return (204, Vec::new()),
        (Method::Post, "/new") => {
            if fs::read_dir(root).map_or(0, Iterator::count) >= MAX_CODES {
                return say(503, "This server is full.");
            }
            let mut b = [0u8; 20];
            getrandom::fill(&mut b).expect("no randomness");
            let code: String = b.iter().map(|x| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ"[usize::from(x % 32)] as char).collect();
            let _ = fs::create_dir_all(root.join(hash(code.as_bytes())));
            return (200, [&code[..5], &code[5..10], &code[10..15], &code[15..]].join("-").into_bytes());
        }
        _ => {}
    }
    let Some(code) = header(req, "Authorization").and_then(|a| a.strip_prefix("Bearer ").map(normalize)) else { return say(401, "No sync code.") };
    let dir = root.join(hash(code.as_bytes()));
    if !dir.is_dir() {
        return say(404, "The server has no saves under that sync code.");
    }
    let list = || fs::read_dir(&dir).into_iter().flatten().flatten().filter_map(|e| Some((e.file_name().into_string().ok()?, e.path()))).filter(|(n, _)| n.ends_with(".json"));
    if path == "/files" && *req.method() == Method::Get {
        let all: BTreeMap<String, String> = list().filter_map(|(n, p)| Some((n, hash(&fs::read(p).ok()?)))).collect();
        return (200, serde_json::to_vec(&all).unwrap_or_default());
    }
    let name = path.strip_prefix("/files/").map(|n| percent_decode_str(n).decode_utf8_lossy().into_owned());
    let Some(name) = name.filter(|n| n.ends_with(".json") && n.len() <= 80 && !n.starts_with('.') && !n.contains(['/', '\\', ':'])) else { return say(404, "No such file.") };
    let file = dir.join(&name);
    let method = req.method().clone();
    if method == Method::Get {
        return fs::read(&file).map_or_else(|_| say(404, "No such file."), |b| (200, b));
    }
    let now = fs::read(&file).map_or("none".into(), |b| hash(&b));
    if header(req, "X-Base").as_deref() != Some(now.as_str()) {
        return (409, now.into_bytes());
    }
    match method {
        Method::Put => {
            let mut body = Vec::new();
            if req.as_reader().take(MAX_FILE as u64 + 1).read_to_end(&mut body).is_err() || body.len() > MAX_FILE {
                return say(413, "That file is too big.");
            }
            if now == "none" && list().count() >= MAX_FILES {
                return say(507, "Too many files under this sync code.");
            }
            let tmp = dir.join(format!(".{name}.part"));
            match fs::write(&tmp, &body).and_then(|()| fs::rename(&tmp, &file)) {
                Ok(()) => (200, hash(&body).into_bytes()),
                Err(e) => say(500, &e.to_string()),
            }
        }
        Method::Delete => fs::remove_file(&file).map_or_else(|e| say(500, &e.to_string()), |()| (200, Vec::new())),
        _ => say(405, "Not allowed."),
    }
}
