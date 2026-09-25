// FilePath: crates/sv-audio/src/wav.rs
//! 16 kHz mono PCM16 WAV, the format kept on disk for retries and sent to every engine.

use sv_domain::{AppError, AppResult};

use crate::TARGET_SAMPLE_RATE;

const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
const BLOCK_ALIGN: u16 = CHANNELS * BITS_PER_SAMPLE / 8;
const FORMAT_PCM: u16 = 1;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

pub fn encode_wav(samples: &[i16]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX - 36);
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&FORMAT_PCM.to_le_bytes());
    bytes.extend_from_slice(&CHANNELS.to_le_bytes());
    bytes.extend_from_slice(&TARGET_SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(TARGET_SAMPLE_RATE * u32::from(BLOCK_ALIGN)).to_le_bytes());
    bytes.extend_from_slice(&BLOCK_ALIGN.to_le_bytes());
    bytes.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Reads a WAV written by [`encode_wav`] (or any 16 kHz mono PCM16 WAV, skipping extra chunks).
pub fn decode_wav(bytes: &[u8]) -> AppResult<Vec<i16>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid("it is not a WAV file"));
    }
    let mut format_ok = false;
    let mut offset: usize = 12;
    while let Some(header) = offset.checked_add(8).and_then(|end| bytes.get(offset..end)) {
        let id = &header[0..4];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let body_start = offset + 8;
        let body_end = body_start.saturating_add(size);
        match id {
            b"fmt " => {
                let body = bytes
                    .get(body_start..body_end)
                    .ok_or_else(|| invalid("its format chunk is truncated"))?;
                check_format(body)?;
                format_ok = true;
            }
            b"data" => {
                if !format_ok {
                    return Err(invalid("the data chunk comes before the format chunk"));
                }
                // Tolerate a data size larger than the file: some writers leave it unset when a
                // recording is interrupted, and the samples present are still valid.
                let end = body_end.min(bytes.len());
                let body = bytes.get(body_start..end).unwrap_or_default();
                return Ok(body
                    .chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                    .collect());
            }
            _ => {}
        }
        // Chunks are word-aligned: odd sizes carry one pad byte.
        offset = body_end.saturating_add(size & 1);
    }
    Err(invalid("it has no audio data"))
}

fn check_format(body: &[u8]) -> AppResult<()> {
    if body.len() < 16 {
        return Err(invalid("its format chunk is truncated"));
    }
    let read_u16 = |at: usize| u16::from_le_bytes([body[at], body[at + 1]]);
    let format = read_u16(0);
    let channels = read_u16(2);
    let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
    let bits = read_u16(14);
    if format != FORMAT_PCM && format != FORMAT_EXTENSIBLE {
        return Err(invalid("it is not PCM audio"));
    }
    if channels != CHANNELS || rate != TARGET_SAMPLE_RATE || bits != BITS_PER_SAMPLE {
        return Err(invalid(&format!(
            "it is {rate} Hz, {channels} channel(s), {bits}-bit instead of 16 kHz mono 16-bit"
        )));
    }
    Ok(())
}

fn invalid(reason: &str) -> AppError {
    AppError::Audio(format!("The saved recording cannot be read: {reason}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_samples() {
        let samples: Vec<i16> = vec![0, 1, -1, i16::MAX, i16::MIN, 12_345, -23_456];
        let bytes = encode_wav(&samples);
        assert_eq!(bytes.len(), 44 + samples.len() * 2);
        assert_eq!(decode_wav(&bytes).unwrap(), samples);
    }

    #[test]
    fn round_trips_empty_recording() {
        assert!(decode_wav(&encode_wav(&[])).unwrap().is_empty());
    }

    #[test]
    fn skips_unknown_chunks_before_data() {
        let mut bytes = encode_wav(&[7, -7]);
        let mut list = Vec::new();
        list.extend_from_slice(b"LIST");
        list.extend_from_slice(&3u32.to_le_bytes());
        list.extend_from_slice(&[1, 2, 3, 0]);
        let data_at = 36;
        bytes.splice(data_at..data_at, list);
        assert_eq!(decode_wav(&bytes).unwrap(), vec![7, -7]);
    }

    #[test]
    fn rejects_bad_headers() {
        assert!(decode_wav(b"").is_err());
        assert!(decode_wav(b"RIFX\0\0\0\0WAVE").is_err());

        let mut wrong_rate = encode_wav(&[1, 2]);
        wrong_rate[24..28].copy_from_slice(&44_100u32.to_le_bytes());
        assert!(decode_wav(&wrong_rate).is_err());

        let mut stereo = encode_wav(&[1, 2]);
        stereo[22..24].copy_from_slice(&2u16.to_le_bytes());
        assert!(decode_wav(&stereo).is_err());

        let mut float = encode_wav(&[1, 2]);
        float[20..22].copy_from_slice(&3u16.to_le_bytes());
        assert!(decode_wav(&float).is_err());

        let header_only = &encode_wav(&[1, 2])[..36];
        assert!(decode_wav(header_only).is_err());
    }

    #[test]
    fn rejection_message_is_audio_error() {
        let result = decode_wav(b"nonsense");
        assert!(
            matches!(&result, Err(AppError::Audio(message)) if message.contains("not a WAV")),
            "unexpected {result:?}"
        );
    }
}
