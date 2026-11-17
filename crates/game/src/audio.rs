//! Sound: the original's WAV effects, and its MIDI music (the Sound Canvas versions)
//! through the GeneralUser GS SoundFont, which the installer ships next to the game.
//! Without the SoundFont the music is skipped.

use std::{
    io::Cursor,
    num::NonZero,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use ddj_extract::vfs;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};

const RATE: u32 = 44_100;
const FONT: &str = "GeneralUser-GS.sf2";
/// The original's songs in its order (MUSIC.sub170B), the Sound Canvas versions.
pub const TRACKS: [&str; 8] =
    ["CITYSC.MID", "MAINSC.MID", "DUNGEOSC.MID", "INTROSC.MID", "GENERASC.MID", "GEN2SC.MID", "GEN3SC.MID", "THEMESC.MID"];
/// The same songs as plain General MIDI files, played when the Options window's "Use
/// Sound Canvas MIDI" is unchecked (sub170B; it starts checked).
const PLAIN: [&str; 8] = ["CITY.MID", "MAIN.MID", "DUNGEON.MID", "INTRO.MID", "GENERAL.MID", "GEN2.MID", "GEN3.MID", "THEME.MID"];
pub const CITY: &str = TRACKS[0];
pub const DUNGEON: &str = TRACKS[2];
pub const THEME: &str = TRACKS[7];
/// The original's sound effects by number (gv20CA).
pub const FILES: [&str; 72] = [
    "FIGHT.WAV", "DIE.WAV", "HIT.WAV", "KILL.WAV", "MAGIC.WAV", "OPEN.WAV", "FALL.WAV", "BLACKOUT.WAV", "SCURRY.WAV", "MOAN.WAV", "DSTSLAM.WAV", "RAISE.WAV",
    "STEAL.WAV", "BUG.WAV", "SLIME.WAV", "GOBLIN.WAV", "DOG.WAV", "DEVIL.WAV", "SNAKE.WAV", "UNDEAD.WAV", "WATERD.WAV", "GIANT.WAV", "MYTHIC.WAV", "LYCAN.WAV",
    "MAGE.WAV", "WARRIOR.WAV", "WATER.WAV", "WATER1.WAV", "FIRE.WAV", "TELEPORT.WAV", "DI-DEMON.WAV", "DI-HUMAN.WAV", "DI-ANIM.WAV", "SHOCK.WAV", "BYE.WAV",
    "OUCH.WAV", "SWING.WAV", "PARAL.WAV", "POISON.WAV", "DISEASE.WAV", "BETTER.WAV", "FOUND.WAV", "COLD.WAV", "EXPLOS.WAV", "MAGIC2.WAV", "WADE.WAV",
    "SLICE.WAV", "DRAIN.WAV", "FEAR.WAV", "SLIMETR.WAV", "SPIT.WAV", "STONE.WAV", "WITHER.WAV", "MAKELEV.WAV", "BUGDIE.WAV", "RIP.WAV", "HELP.WAV", "DEMON.WAV",
    "DRAGON.WAV", "BIRD.WAV", "BIRDDIE.WAV", "EXTING.WAV", "HELLO.WAV", "YEECH.WAV", "DI-UNDEA.WAV", "FOG.WAV", "MESSAGE.WAV", "DROWN.WAV", "ASMO.WAV",
    "PLANTATT.WAV", "BSTAB.WAV", "CHIT.WAV",
];
pub const HELP: u8 = 56;
pub const MESSAGE: u8 = 66;

pub struct Audio {
    sink: MixerDeviceSink,
    font: Option<Arc<SoundFont>>,
    font_tried: bool,
    music: Option<Player>,
    track: Option<&'static str>,
    scene: Option<&'static str>,
    pub music_on: bool,
    pub sound_on: bool,
    pub sound_canvas: bool,
    /// The Options window's sliders, 0 to 1.
    pub music_volume: f32,
    pub fx_volume: f32,
}

/// A MIDI song rendered a block at a time.
struct Song {
    seq: MidiFileSequencer,
    left: Vec<f32>,
    right: Vec<f32>,
    /// Next sample, counting left and right separately.
    at: usize,
}

impl Iterator for Song {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.at >= 2 * self.left.len() {
            self.seq.render(&mut self.left, &mut self.right);
            self.at = 0;
        }
        let i = self.at / 2;
        let s = if self.at % 2 == 0 { self.left[i] } else { self.right[i] };
        self.at += 1;
        (!self.seq.end_of_sequence()).then_some(s)
    }
}

impl Source for Song {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// The SoundFont: fetched beside the web page into memory (web/index.html).
#[cfg(target_arch = "wasm32")]
fn font_path() -> Option<PathBuf> {
    Some(PathBuf::from("sound").join(FONT))
}

/// The SoundFont: beside the installed game, or in the source tree's `vendor/`.
#[cfg(not(target_arch = "wasm32"))]
fn font_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("sound").join(FONT)));
    exe.into_iter().chain([PathBuf::from("sound").join(FONT), PathBuf::from("vendor/soundfont").join(FONT)]).find(|p| p.exists())
}

impl Audio {
    pub fn new() -> Option<Self> {
        let mut sink = DeviceSinkBuilder::open_default_sink().ok()?;
        sink.log_on_drop(false);
        Some(Self { sink, font: None, font_tried: false, music: None, track: None, scene: None, music_on: true, sound_on: true, sound_canvas: true, music_volume: 1.0, fx_volume: 1.0 })
    }

    /// The original's music (frmMain.timMusic, sub1A50): each screen starts its song (the
    /// one playing carries on). When a song ends the menu and the city play theirs again;
    /// the dungeon repeats it one time in four, else plays any song but the city's.
    pub fn music(&mut self, dir: &Path, scene: &'static str) {
        if !self.music_on {
            return self.stop_music();
        }
        let playing = self.music.as_ref().is_some_and(|p| !p.empty());
        let track = if self.scene != Some(scene) {
            scene
        } else if playing {
            return;
        } else if scene != DUNGEON {
            scene
        } else {
            let r = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
            if r % 100 < 25 { self.track.unwrap_or(scene) } else { TRACKS[1 + r / 100 % 7] }
        };
        self.scene = Some(scene);
        if playing && self.track == Some(track) {
            return;
        }
        self.stop_music();
        self.scene = Some(scene);
        // The web build's SoundFont arrives after start-up, so it keeps looking until then.
        if !self.font_tried {
            self.font = font_path().and_then(|p| SoundFont::new(&mut Cursor::new(vfs::read(p).ok()?)).ok()).map(Arc::new);
            self.font_tried = self.font.is_some() || cfg!(not(target_arch = "wasm32"));
        }
        let file = if self.sound_canvas { track } else { TRACKS.iter().position(|&t| t == track).map_or(track, |i| PLAIN[i]) };
        let (Some(font), Ok(b)) = (&self.font, vfs::read(dir.join(file))) else { return };
        let Ok(midi) = MidiFile::new(&mut Cursor::new(b)) else { return };
        let Ok(synth) = Synthesizer::new(font, &SynthesizerSettings::new(RATE as i32)) else { return };
        let mut seq = MidiFileSequencer::new(synth);
        seq.play(&Arc::new(midi), false);
        let player = Player::connect_new(self.sink.mixer());
        player.set_volume(0.45 * self.music_volume);
        player.append(Song { seq, left: vec![0.0; 1024], right: vec![0.0; 1024], at: 2048 });
        self.music = Some(player);
        self.track = Some(track);
    }

    pub fn stop_music(&mut self) {
        if let Some(p) = self.music.take() {
            p.stop();
        }
        self.track = None;
        self.scene = None;
    }

    /// Effect `n` (sub09DE), unless the Options turn sound effects off.
    pub fn effect(&self, dir: &Path, n: u8) {
        if self.sound_on {
            self.play(dir, n, self.fx_volume);
        }
    }

    /// Effect `n` at volume `vol` (0 to 1), sound effects on or off.
    pub fn play(&self, dir: &Path, n: u8, vol: f32) {
        if let Some(wav) = FILES.get(usize::from(n))
            && let Ok(b) = vfs::read(dir.join(wav))
            && let Ok(d) = Decoder::try_from(Cursor::new(b))
        {
            self.sink.mixer().add(d.amplify(0.8 * vol));
        }
    }

    /// The Options window's settings, applied at once: a new song choice starts over.
    pub fn apply(&mut self, music_on: bool, sound_on: bool, sound_canvas: bool, music: f32, fx: f32) {
        if sound_canvas != self.sound_canvas || !music_on {
            self.stop_music();
        }
        (self.music_on, self.sound_on, self.sound_canvas, self.music_volume, self.fx_volume) = (music_on, sound_on, sound_canvas, music, fx);
        if let Some(p) = &self.music {
            p.set_volume(0.45 * music);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dungeon song renders through the SoundFont (when present) and every effect of
    /// the original's table exists in the import, but FIGHT.WAV: nothing plays it, and the
    /// game doesn't ship it.
    #[test]
    fn music_renders_and_effects_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = root.join("target/ddj-audio-test");
        ddj_extract::import(&root.join("tests/fixtures/MORDOR11.ZIP"), &out).unwrap();
        for w in &FILES[1..] {
            assert!(out.join(w).exists(), "{w}");
        }
        let Some(font) = [root.join("vendor/soundfont").join(FONT)].into_iter().find(|p| p.exists()) else { return };
        let font = Arc::new(SoundFont::new(&mut Cursor::new(vfs::read(font).unwrap())).unwrap());
        let midi = Arc::new(MidiFile::new(&mut Cursor::new(vfs::read(out.join("DUNGEOSC.MID")).unwrap())).unwrap());
        let mut seq = MidiFileSequencer::new(Synthesizer::new(&font, &SynthesizerSettings::new(RATE as i32)).unwrap());
        seq.play(&midi, true);
        let mut song = Song { seq, left: vec![0.0; 1024], right: vec![0.0; 1024], at: 2048 };
        let loud = (0..2 * RATE as usize).filter_map(|_| song.next()).map(f32::abs).fold(0.0, f32::max);
        assert!(loud > 0.01, "silent: {loud}");
    }
}
