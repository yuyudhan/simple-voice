// FilePath: crates/sv-audio/src/meter.rs
//! The live level meter: RMS over ~33 ms windows of 16 kHz audio, on a dB scale.

/// 33.3 ms of 16 kHz audio per level reading.
const LEVEL_WINDOW: usize = 533;
/// Quiet room tone sits below this and reads as an empty meter.
const LEVEL_FLOOR_DB: f32 = -55.0;
/// Normal speech close to a laptop mic peaks around here and reads as a full meter.
const LEVEL_CEILING_DB: f32 = -10.0;

#[derive(Debug, Default)]
pub(crate) struct LevelMeter {
    sum_squares: f64,
    count: usize,
    latest: Option<f32>,
}

impl LevelMeter {
    pub(crate) fn add(&mut self, sample: f32) {
        self.sum_squares += f64::from(sample) * f64::from(sample);
        self.count += 1;
        if self.count >= LEVEL_WINDOW {
            let rms = (self.sum_squares / self.count as f64).sqrt() as f32;
            self.latest = Some(level_from_rms(rms));
            self.sum_squares = 0.0;
            self.count = 0;
        }
    }

    pub(crate) fn take_latest(&mut self) -> Option<f32> {
        self.latest.take()
    }
}

/// Maps RMS amplitude to a 0..1 meter value on a dB scale, which matches perceived loudness:
/// linear RMS would leave the meter nearly empty for normal speech.
fn level_from_rms(rms: f32) -> f32 {
    if rms.is_nan() || rms <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db - LEVEL_FLOOR_DB) / (LEVEL_CEILING_DB - LEVEL_FLOOR_DB)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_mapping_is_bounded() {
        assert_eq!(level_from_rms(0.0), 0.0);
        assert_eq!(level_from_rms(-1.0), 0.0);
        assert_eq!(level_from_rms(f32::NAN), 0.0);
        assert_eq!(level_from_rms(1e-6), 0.0);
        assert_eq!(level_from_rms(1.0), 1.0);
        assert_eq!(level_from_rms(10.0), 1.0);
        let mid = level_from_rms(10f32.powf(-32.5 / 20.0));
        assert!((mid - 0.5).abs() < 1e-3, "{mid}");
    }

    #[test]
    fn level_mapping_is_monotonic() {
        let quiet = level_from_rms(0.003);
        let speech = level_from_rms(0.05);
        let loud = level_from_rms(0.2);
        assert!(quiet < speech && speech < loud);
    }

    #[test]
    fn meter_reports_once_per_window() {
        let mut meter = LevelMeter::default();
        for _ in 0..LEVEL_WINDOW - 1 {
            meter.add(0.1);
        }
        assert_eq!(meter.take_latest(), None);
        meter.add(0.1);
        let level = meter.take_latest();
        assert_eq!(level, Some(level_from_rms(0.1)));
        assert_eq!(meter.take_latest(), None);
    }
}
