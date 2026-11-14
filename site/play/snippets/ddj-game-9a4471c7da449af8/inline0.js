
export const pick = dir => new Promise(ok => {
    const i = document.createElement("input");
    i.type = "file";
    i.multiple = true;
    i.webkitdirectory = dir;
    i.onchange = async () => ok(await Promise.all([...i.files].map(async f => [f.name, new Uint8Array(await f.arrayBuffer())])));
    i.click();
});
