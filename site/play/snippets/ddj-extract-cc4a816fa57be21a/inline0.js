
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
