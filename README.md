# Depths of Dejenol remake (Rust)

A modernized remake of *Mordor: The Depths of Dejenol* (1995), written in Rust.
It started as a fork of [matteo-prosperi/DungeonsOfDejremake](https://github.com/matteo-prosperi/DungeonsOfDejremake);
that project publishes no source, so everything here is new. Its README is kept at
[docs/upstream-README.md](docs/upstream-README.md).

Apart from the original's freely redistributable shareware release (floors 1-3), which
every build ships and starts with, this repo and its builds contain **no original-game
content**. For the full game the player imports their own registered copy, and the
importer extracts data tables, pictures, MIDI music and sounds (never the program
itself). How the builds are distributed is in [docs/distribution.md](docs/distribution.md). The remake's rebalance is shipped as patch files that
apply on top of those tables; the rules it changes are in [docs/balance.md](docs/balance.md).

## Layout

- `crates/rules`: headless game rules: the party, combat, magic, treasure, monsters,
  the city, party AI macros, and the tuning layer. No window, fully testable.
- `crates/extract`: imports `MORDOR11.ZIP`, a `.WIP` archive, or an install folder, and
  reads the original's bitmaps.
- `crates/game`: the egui window: the original's art and music, the city, the dungeon.
- `patches/modern.toml`: the rebalance start values. Any other `patches/*.toml` in the
  data folder overrides it key by key.

## Play

Install with `DejenolRemake-Setup-<version>.exe` and start it: it opens on the shareware.
For all 15 floors, choose *Import your registered copy* and drop your copy of the
original (its ZIP, `MORDOR.WIP` or install folder) on the window. Imported data, saves
and patches live in `%LOCALAPPDATA%\DejenolRemake`; uninstalling leaves them.

In the browser version the import screen also has *Choose files* and *Choose folder*.
The browser keeps the imported files and saves in its own storage (IndexedDB) for that
site; clearing the site's data deletes them.

## Run from source

```
cargo run -p ddj-extract -- tests/fixtures/MORDOR11.ZIP
cargo run -p ddj-game
```

Running from the source tree keeps data in `gamedata/` and saves in `saves/` (both
gitignored). The bundled shareware covers floors 1-3; point the importer at a retail copy
for all 15. Music needs the GeneralUser GS SoundFont in `vendor/soundfont/` (see
`scripts/build-installer.ps1`); without it the game runs silent but for effects.

## Edit saves

`ddj-save` shows and edits the remake's saves and the original's character file
(MDATA4.MDR), for testing:

```
cargo run -p ddj-rules --bin ddj-save -- show saves/Ann.json
cargo run -p ddj-rules --bin ddj-save -- set saves/Ann.json gold 50000
cargo run -p ddj-rules --bin ddj-save -- set C:/MORDOR/DATA/MDATA4.MDR Ann.guild.0.level 30
```

Remake fields are JSON paths (`stats.0`, `items.2.charges`); the original's are
`NAME.field`, listed by `show`. A remake save is checked before it is written.
`scripts/re/` holds the tools that read the original's rules out of MORDOR.EXE.

## Build the web version

```
cd crates/game
trunk build --release
```

Needs `rustup target add wasm32-unknown-unknown`, [trunk](https://trunkrs.dev) and the
SoundFont in `vendor/soundfont/`. The site lands in `target/web/` and is static: serve
it from any web host (`trunk serve` for a local one). It holds the game, the shareware
and the SoundFont, never a registered copy.

## Build the installer

```
powershell -File scripts/build-installer.ps1
```

Needs Inno Setup 6 and the SoundFont in `vendor/soundfont/`. The installer lands in
`target/installer/`.
