//! The files the game reads and writes (imported data, saves, the store). Native builds
//! use the disk; the web build keeps them in memory, mirrored to the browser's IndexedDB.

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::{create_dir_all, read, remove_file as remove, write};
#[cfg(not(target_arch = "wasm32"))]
pub fn read_dir(dir: impl AsRef<std::path::Path>) -> std::io::Result<Vec<std::path::PathBuf>> {
    std::fs::read_dir(dir)?.map(|e| e.map(|e| e.path())).collect()
}

#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::{cell::RefCell, collections::BTreeMap, io, path::Path, path::PathBuf};

    use js_sys::{Array, Promise, Uint8Array};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{JsFuture, spawn_local};

    #[wasm_bindgen(inline_js = r#"
let db;
const open = () => db ??= new Promise((ok, err) => {
    navigator.storage?.persist?.();
    const r = indexedDB.open("ddj", 1);
    r.onupgradeneeded = () => r.result.createObjectStore("files");
    r.onsuccess = () => ok(r.result);
    r.onerror = () => err(r.error);
});
const run = async (mode, f) => {
    const t = (await open()).transaction("files", mode), r = f(t.objectStore("files"));
    return new Promise((ok, err) => { t.oncomplete = () => ok(r); t.onerror = () => err(t.error); });
};
export const idb_put = (k, v) => run("readwrite", s => s.put(v, k));
export const idb_del = k => run("readwrite", s => s.delete(k));
export const idb_all = async () => { let k, v; await run("readonly", s => { k = s.getAllKeys(); v = s.getAll(); }); return [k.result, v.result]; };
export const fetch_bytes = async url => { const r = await fetch(url); return r.ok ? new Uint8Array(await r.arrayBuffer()) : null; };
"#)]
    extern "C" {
        fn idb_put(k: &str, v: &Uint8Array) -> Promise;
        fn idb_del(k: &str) -> Promise;
        fn idb_all() -> Promise;
        fn fetch_bytes(url: &str) -> Promise;
    }

    thread_local!(static FILES: RefCell<BTreeMap<String, Vec<u8>>> = RefCell::default());

    fn key(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "/").trim_start_matches("./").to_owned()
    }

    pub fn read(p: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        FILES.with_borrow(|f| f.get(&key(p.as_ref())).cloned()).ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    pub fn write(p: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
        let (k, v) = (key(p.as_ref()), Uint8Array::from(data.as_ref()));
        FILES.with_borrow_mut(|f| f.insert(k.clone(), data.as_ref().to_vec()));
        spawn_local(async move {
            if let Err(e) = JsFuture::from(idb_put(&k, &v)).await {
                web_sys::console::error_2(&format!("could not store {k}").into(), &e);
            }
        });
        Ok(())
    }

    pub fn remove(p: impl AsRef<Path>) -> io::Result<()> {
        let k = key(p.as_ref());
        FILES.with_borrow_mut(|f| f.remove(&k)).ok_or(io::ErrorKind::NotFound)?;
        spawn_local(async move {
            let _ = JsFuture::from(idb_del(&k)).await;
        });
        Ok(())
    }

    pub fn create_dir_all(_: impl AsRef<Path>) -> io::Result<()> {
        Ok(())
    }

    /// The files directly inside `dir`.
    pub fn read_dir(dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
        let pre = format!("{}/", key(dir.as_ref()));
        Ok(FILES.with_borrow(|f| f.keys().filter(|k| k.strip_prefix(&pre).is_some_and(|r| !r.contains('/'))).map(PathBuf::from).collect()))
    }

    /// Loads every stored file; call once before the game starts.
    pub async fn mount() {
        let Ok(all) = JsFuture::from(idb_all()).await else { return };
        let all = Array::from(&all);
        let (keys, vals) = (Array::from(&all.get(0)), Array::from(&all.get(1)));
        FILES.with_borrow_mut(|f| {
            for (k, v) in keys.iter().zip(vals.iter()) {
                f.insert(k.as_string().unwrap_or_default(), Uint8Array::new(&v).to_vec());
            }
        });
    }

    /// Fetches a file served beside the page into memory only (not stored).
    pub async fn fetch(url: &str, p: impl AsRef<Path>) {
        if let Ok(v) = JsFuture::from(fetch_bytes(url)).await
            && !v.is_null()
        {
            FILES.with_borrow_mut(|f| f.insert(key(p.as_ref()), Uint8Array::new(&v).to_vec()));
        }
    }
}
