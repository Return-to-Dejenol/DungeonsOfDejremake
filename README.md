# Depths of Dejenol remake (Rust)

A modernized remake of *Mordor: The Depths of Dejenol* (1995), written in Rust.
It started as a fork of [matteo-prosperi/DungeonsOfDejremake](https://github.com/matteo-prosperi/DungeonsOfDejremake);
that project publishes no source, so everything here is new. Its README is kept at
[docs/upstream-README.md](docs/upstream-README.md).

Like upstream, this repo and its builds contain **no original-game content**. The player
imports their own copy, and the importer extracts data tables, MIDI music and sounds into
`gamedata/` (gitignored, never redistributed). The remake's rebalance is shipped as
patch files that apply on top of those tables.

## Layout

- `crates/rules`: headless game rules and the tuning layer. No window, fully testable.
- `crates/extract`: imports `MORDOR11.ZIP`, a `.WIP` archive, or an install folder.
- `crates/game`: thin egui frontend.
- `patches/modern.toml`: the rebalance (XP curve, depth rewards, SP, potion and lock rates).
  Any other `patches/*.toml` overrides it key by key.

## Run

```
cargo run -p ddj-extract -- tests/fixtures/MORDOR11.ZIP
cargo run -p ddj-game
```

The bundled shareware covers floors 1-3; point the importer at a retail copy for all 15.
