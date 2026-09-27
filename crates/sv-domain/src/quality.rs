// FilePath: crates/sv-domain/src/quality.rs
//! How sure a transcription model was of its result. Smart Select compares these numbers with
//! per-model floors to decide whether a second model should double-check the dictation.

use serde::{Deserialize, Serialize};

/// Whisper models (Groq and local) report the first three; Parakeet reports `confidence`.
/// Every field is absent when the model reports nothing.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptQuality {
    /// Token-weighted mean of the segments' average log probability.
    pub avg_logprob: Option<f32>,
    /// Highest segment compression ratio; high values mean repetitive (hallucinated) text.
    pub compression_ratio: Option<f32>,
    /// Token-weighted mean probability that a segment holds no speech.
    pub no_speech_prob: Option<f32>,
    /// Parakeet's overall confidence, 0.1..=1.0.
    pub confidence: Option<f32>,
}
