# Saves: local-first with optional sync

Decided 2026-11-02. Not built yet; this is the target.

- **One format everywhere.** A versioned serde file per character and one per world
  (town, dungeon state, dropped corpses). Desktop and mobile store them as files; web
  stores the same bytes in IndexedDB.
- **Local is the source of truth.** The game never needs a network. Cloud sync is a
  per-profile toggle that copies the same files up and down.
- **Backends sit behind one trait** (`list`, `get`, `put`, each file carrying a checkpoint
  time and a content hash). Candidates: a small self-hosted server, Google Drive (what
  upstream uses), Steam Cloud if it ships there.
- **Conflicts.** If only one side changed since the last sync, take it. If both changed,
  keep both copies and ask the player which to play; never overwrite silently.
- **Known trade-off.** Keeping both copies lets a player restore a save from before a
  death and skip the corpse run. Accepted for single-player; revisit if it matters.
