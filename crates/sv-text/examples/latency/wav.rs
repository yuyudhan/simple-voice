// FilePath: crates/sv-text/examples/latency/wav.rs
//! 16 kHz mono PCM16 WAV files, the format of the eval recordings and of the app's own audio.

use std::path::Path;

pub(crate) const SAMPLE_RATE: u32 = 16_000;

/// The samples of a 16 kHz mono 16-bit PCM WAV file.
pub(crate) fn read(path: &Path) -> Result<Vec<i16>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let invalid = || format!("{}: not a 16 kHz mono PCM16 WAV file", path.display());
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(invalid());
    }
    let mut at = 12;
    let mut format_ok = false;
    while let Some(header) = bytes.get(at..at + 8) {
        let (id, size) = header.split_at(4);
        let size = usize::try_from(u32::from_le_bytes([size[0], size[1], size[2], size[3]]))
            .map_err(|_| invalid())?;
        let body = bytes.get(at + 8..at + 8 + size).ok_or_else(invalid)?;
        match id {
            b"fmt " => {
                let field = |from: usize| body.get(from..from + 2).map(|b| [b[0], b[1]]);
                let channels = field(2).map(u16::from_le_bytes);
                let bits = field(14).map(u16::from_le_bytes);
                let rate = body
                    .get(4..8)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                format_ok = channels == Some(1) && bits == Some(16) && rate == Some(SAMPLE_RATE);
            }
            b"data" if format_ok => {
                return Ok(body
                    .chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                    .collect());
            }
            _ => {}
        }
        // Chunks are padded to an even length.
        at += 8 + size + size % 2;
    }
    Err(invalid())
}

/// Writes `samples` as a 16 kHz mono PCM16 WAV file.
pub(crate) fn write(path: &Path, samples: &[i16]) -> Result<(), String> {
    let data_len = u32::try_from(samples.len() * 2).map_err(|_| "recording too long")?;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}
