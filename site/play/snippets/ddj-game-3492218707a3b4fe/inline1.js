
export async function http(method, url, code, base, body) {
    const headers = { Authorization: "Bearer " + code };
    if (base != null) headers["X-Base"] = base;
    try {
        const r = await fetch(url, { method, headers, body });
        return [r.status, new Uint8Array(await r.arrayBuffer())];
    } catch (e) {
        return [0, new TextEncoder().encode("Could not reach the server: " + e)];
    }
}