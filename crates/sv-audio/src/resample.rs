// FilePath: crates/sv-audio/src/resample.rs
//! Downmix and sample-rate conversion to the 16 kHz mono stream speech engines expect.
//!
//! The resampler is a streaming band-limited interpolator (windowed sinc). Microphones run at
//! 44.1 or 48 kHz, and a naive decimation or linear interpolation folds everything between 8 and
//! 24 kHz (fans, keyboard clicks, sibilance) back into the speech band as aliasing, which costs
//! recognition accuracy. The kernel is a Blackman-windowed sinc with its cutoff at 92% of the
//! lower Nyquist frequency and 16 zero crossings per side: better than 70 dB of stopband
//! rejection at roughly 100 multiply-adds per output sample, which is negligible at 16 kHz. The
//! kernel is tabulated once with 256 points per input sample and linearly interpolated, so any
//! rational or irrational rate ratio works with the same code path.

use std::f64::consts::PI;

const ZERO_CROSSINGS: f64 = 16.0;
const ROLLOFF: f64 = 0.92;
const TABLE_OVERSAMPLE: usize = 256;

/// Averages interleaved frames into mono, appending to `out` without reallocating once `out`
/// has grown to the callback size.
pub(crate) fn downmix_into<T: Copy>(
    interleaved: &[T],
    channels: usize,
    convert: impl Fn(T) -> f32,
    out: &mut Vec<f32>,
) {
    if channels <= 1 {
        out.extend(interleaved.iter().map(|sample| convert(*sample)));
        return;
    }
    let scale = 1.0 / channels as f32;
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().map(|sample| convert(*sample)).sum::<f32>() * scale),
    );
}

/// Converts a float sample in -1..1 to PCM16 with rounding and clipping.
pub(crate) fn to_i16(sample: f32) -> i16 {
    (sample * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16
}

#[derive(Debug)]
pub(crate) struct Resampler {
    /// Input samples advanced per output sample.
    step: f64,
    /// Kernel half-length in input samples.
    half_width: usize,
    /// Kernel values at distances 0, 1/TABLE_OVERSAMPLE, ... up to `half_width` (then zero).
    table: Vec<f32>,
    /// Input not yet fully consumed, with `half_width` zeros of lead-in so the first output
    /// lines up with the first input sample.
    history: Vec<f32>,
    /// Time of the next output sample, as an index into `history`.
    position: f64,
    passthrough: bool,
    input_rate: u64,
    output_rate: u64,
    input_total: u64,
    output_total: u64,
}

impl Resampler {
    pub(crate) fn new(input_rate: u32, output_rate: u32) -> Self {
        let input = f64::from(input_rate.max(1));
        let output = f64::from(output_rate.max(1));
        let passthrough = input_rate == output_rate;
        let cutoff = (output / input).min(1.0) * ROLLOFF;
        let half_width = if passthrough {
            0
        } else {
            (ZERO_CROSSINGS / cutoff).ceil() as usize
        };
        let table = if passthrough {
            Vec::new()
        } else {
            kernel_table(cutoff, half_width)
        };
        Self {
            step: input / output,
            half_width,
            table,
            history: vec![0.0; half_width],
            position: half_width as f64,
            passthrough,
            input_rate: u64::from(input_rate.max(1)),
            output_rate: u64::from(output_rate.max(1)),
            input_total: 0,
            output_total: 0,
        }
    }

    /// Consumes `input` and appends every output sample it fully determines.
    pub(crate) fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.input_total += input.len() as u64;
        if self.passthrough {
            out.extend_from_slice(input);
            self.output_total += input.len() as u64;
            return;
        }
        self.history.extend_from_slice(input);
        self.produce(u64::MAX, out);
    }

    /// Emits the tail held back for lookahead. The total output length is
    /// `ceil(input_len * output_rate / input_rate)`.
    pub(crate) fn flush(&mut self, out: &mut Vec<f32>) {
        if self.passthrough {
            return;
        }
        let expected = (self.input_total * self.output_rate).div_ceil(self.input_rate);
        self.history
            .extend(std::iter::repeat_n(0.0, self.half_width + 1));
        self.produce(expected, out);
    }

    fn produce(&mut self, limit: u64, out: &mut Vec<f32>) {
        let half = self.half_width;
        while self.output_total < limit {
            let base = self.position.floor() as usize;
            if base + half >= self.history.len() {
                break;
            }
            let frac = self.position - base as f64;
            let first = base + 1 - half;
            let mut acc = 0.0f32;
            for (offset, sample) in self.history[first..=base + half].iter().enumerate() {
                let distance = (frac + (half - 1) as f64 - offset as f64).abs();
                acc += sample * self.kernel(distance);
            }
            out.push(acc);
            self.output_total += 1;
            self.position += self.step;
        }
        let keep_from = (self.position.floor() as usize + 1).saturating_sub(half);
        let drop = keep_from.min(self.history.len());
        if drop > 0 {
            self.history.drain(..drop);
            self.position -= drop as f64;
        }
    }

    fn kernel(&self, distance: f64) -> f32 {
        let scaled = distance * TABLE_OVERSAMPLE as f64;
        let index = scaled.floor() as usize;
        let frac = (scaled - index as f64) as f32;
        match (self.table.get(index), self.table.get(index + 1)) {
            (Some(a), Some(b)) => a + (b - a) * frac,
            (Some(a), None) => *a,
            _ => 0.0,
        }
    }
}

fn kernel_table(cutoff: f64, half_width: usize) -> Vec<f32> {
    let len = half_width * TABLE_OVERSAMPLE + 1;
    (0..len)
        .map(|index| {
            let x = index as f64 / TABLE_OVERSAMPLE as f64;
            let sinc = if x == 0.0 {
                1.0
            } else {
                (PI * cutoff * x).sin() / (PI * cutoff * x)
            };
            let u = x / half_width as f64;
            let window = 0.42 + 0.5 * (PI * u).cos() + 0.08 * (2.0 * PI * u).cos();
            (cutoff * sinc * window) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f64, seconds: f64) -> Vec<f32> {
        let n = (f64::from(rate) * seconds) as usize;
        (0..n)
            .map(|i| (0.5 * (2.0 * PI * freq * i as f64 / f64::from(rate)).sin()) as f32)
            .collect()
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count()
    }

    fn resample_in_chunks(input: &[f32], from: u32, to: u32, chunk: usize) -> Vec<f32> {
        let mut resampler = Resampler::new(from, to);
        let mut out = Vec::new();
        for part in input.chunks(chunk) {
            resampler.process(part, &mut out);
        }
        resampler.flush(&mut out);
        out
    }

    #[test]
    fn keeps_frequency_and_length_ratio_from_48k() {
        let input = sine(48_000, 440.0, 1.0);
        let out = resample_in_chunks(&input, 48_000, 16_000, 480);
        assert_eq!(out.len(), 16_000);
        // 440 Hz for one second crosses zero 880 times.
        let crossings = zero_crossings(&out[200..out.len() - 200]);
        let expected = 880.0 * (out.len() - 400) as f64 / 16_000.0;
        assert!(
            (crossings as f64 - expected).abs() <= 3.0,
            "{crossings} vs {expected}"
        );
    }

    #[test]
    fn handles_non_integer_ratio_from_44k1() {
        let input = sine(44_100, 1_000.0, 0.5);
        let out = resample_in_chunks(&input, 44_100, 16_000, 441);
        assert_eq!(out.len(), 8_000);
        let crossings = zero_crossings(&out[200..out.len() - 200]);
        let expected = 2_000.0 * (out.len() - 400) as f64 / 16_000.0;
        assert!(
            (crossings as f64 - expected).abs() <= 3.0,
            "{crossings} vs {expected}"
        );
    }

    #[test]
    fn preserves_amplitude_in_passband() {
        let input = sine(48_000, 1_000.0, 0.5);
        let out = resample_in_chunks(&input, 48_000, 16_000, 1_024);
        let peak = out[500..out.len() - 500]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn rejects_content_above_the_new_nyquist() {
        // 11 kHz would alias to 5 kHz at 16 kHz if it were not filtered out.
        let input = sine(48_000, 11_000.0, 0.5);
        let out = resample_in_chunks(&input, 48_000, 16_000, 512);
        let peak = out[500..out.len() - 500]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.005, "alias peak {peak}");
    }

    #[test]
    fn chunking_does_not_change_the_output() {
        let input = sine(44_100, 300.0, 0.3);
        let whole = resample_in_chunks(&input, 44_100, 16_000, input.len());
        let pieces = resample_in_chunks(&input, 44_100, 16_000, 97);
        assert_eq!(whole.len(), pieces.len());
        assert!(whole.iter().zip(&pieces).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn same_rate_passes_through() {
        let input = sine(16_000, 440.0, 0.1);
        let out = resample_in_chunks(&input, 16_000, 16_000, 100);
        assert_eq!(out, input);
    }

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_into(&[1.0f32, 0.0, 0.5, 0.5, -1.0, 1.0], 2, |s| s, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
        let mut mono = Vec::new();
        downmix_into(&[100i16, -100], 1, |s| f32::from(s) / 32_768.0, &mut mono);
        assert_eq!(mono.len(), 2);
    }

    #[test]
    fn converts_to_i16_with_clipping() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), 32_767);
        assert_eq!(to_i16(2.0), 32_767);
        assert_eq!(to_i16(-2.0), -32_768);
    }
}
