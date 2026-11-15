//! The Options window's settings, kept as the original kept them in MORDOR.INI's
//! [Loading Information] (here options.json beside the saves). Every box starts
//! unchecked and both volumes at their top, as with a missing INI, except "Use Sound
//! Canvas MIDI", checked (owner, 2026-11-15: the richer sound, and any system plays it).

use ddj_extract::vfs as fs;
use serde::{Deserialize, Serialize};

/// The sliders' top (hsbVolume's default Max).
pub const LOUD: usize = 32_767;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Opts {
    pub no_3d: bool,
    pub no_music: bool,
    pub no_fx: bool,
    pub no_stretch: bool,
    pub sound_canvas: bool,
    pub no_ding: bool,
    pub no_load_automap: bool,
    pub no_toolbar: bool,
    pub no_party_fx: bool,
    pub no_help_sound: bool,
    pub no_autosave: bool,
    pub no_blink: bool,
    pub music_volume: usize,
    pub fx_volume: usize,
    /// The Control Panel's Preload Char.: the character loaded at start-up.
    pub preload: Option<String>,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            no_3d: false,
            no_music: false,
            no_fx: false,
            no_stretch: false,
            sound_canvas: true,
            no_ding: false,
            no_load_automap: false,
            no_toolbar: false,
            no_party_fx: false,
            no_help_sound: false,
            no_autosave: false,
            no_blink: false,
            music_volume: LOUD,
            fx_volume: LOUD,
            preload: None,
        }
    }
}

fn path() -> std::path::PathBuf {
    ddj_extract::home().join("options.json")
}

impl Opts {
    pub fn load() -> Self {
        fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(path(), b);
        }
    }

    /// The Options window's check boxes (chkCheck 0-11) as fields.
    pub fn check(&mut self, k: usize) -> &mut bool {
        match k {
            0 => &mut self.no_3d,
            1 => &mut self.no_music,
            2 => &mut self.no_fx,
            3 => &mut self.no_stretch,
            4 => &mut self.sound_canvas,
            5 => &mut self.no_ding,
            6 => &mut self.no_load_automap,
            7 => &mut self.no_toolbar,
            8 => &mut self.no_party_fx,
            9 => &mut self.no_help_sound,
            10 => &mut self.no_autosave,
            _ => &mut self.no_blink,
        }
    }
}
