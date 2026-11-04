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

pub struct Audio {
    sink: MixerDeviceSink,
    font: Option<Arc<SoundFont>>,
    font_tried: bool,
    music: Option<Player>,
    track: Option<&'static str>,
    pub music_on: bool,
    pub sound_on: bool,
}

/// A looping MIDI song rendered a block at a time.
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
        Some(s)
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
        Some(Self { sink, font: None, font_tried: false, music: None, track: None, music_on: true, sound_on: true })
    }

    /// Plays a song from the imported data on a loop; the same song keeps playing.
    pub fn music(&mut self, dir: &Path, track: &'static str) {
        if !self.music_on {
            return self.stop_music();
        }
        if self.track == Some(track) {
            return;
        }
        self.stop_music();
        // The web build's SoundFont arrives after start-up, so it keeps looking until then.
        if !self.font_tried {
            self.font = font_path().and_then(|p| SoundFont::new(&mut Cursor::new(vfs::read(p).ok()?)).ok()).map(Arc::new);
            self.font_tried = self.font.is_some() || cfg!(not(target_arch = "wasm32"));
        }
        let (Some(font), Ok(b)) = (&self.font, vfs::read(dir.join(track))) else { return };
        let Ok(midi) = MidiFile::new(&mut Cursor::new(b)) else { return };
        let Ok(synth) = Synthesizer::new(font, &SynthesizerSettings::new(RATE as i32)) else { return };
        let mut seq = MidiFileSequencer::new(synth);
        seq.play(&Arc::new(midi), true);
        let player = Player::connect_new(self.sink.mixer());
        player.set_volume(0.45);
        player.append(Song { seq, left: vec![0.0; 1024], right: vec![0.0; 1024], at: 2048 });
        self.music = Some(player);
        self.track = Some(track);
    }

    pub fn stop_music(&mut self) {
        if let Some(p) = self.music.take() {
            p.stop();
        }
        self.track = None;
    }

    pub fn effect(&self, dir: &Path, wav: &str) {
        if !self.sound_on {
            return;
        }
        if let Ok(b) = vfs::read(dir.join(wav))
            && let Ok(d) = Decoder::try_from(Cursor::new(b))
        {
            self.sink.mixer().add(d.amplify(0.8));
        }
    }
}

/// The original's sound for a log line, if it has one.
pub fn effect_for(line: &str) -> Option<&'static str> {
    let l = line.to_lowercase();
    let table: [(&str, &str); 28] = [
        ("critically hits", "SLICE.WAV"),
        ("and kills it", "KILL.WAV"),
        ("has died", "DIE.WAV"),
        ("breathes fire", "FIRE.WAV"),
        ("breathes frost", "COLD.WAV"),
        ("breathes lightning", "SHOCK.WAV"),
        ("is poisoned", "POISON.WAV"),
        ("is diseased", "DISEASE.WAV"),
        ("is paralysed", "PARAL.WAV"),
        ("turned to stone", "STONE.WAV"),
        ("drained", "DRAIN.WAV"),
        ("ages", "WITHER.WAV"),
        ("steals", "STEAL.WAV"),
        ("destroys", "DSTSLAM.WAV"),
        ("acid burns", "SPIT.WAV"),
        ("teleport", "TELEPORT.WAV"),
        ("falls into a pit", "FALL.WAV"),
        ("chute", "FALL.WAV"),
        ("water", "WADE.WAV"),
        ("welcome to level", "MAKELEV.WAV"),
        ("is raised", "RAISE.WAV"),
        ("lives again", "RAISE.WAV"),
        ("sets off", "EXPLOS.WAV"),
        ("disarms", "OPEN.WAV"),
        ("finds", "FOUND.WAV"),
        ("magic fades", "EXTING.WAV"),
        (" casts ", "MAGIC.WAV"),
        (" heals ", "MAGIC2.WAV"),
    ];
    if let Some((_, w)) = table.iter().find(|(k, _)| l.contains(k)) {
        return Some(w);
    }
    if l.contains(" misses ") {
        Some("SWING.WAV")
    } else if l.starts_with("the ") && l.contains(" hits ") {
        Some("OUCH.WAV")
    } else if l.contains(" hits ") {
        Some("HIT.WAV")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dungeon song renders through the SoundFont (when present) and every effect the
    /// log can trigger exists in the import.
    #[test]
    fn music_renders_and_effects_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = root.join("target/ddj-audio-test");
        ddj_extract::import(&root.join("tests/fixtures/MORDOR11.ZIP"), &out).unwrap();
        for line in ["Ann critically hits the Orc", "Bo has died!", "The Orc hits Ann for 3.", "Ann misses the Orc.", "Cy casts Heal.", "Welcome to level 2!"] {
            let w = effect_for(line).unwrap_or_else(|| panic!("{line}"));
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
