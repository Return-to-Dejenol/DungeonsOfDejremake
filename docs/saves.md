# Saves: local-first with optional sync

Decided 2026-11-02; cloud sync built 2026-11-05 (`crates/game/src/sync.rs`, `crates/sync-server`).

- **One format everywhere.** The saves folder holds a JSON file per character, plus
  `store.json` (the shared store stock) and `hall.json` (the Hall of Records). Desktop and
  Android keep them as files; the web keeps the same bytes in IndexedDB.
- **Local is the source of truth.** The game never needs a network. Cloud saves are off
  until the player turns them on (Options, where the original had its file backups).
- **The server** is `ddj-sync [folder] [address]`, one small binary (default
  `127.0.0.1:8737`, plain HTTP: put TLS in front, e.g. Tailscale Funnel or Caddy). It keeps
  each sync code's files in a folder named by the code's SHA-256. `POST /new` makes a code;
  `GET /files` lists names and hashes; `GET`/`PUT`/`DELETE /files/<name>` move one file, a
  write carrying the hash it expects to replace (`X-Base`) so a stale one gets 409.
- **Linking devices is a sync code** (`XXXXX-XXXXX-XXXXX-XXXXX`, 100 random bits): no
  accounts or passwords. Typing it on another device shares the saves. It is the only
  key, so anyone with it can read and change those saves.
- **When it syncs.** In full at start-up and on Sync Now from the menu: changes go both
  ways. While playing, only this device's changes go up, a few seconds after each save,
  and once more on closing (desktop). What changed elsewhere waits for the next full
  sync, since the party's files are in use.
- **Conflicts.** The client keeps each file's hash from the last time both sides matched
  (`sync.json`, beside the saves). If only one side changed since, it wins. If both did:
  a character asks the player which copy to keep (closing the window asks again next
  time); the store keeps its newer copy (its `saved` time); the Hall of Records takes the
  better of each record. Deletions sync like changes.
- **Known trade-off.** Choosing the other device's copy lets a player restore a save
  from before a death and skip the corpse run. Accepted for single-player.
- **The project's server** runs on sereneblossomns (`C:\ddj-sync`, a scheduled task at
  boot as SYSTEM, saves in `C:\ddj-sync\saves`) behind Tailscale Funnel at
  `https://sereneblossomns1.tail042c2e.ts.net`, the game's default (2026-11-05).
- **Not done yet:** Google Drive or Steam Cloud backends; encrypting saves so the server
  can't read them.
