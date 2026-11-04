# Depths of Dejenol remake (Rust)

A modernized remake of *Mordor: The Depths of Dejenol* (1995), written in Rust.
It started as a fork of [matteo-prosperi/DungeonsOfDejremake](https://github.com/matteo-prosperi/DungeonsOfDejremake);
that project publishes no source, so everything here is new. Its README is kept at
[docs/upstream-README.md](docs/upstream-README.md).

Like upstream, this repo and its builds contain **no original-game content**. The player
imports their own copy, and the importer extracts data tables, pictures, MIDI music and
sounds (never the program itself). The remake's rebalance is shipped as patch files that
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

Install with `DejenolRemake-Setup-<version>.exe`, start it, and drop your copy of the
original (its ZIP, `MORDOR.WIP` or install folder) on the window. Imported data, saves
and patches live in `%LOCALAPPDATA%\DejenolRemake`; uninstalling leaves them.

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

## Build the installer

```
powershell -File scripts/build-installer.ps1
```

Needs Inno Setup 6 and the SoundFont in `vendor/soundfont/`. The installer lands in
`target/installer/`.
