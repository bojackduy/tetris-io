//! Cached, nonblocking gameplay audio. Asset credits are in `assets/audio/README.md`.

use std::f32::consts::TAU;
use std::io::Cursor;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rodio::buffer::SamplesBuffer;
use rodio::source::Amplify;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Source};

const SAMPLE_RATE: NonZeroU32 = NonZeroU32::new(44_100).unwrap();
// Verified from shipped Jstris replayer.js (RainforestSFXset) and game.js (loadSounds):
// comboTones = { duration: 1000 ms, spacing: 500 ms, cnt: 15 }, startTime = i * 1500 ms.
const COMBO_STEPS: usize = 15;
const COMBO_SLICE_DURATION_MS: usize = 1000;
const COMBO_SLICE_STRIDE_MS: usize = 1500;
const LOCK: &[u8] = include_bytes!("../../assets/audio/lock.mp3");
const TOP_OUT: &[u8] = include_bytes!("../../assets/audio/topout.mp3");
const COMBO_TONES: &[u8] = include_bytes!("../../assets/audio/comboTones.mp3");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCue {
    Start,
    Move,
    Rotate,
    Hold,
    HardDrop,
    Lock,
    Clear(u8),
    Combo(u32),
    PerfectClear,
    GameOver,
}

/// Retains the output device and predecoded samples for the game's lifetime.
///
/// Construct this before entering the terminal's alternate screen. Missing or
/// failed audio output is silent; callers can show `available()` in settings.
pub struct Audio {
    output: Option<MixerDeviceSink>,
    stream_failed: Arc<AtomicBool>,
    sounds: SoundBank,
}

impl Audio {
    pub fn new() -> Self {
        let sounds = SoundBank::new();
        let stream_failed = Arc::new(AtomicBool::new(false));
        let failed = Arc::clone(&stream_failed);
        // open_default_sink() installs stderr logging. Use the builder instead
        // so device errors and shutdown cannot corrupt the terminal display.
        let output = DeviceSinkBuilder::from_default_device()
            .ok()
            .and_then(|builder| {
                builder
                    .with_error_callback(move |_| failed.store(true, Ordering::Relaxed))
                    .open_sink_or_fallback()
                    .ok()
            })
            .map(|mut output| {
                output.log_on_drop(false);
                output
            });
        Self {
            output,
            stream_failed,
            sounds,
        }
    }

    pub fn available(&self) -> bool {
        self.output.is_some() && !self.stream_failed.load(Ordering::Relaxed)
    }

    /// Mix a successful gameplay event immediately, without decoding or file I/O.
    ///
    /// Volume is a percentage: zero mutes; values above 100 clamp to 100.
    /// Clear(0) and Combo(0) are silent. Rotation/start preferences and failed
    /// action filtering belong to the runtime, not this adapter.
    pub fn play(&self, cue: SoundCue, volume: u8) {
        if !self.available() {
            return;
        }
        if let (Some(output), Some(source)) = (&self.output, self.sounds.source(cue, volume)) {
            output.mixer().add(source);
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

struct SoundBank {
    start: SamplesBuffer,
    move_piece: SamplesBuffer,
    rotate: SamplesBuffer,
    hold: SamplesBuffer,
    hard_drop: SamplesBuffer,
    lock: SamplesBuffer,
    clears: [SamplesBuffer; 4],
    combos: [SamplesBuffer; COMBO_STEPS],
    perfect_clear: SamplesBuffer,
    game_over: SamplesBuffer,
}

impl SoundBank {
    fn new() -> Self {
        let lock = decode(LOCK, 0.36).unwrap_or_else(|| chime(&[110.0], 0.08, 0.24));
        let combos = decode_sprite(
            COMBO_TONES,
            COMBO_STEPS,
            COMBO_SLICE_STRIDE_MS,
            COMBO_SLICE_DURATION_MS,
            0.38,
        )
        .unwrap_or_else(|| {
            std::array::from_fn(|index| {
                let frequency = 392.0 * 2.0_f32.powf(index as f32 / 12.0);
                chime(&[frequency, frequency * 1.5], 0.075, 0.25)
            })
        });
        Self {
            start: chime(&[523.25, 659.25, 783.99], 0.075, 0.25),
            move_piece: chime(&[300.0], 0.018, 0.08),
            rotate: chime(&[680.0], 0.035, 0.11),
            hold: chime(&[523.25, 783.99], 0.035, 0.16),
            // Jstris RainforestSFXset leaves harddrop null, which loadSounds aliases to lock.mp3.
            hard_drop: lock.clone(),
            lock,
            clears: std::array::from_fn(|index| combos[index.min(COMBO_STEPS - 1)].clone()),
            combos,
            perfect_clear: chime(&[523.25, 659.25, 783.99, 1046.5], 0.08, 0.34),
            game_over: decode(TOP_OUT, 0.45)
                .unwrap_or_else(|| chime(&[392.0, 293.66, 196.0], 0.12, 0.30)),
        }
    }

    fn source(&self, cue: SoundCue, volume: u8) -> Option<Amplify<SamplesBuffer>> {
        if volume == 0 {
            return None;
        }
        let sound = match cue {
            SoundCue::Start => &self.start,
            SoundCue::Move => &self.move_piece,
            SoundCue::Rotate => &self.rotate,
            SoundCue::Hold => &self.hold,
            SoundCue::HardDrop => &self.hard_drop,
            SoundCue::Lock => &self.lock,
            SoundCue::Clear(0) | SoundCue::Combo(0) => return None,
            SoundCue::Clear(lines) => &self.clears[usize::from(lines.min(4) - 1)],
            SoundCue::Combo(combo) => {
                &self.combos[(combo - 1).min(COMBO_STEPS as u32 - 1) as usize]
            }
            SoundCue::PerfectClear => &self.perfect_clear,
            SoundCue::GameOver => &self.game_over,
        };
        // SamplesBuffer clones share their immutable PCM allocation.
        Some(sound.clone().amplify(f32::from(volume.min(100)) / 100.0))
    }
}

fn decode(bytes: &'static [u8], peak: f32) -> Option<SamplesBuffer> {
    let decoder = Decoder::try_from(Cursor::new(bytes)).ok()?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let mut samples: Vec<f32> = decoder.collect();
    if samples.is_empty() || samples.iter().any(|sample| !sample.is_finite()) {
        return None;
    }
    let maximum = samples
        .iter()
        .fold(0.0_f32, |max, sample| max.max(sample.abs()));
    if maximum == 0.0 {
        return None;
    }
    // Preserve the downloaded files verbatim; scale only the cached playback
    // samples, leaving headroom when a lock, clear and combo overlap.
    let gain = peak / maximum;
    for sample in &mut samples {
        *sample *= gain;
    }
    Some(SamplesBuffer::new(channels, sample_rate, samples))
}

fn decode_sprite<const N: usize>(
    bytes: &'static [u8],
    count: usize,
    stride_ms: usize,
    duration_ms: usize,
    peak: f32,
) -> Option<[SamplesBuffer; N]> {
    if count != N {
        return None;
    }
    let decoder = Decoder::try_from(Cursor::new(bytes)).ok()?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let mut all: Vec<f32> = decoder.collect();
    if all.is_empty() || all.iter().any(|sample| !sample.is_finite()) {
        return None;
    }
    let maximum = all
        .iter()
        .fold(0.0_f32, |max, sample| max.max(sample.abs()));
    if maximum == 0.0 {
        return None;
    }
    let gain = peak / maximum;
    for sample in &mut all {
        *sample *= gain;
    }

    let ch = usize::from(channels.get());
    let sr = sample_rate.get() as usize;
    let mut slices: Vec<SamplesBuffer> = Vec::with_capacity(N);
    for idx in 0..N {
        let start_frame = idx * stride_ms * sr / 1000;
        let len_frames = duration_ms * sr / 1000;
        let start = (start_frame * ch).min(all.len());
        let end = ((start_frame + len_frames) * ch).min(all.len());
        if end <= start {
            return None;
        }
        let mut slice = all[start..end].to_vec();
        // Smooth 3ms fade-out at slice end to avoid clicks at sprite boundaries.
        let fade_frames = (sr * 3 / 1000).min(slice.len() / ch);
        let total_frames = slice.len() / ch;
        for f in 0..fade_frames {
            let factor = (fade_frames - f) as f32 / fade_frames as f32;
            let frame_idx = total_frames - fade_frames + f;
            for c in 0..ch {
                slice[frame_idx * ch + c] *= factor;
            }
        }
        slices.push(SamplesBuffer::new(channels, sample_rate, slice));
    }
    slices.try_into().ok()
}

/// Original short, decaying arpeggios, not transcriptions of Jstris assets.
fn chime(notes: &[f32], note_seconds: f32, peak: f32) -> SamplesBuffer {
    let note_samples = (SAMPLE_RATE.get() as f32 * note_seconds) as usize;
    let mut samples = Vec::with_capacity(note_samples * notes.len());
    for frequency in notes {
        for frame in 0..note_samples {
            let time = frame as f32 / SAMPLE_RATE.get() as f32;
            let progress = frame as f32 / note_samples as f32;
            let attack = (time / 0.002).min(1.0);
            let release =
                ((note_samples - 1 - frame) as f32 / (SAMPLE_RATE.get() as f32 * 0.008)).min(1.0);
            let phase = TAU * frequency * time;
            let wave = (phase.sin() + 0.25 * (phase * 2.0).sin()) / 1.25;
            samples.push(wave * peak * attack * release * (-5.0 * progress).exp());
        }
    }
    SamplesBuffer::new(std::num::NonZeroU16::MIN, SAMPLE_RATE, samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn cues() -> Vec<SoundCue> {
        let mut cues = vec![
            SoundCue::Start,
            SoundCue::Move,
            SoundCue::Rotate,
            SoundCue::Hold,
            SoundCue::HardDrop,
            SoundCue::Lock,
            SoundCue::PerfectClear,
            SoundCue::GameOver,
        ];
        cues.extend((1..=4).map(SoundCue::Clear));
        cues.extend((1..=COMBO_STEPS as u32).map(SoundCue::Combo));
        cues
    }

    #[test]
    fn bundled_rainforest_assets_decode_as_stereo_mp3() {
        for (bytes, minimum, maximum) in [
            (LOCK, Duration::from_millis(600), Duration::from_millis(800)),
            (TOP_OUT, Duration::from_secs(1), Duration::from_millis(1200)),
            (
                COMBO_TONES,
                Duration::from_secs(21),
                Duration::from_millis(22_500),
            ),
        ] {
            let sound = decode(bytes, 0.45).expect("bundled asset decodes");
            assert_eq!(sound.channels().get(), 2);
            assert_eq!(sound.sample_rate(), SAMPLE_RATE);
            assert!((minimum..=maximum).contains(&sound.total_duration().unwrap()));
            assert!(sound.len() > 1000);
        }
    }

    #[test]
    fn rainforest_combo_sprite_slices_all_fifteen_tones() {
        let slices = decode_sprite::<COMBO_STEPS>(
            COMBO_TONES,
            COMBO_STEPS,
            COMBO_SLICE_STRIDE_MS,
            COMBO_SLICE_DURATION_MS,
            0.38,
        )
        .expect("combo sprite slices cleanly");
        for (i, slice) in slices.iter().enumerate() {
            let dur = slice.total_duration().unwrap();
            assert!(
                (Duration::from_millis(850)..=Duration::from_millis(1005)).contains(&dur),
                "combo tone {i} duration {dur:?}"
            );
            let samples: Vec<f32> = slice.clone().collect();
            assert!(
                samples.iter().any(|s| s.abs() > 0.01),
                "combo tone {i} must be audible"
            );
        }
    }

    #[test]
    fn every_cue_is_audible_finite_normalized_and_short() {
        let sounds = SoundBank::new();
        for cue in cues() {
            let source = sounds.source(cue, 100).unwrap();
            let duration = source.total_duration().unwrap();
            assert!(
                (Duration::from_millis(10)..=Duration::from_secs(2)).contains(&duration),
                "{cue:?}: {duration:?}"
            );
            let samples: Vec<f32> = source.collect();
            assert!(!samples.is_empty(), "{cue:?}");
            assert!(
                samples
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 0.46),
                "{cue:?}"
            );
            assert!(samples.iter().any(|sample| sample.abs() > 0.01), "{cue:?}");
        }
    }

    #[test]
    fn zero_volume_and_empty_clear_or_combo_are_silent() {
        let sounds = SoundBank::new();
        for cue in cues() {
            assert!(sounds.source(cue, 0).is_none(), "{cue:?}");
        }
        assert!(sounds.source(SoundCue::Clear(0), 60).is_none());
        assert!(sounds.source(SoundCue::Combo(0), 60).is_none());
    }

    #[test]
    fn volume_is_linear_and_clamped_to_a_percentage() {
        let sounds = SoundBank::new();
        let samples = |volume| {
            sounds
                .source(SoundCue::Lock, volume)
                .unwrap()
                .collect::<Vec<_>>()
        };
        let full = samples(100);
        assert_eq!(samples(u8::MAX), full);
        for (quiet, full) in samples(60).into_iter().zip(full) {
            assert!((quiet - full * 0.6).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn out_of_range_counts_use_last_cached_sound() {
        let sounds = SoundBank::new();
        for (maximum, overflow) in [
            (SoundCue::Clear(4), SoundCue::Clear(u8::MAX)),
            (
                SoundCue::Combo(COMBO_STEPS as u32),
                SoundCue::Combo(u32::MAX),
            ),
        ] {
            assert_eq!(
                sounds.source(maximum, 60).unwrap().collect::<Vec<_>>(),
                sounds.source(overflow, 60).unwrap().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn disabled_or_failed_output_does_not_panic() {
        let audio = Audio {
            output: None,
            stream_failed: Arc::new(AtomicBool::new(false)),
            sounds: SoundBank::new(),
        };
        assert!(!audio.available());
        for cue in cues() {
            audio.play(cue, 60);
            audio.play(cue, 0);
        }
        audio.stream_failed.store(true, Ordering::Relaxed);
        assert!(!audio.available());
        audio.play(SoundCue::GameOver, u8::MAX);
    }

    #[test]
    fn invalid_audio_falls_back_without_panicking() {
        assert!(decode(b"not audio", 0.4).is_none());
        assert!(decode(b"", 0.4).is_none());
    }

    #[test]
    fn synthesized_notes_start_and_end_without_a_step() {
        let sounds = SoundBank::new();
        for cue in [
            SoundCue::Start,
            SoundCue::Move,
            SoundCue::Rotate,
            SoundCue::Hold,
            SoundCue::PerfectClear,
        ] {
            let samples: Vec<_> = sounds.source(cue, 100).unwrap().collect();
            assert_eq!(samples.first(), Some(&0.0));
            assert_eq!(samples.last(), Some(&0.0));
        }
    }
}
