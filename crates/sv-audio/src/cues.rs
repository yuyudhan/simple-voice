// FilePath: crates/sv-audio/src/cues.rs
//! Start/stop/error cue sounds, synthesized at runtime so the app ships no audio files.
//!
//! Every cue is 44.1 kHz mono, peak-normalized to -1 dBFS, and starts and ends on an envelope
//! value of exactly zero so it never clicks. Playback goes through a rodio mixer owned by a
//! dedicated thread (the underlying output stream is not `Send`), fed by a channel so `play`
//! returns immediately on the dictation hot path.

use std::f32::consts::PI;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::mpsc;
use std::thread;

use sv_domain::{AppError, AppResult, SoundTheme};

const RATE: u32 = 44_100;
const PEAK: f32 = 0.89;
const RODIO_RATE: NonZeroU32 = match NonZeroU32::new(RATE) {
    Some(rate) => rate,
    None => NonZeroU32::MIN,
};
const MONO: NonZeroU16 = NonZeroU16::MIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cue {
    Start,
    Stop,
    Error,
}

#[derive(Debug)]
struct Request {
    theme: SoundTheme,
    cue: Cue,
    volume: f32,
}

/// Plays cues on the default output device.
#[derive(Debug)]
pub struct CuePlayer {
    requests: mpsc::Sender<Request>,
}

impl CuePlayer {
    /// Starts the playback thread. The output device is opened there, and reopened on the next
    /// cue if it was missing, so a Mac that boots with no output still gets cues later.
    pub fn new() -> AppResult<CuePlayer> {
        let (requests, inbox) = mpsc::channel::<Request>();
        thread::Builder::new()
            .name("sv-audio-cues".to_owned())
            .spawn(move || run_player(&inbox))
            .map_err(|error| AppError::Audio(format!("Could not start the cue player: {error}")))?;
        Ok(CuePlayer { requests })
    }

    /// Queues a cue; never blocks. `volume` is clamped to 0..1.
    pub fn play(&self, theme: SoundTheme, cue: Cue, volume: f32) {
        let volume = if volume.is_nan() {
            0.0
        } else {
            volume.clamp(0.0, 1.0)
        };
        if volume <= 0.0 {
            return;
        }
        if self.requests.send(Request { theme, cue, volume }).is_err() {
            tracing::warn!(?cue, "cue player thread has stopped; cue skipped");
        }
    }
}

fn run_player(inbox: &mpsc::Receiver<Request>) {
    let mut sink = open_sink();
    let mut cache: Vec<((SoundTheme, Cue), Vec<f32>)> = Vec::new();
    while let Ok(request) = inbox.recv() {
        if sink.is_none() {
            sink = open_sink();
        }
        let Some(output) = sink.as_ref() else {
            continue;
        };
        let key = (request.theme, request.cue);
        let index = match cache.iter().position(|(cached, _)| *cached == key) {
            Some(index) => index,
            None => {
                cache.push((key, synthesize(request.theme, request.cue)));
                cache.len() - 1
            }
        };
        let Some((_, samples)) = cache.get(index) else {
            continue;
        };
        let scaled: Vec<f32> = samples
            .iter()
            .map(|sample| sample * request.volume)
            .collect();
        output
            .mixer()
            .add(rodio::buffer::SamplesBuffer::new(MONO, RODIO_RATE, scaled));
    }
}

fn open_sink() -> Option<rodio::MixerDeviceSink> {
    match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(mut sink) => {
            // The sink is only dropped when the app quits, which is not worth a log line.
            sink.log_on_drop(false);
            Some(sink)
        }
        Err(error) => {
            tracing::warn!(%error, "no audio output for cues");
            None
        }
    }
}

/// Renders a cue at 44.1 kHz mono.
pub(crate) fn synthesize(theme: SoundTheme, cue: Cue) -> Vec<f32> {
    let mut samples = match (theme, cue) {
        (_, Cue::Error) => error_blips(),
        (SoundTheme::Soft, Cue::Start) => soft(784.0, 1_175.0),
        (SoundTheme::Soft, Cue::Stop) => soft(1_175.0, 784.0),
        (SoundTheme::Glass, Cue::Start) => glass(1_318.5),
        (SoundTheme::Glass, Cue::Stop) => glass(987.8),
        (SoundTheme::Pop, Cue::Start) => pop(380.0, 1_100.0),
        (SoundTheme::Pop, Cue::Stop) => pop(1_100.0, 380.0),
        (SoundTheme::Chime, Cue::Start) => chime(&[1_046.5, 1_318.5, 1_568.0]),
        (SoundTheme::Chime, Cue::Stop) => chime(&[1_568.0, 1_318.5, 1_046.5]),
    };
    normalize(&mut samples);
    samples
}

fn frames(seconds: f32) -> usize {
    (seconds * RATE as f32).round() as usize
}

fn time(index: usize) -> f32 {
    index as f32 / RATE as f32
}

/// Half-sine fade in over `fade_in` frames and out over `fade_out` frames (sox `fade h`); the
/// first and last frames are exactly zero.
fn fades(index: usize, len: usize, fade_in: usize, fade_out: usize) -> f32 {
    let mut gain = 1.0;
    if fade_in > 0 && index < fade_in {
        gain *= (PI / 2.0 * index as f32 / fade_in as f32).sin();
    }
    let remaining = len.saturating_sub(1 + index);
    if fade_out > 0 && remaining < fade_out {
        gain *= (PI / 2.0 * remaining as f32 / fade_out as f32).sin();
    }
    gain
}

/// The original dictation cue: two short sine blips a fifth apart.
fn soft(first: f32, second: f32) -> Vec<f32> {
    let mut out = blip(first, 0.07, 0.005, 0.04, 0.0);
    out.extend(blip(second, 0.09, 0.005, 0.06, 0.0));
    out
}

/// A sine blip with an optional second harmonic for small laptop speakers.
fn blip(freq: f32, seconds: f32, fade_in: f32, fade_out: f32, harmonic: f32) -> Vec<f32> {
    let len = frames(seconds);
    let (fade_in, fade_out) = (frames(fade_in), frames(fade_out));
    (0..len)
        .map(|index| {
            let phase = 2.0 * PI * freq * time(index);
            let tone = phase.sin() + harmonic * (2.0 * phase).sin();
            tone * fades(index, len, fade_in, fade_out)
        })
        .collect()
}

/// A struck glass: inharmonic partials, the higher ones dying away faster.
fn glass(fundamental: f32) -> Vec<f32> {
    const PARTIALS: [(f32, f32, f32); 4] = [
        (1.0, 1.0, 7.0),
        (2.32, 0.45, 11.0),
        (4.25, 0.22, 16.0),
        (6.63, 0.1, 23.0),
    ];
    let len = frames(0.45);
    let (fade_in, fade_out) = (frames(0.003), frames(0.03));
    (0..len)
        .map(|index| {
            let t = time(index);
            let tone: f32 = PARTIALS
                .iter()
                .map(|(ratio, amp, decay)| {
                    amp * (2.0 * PI * fundamental * ratio * t).sin() * (-decay * t).exp()
                })
                .sum();
            tone * fades(index, len, fade_in, fade_out)
        })
        .collect()
}

/// A bubble pop: ~70 ms sine with an exponential pitch sweep.
fn pop(from: f32, to: f32) -> Vec<f32> {
    let seconds = 0.07;
    let len = frames(seconds);
    let fade_in = frames(0.004);
    let mut phase = 0.0f32;
    (0..len)
        .map(|index| {
            let progress = index as f32 / len as f32;
            let freq = from * (to / from).powf(progress);
            let value = phase.sin();
            phase = (phase + 2.0 * PI * freq / RATE as f32) % (2.0 * PI);
            let remaining = len.saturating_sub(1 + index) as f32 / len as f32;
            value * fades(index, len, fade_in, 0) * remaining.powf(1.5)
        })
        .collect()
}

/// Three notes of a major triad, each ringing on under the next.
fn chime(notes: &[f32]) -> Vec<f32> {
    let stagger = frames(0.075);
    let note_len = frames(0.3);
    let mut out = vec![0.0f32; stagger * notes.len().saturating_sub(1) + note_len];
    let (fade_in, fade_out) = (frames(0.005), frames(0.04));
    for (position, freq) in notes.iter().enumerate() {
        let start = position * stagger;
        for index in 0..note_len {
            let t = time(index);
            let phase = 2.0 * PI * freq * t;
            let tone = (phase.sin() + 0.15 * (2.0 * phase).sin()) * (-9.0 * t).exp();
            if let Some(slot) = out.get_mut(start + index) {
                *slot += tone * fades(index, note_len, fade_in, fade_out);
            }
        }
    }
    out
}

/// Two low blips: the same for every theme so an error always sounds like an error.
fn error_blips() -> Vec<f32> {
    let mut out = blip(262.0, 0.09, 0.008, 0.05, 0.3);
    out.extend(std::iter::repeat_n(0.0, frames(0.05)));
    out.extend(blip(262.0, 0.09, 0.008, 0.05, 0.3));
    out
}

fn normalize(samples: &mut [f32]) {
    let peak = samples
        .iter()
        .fold(0.0f32, |max, sample| max.max(sample.abs()));
    if peak > 0.0 {
        let gain = PEAK / peak;
        samples.iter_mut().for_each(|sample| *sample *= gain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEMES: [SoundTheme; 4] = [
        SoundTheme::Soft,
        SoundTheme::Glass,
        SoundTheme::Pop,
        SoundTheme::Chime,
    ];
    const CUES: [Cue; 3] = [Cue::Start, Cue::Stop, Cue::Error];

    #[test]
    fn every_cue_is_audible_normalized_and_clickless() {
        for theme in THEMES {
            for cue in CUES {
                let samples = synthesize(theme, cue);
                assert!(samples.len() > frames(0.05), "{theme:?} {cue:?} too short");
                assert!(samples.len() < frames(0.6), "{theme:?} {cue:?} too long");
                let peak = samples.iter().fold(0.0f32, |max, s| max.max(s.abs()));
                assert!(peak <= 1.0 && peak > 0.8, "{theme:?} {cue:?} peak {peak}");
                let first = samples.first().copied().unwrap_or(1.0).abs();
                let last = samples.last().copied().unwrap_or(1.0).abs();
                assert!(
                    first < 1e-3 && last < 1e-3,
                    "{theme:?} {cue:?} {first} {last}"
                );
                // A discontinuity shows up as a near full-scale jump; the brightest partials
                // (glass, ~8.7 kHz) stay well below this.
                let max_step = samples
                    .windows(2)
                    .fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
                assert!(max_step < 0.75, "{theme:?} {cue:?} step {max_step}");
            }
        }
    }

    #[test]
    fn soft_cue_matches_the_original_timing() {
        let start = synthesize(SoundTheme::Soft, Cue::Start);
        assert_eq!(start.len(), frames(0.07) + frames(0.09));
    }

    #[test]
    fn start_and_stop_differ() {
        for theme in THEMES {
            assert_ne!(
                synthesize(theme, Cue::Start),
                synthesize(theme, Cue::Stop),
                "{theme:?}"
            );
        }
    }

    #[test]
    fn error_cue_is_shared_across_themes() {
        assert_eq!(
            synthesize(SoundTheme::Soft, Cue::Error),
            synthesize(SoundTheme::Chime, Cue::Error)
        );
    }
}
