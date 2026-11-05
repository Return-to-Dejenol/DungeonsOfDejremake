//! The Android build: start-up, and choosing the player's own game files with the
//! system's document picker (the Java side is android/Main.java).

use std::{fs, io, sync::OnceLock};

use jni::{jni_sig, jni_str, objects::JObject};

pub use android_activity::AndroidApp as App;

static APP: OnceLock<App> = OnceLock::new();

/// Keeps the game's data and saves in the app's own storage, and unpacks the SoundFont
/// from the APK's assets once, to where the music looks for it (`sound/`).
pub fn start(app: &App) {
    let _ = APP.set(app.clone());
    let Some(dir) = app.internal_data_path() else { return };
    // SAFETY: nothing else runs yet.
    unsafe { std::env::set_var("XDG_DATA_HOME", &dir) };
    let _ = std::env::set_current_dir(&dir);
    let font = dir.join("sound/GeneralUser-GS.sf2");
    if !font.exists()
        && let Some(mut asset) = app.asset_manager().open(c"GeneralUser-GS.sf2")
    {
        let part = font.with_extension("part");
        let _ = fs::create_dir_all(dir.join("sound"));
        if fs::File::create(&part).and_then(|mut f| io::copy(&mut asset, &mut f)).is_ok() {
            let _ = fs::rename(&part, &font);
        }
    }
}

/// Opens the document picker; the chosen files arrive through `picked`.
pub fn choose() {
    let Some(app) = APP.get() else { return };
    // SAFETY: the pointers are the running activity's VM and its global reference.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        env.call_method(&activity, jni_str!("pick"), jni_sig!(() -> ()), &[]).map(drop)
    });
}

/// The files the picker copied into `files/picked/`, as (name, contents), once there.
pub fn picked() -> Option<Vec<(String, Vec<u8>)>> {
    let dir = APP.get()?.internal_data_path()?.join("picked");
    let files = fs::read_dir(&dir).ok()?.flatten().filter_map(|e| Some((e.file_name().into_string().ok()?, fs::read(e.path()).ok()?))).collect();
    let _ = fs::remove_dir_all(&dir);
    Some(files)
}
