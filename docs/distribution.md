# Distribution

Decided 2026-11-04. Built so far: the shareware default, registered import, the web
build (2026-11-04) and the Android APK (2026-11-05).

- **Shareware by default.** Every build ships the unmodified `MORDOR11.ZIP` (its
  `MORDOR.WRI` allows redistribution) and imports it on first run, so the game starts
  with floors 1-3 and no setup. DISCLAIMER.md's "no original content in the builds"
  changes to name this one exception.
- **Registered = 15 floors.** The player points the game at their own copy (folder or
  zip) on any platform. If the imported `MDATA11` has all 15 floors, the full game
  unlocks. No hashes or registration codes: the data itself is the gate.
- **Desktop.** The Windows installer, as now.
- **Android.** The APK is published openly (it holds only project code, the shareware
  and the SoundFont). The player imports their files on the phone with the system's
  document picker (`android/Main.java` copies them into the app's storage). Built by
  `scripts/build-apk.ps1` without Gradle, for arm64 and x86_64, Android 8 and up, in either orientation. It is
  signed with `~/.android/dejenol-release.jks` (made on the first build): keep that key,
  since an APK signed with another can't update an installed copy. Updates are a new APK;
  the Windows updater isn't built here.
- **Web.** A static WASM build of the same eframe app (`crates/game/index.html`, built
  with trunk into `target/web/`). The player drops or chooses their files in the
  browser, which keeps them and the saves in IndexedDB (`crates/extract/src/vfs.rs`).
  The SoundFont downloads beside the page (32 MB); music starts once it's in. The
  server never holds or sends registered game data.
- **Moving between devices** is cloud saves (docs/saves.md): the save files only, never game data.
