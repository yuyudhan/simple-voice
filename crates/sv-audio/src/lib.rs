// FilePath: crates/sv-audio/src/lib.rs
//! Audio for Simple Voice: microphone capture delivered as 16 kHz mono PCM16 (the format every
//! speech engine accepts), the check for a microphone to record from, WAV encoding for kept
//! recordings, a live level meter for the overlay, and synthesized start/stop cues.
#![forbid(unsafe_code)]

mod availability;
mod capture;
mod cue_output;
mod cues;
mod devices;
mod meter;
mod resample;
mod watch;
mod wav;

pub use availability::{check_input, input_state, InputProbe};
pub use capture::{CaptureHooks, ChunkSink, ProblemSink, Recorder, Recording};
pub use cues::{Cue, CuePlayer};
pub use devices::{list_microphones, Microphone};
pub use wav::{
    copy_wav_range, decode_wav, encode_wav, mix_wav_files, samples_to_ms, wav_sample_count,
    WavWriter,
};

/// Sample rate of every recording handed to transcription.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;
