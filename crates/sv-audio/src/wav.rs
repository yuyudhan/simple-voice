// FilePath: crates/sv-audio/src/wav.rs
//! 16 kHz mono PCM16 WAV, the format kept on disk for retries and sent to every engine.
//!
//! Short dictations are encoded and decoded in memory. Meeting recordings can run for hours, so
//! they are written chunk by chunk with [`WavWriter`] and mixed or cut with functions that
//! stream fixed-size blocks between files and never hold a whole recording in memory.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use sv_domain::{AppError, AppResult};

use crate::TARGET_SAMPLE_RATE;

const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
const BLOCK_ALIGN: u16 = CHANNELS * BITS_PER_SAMPLE / 8;
const FORMAT_PCM: u16 = 1;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const HEADER_BYTES: usize = 44;
/// Offsets of the two size fields [`WavWriter::finish`] patches.
const RIFF_SIZE_AT: u64 = 4;
const DATA_SIZE_AT: u64 = 40;
/// The largest data chunk the 32-bit RIFF size field can describe (`u32::MAX - 36`), about 37
/// hours of 16 kHz mono PCM16.
const MAX_DATA_BYTES: u64 = 4_294_967_259;
/// A format chunk longer than this is not one this app wrote or can read.
const MAX_FORMAT_BYTES: u64 = 256;
/// Samples moved per step when streaming between files: one second of audio.
const BLOCK_SAMPLES: usize = 16_000;
const READ_BUFFER_BYTES: usize = 64 * 1024;

pub fn encode_wav(samples: &[i16]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX - 36);
    let mut bytes = Vec::with_capacity(HEADER_BYTES + samples.len() * 2);
    bytes.extend_from_slice(&header(data_len));
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// The canonical 44-byte header of a 16 kHz mono PCM16 file with `data_len` bytes of samples.
fn header(data_len: u32) -> [u8; HEADER_BYTES] {
    let fields: [&[u8]; 13] = [
        b"RIFF",
        &data_len.saturating_add(36).to_le_bytes(),
        b"WAVE",
        b"fmt ",
        &16u32.to_le_bytes(),
        &FORMAT_PCM.to_le_bytes(),
        &CHANNELS.to_le_bytes(),
        &TARGET_SAMPLE_RATE.to_le_bytes(),
        &(TARGET_SAMPLE_RATE * u32::from(BLOCK_ALIGN)).to_le_bytes(),
        &BLOCK_ALIGN.to_le_bytes(),
        &BITS_PER_SAMPLE.to_le_bytes(),
        b"data",
        &data_len.to_le_bytes(),
    ];
    let mut header = [0u8; HEADER_BYTES];
    let mut at = 0;
    for field in fields {
        header[at..at + field.len()].copy_from_slice(field);
        at += field.len();
    }
    header
}

/// Milliseconds of 16 kHz audio in `samples`.
pub fn samples_to_ms(samples: u64) -> i64 {
    i64::try_from(samples.saturating_mul(1000) / u64::from(TARGET_SAMPLE_RATE)).unwrap_or(i64::MAX)
}

/// Writes a 16 kHz mono PCM16 WAV file incrementally, so a long recording goes to disk as it is
/// captured instead of accumulating in memory.
///
/// The header's sizes are patched by [`WavWriter::finish`]. A file that was never finished (the
/// app quit mid-recording) still reads back with [`wav_sample_count`], [`copy_wav_range`] and
/// [`mix_wav_files`]: they take a zero data size as "everything to the end of the file".
#[derive(Debug)]
pub struct WavWriter {
    file: BufWriter<File>,
    samples: u64,
}

impl WavWriter {
    /// Creates `path` (replacing any file there) readable by the owner only.
    pub fn create(path: &Path) -> AppResult<WavWriter> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| {
                AppError::Audio(format!("Could not create {}: {error}", path.display()))
            })?;
        let mut file = BufWriter::new(file);
        file.write_all(&header(0))?;
        Ok(WavWriter { file, samples: 0 })
    }

    /// Appends samples; fails once the file would outgrow what a WAV header can describe.
    pub fn append(&mut self, samples: &[i16]) -> AppResult<()> {
        let total = self.samples.saturating_add(samples.len() as u64);
        if total.saturating_mul(2) > MAX_DATA_BYTES {
            return Err(AppError::Audio(
                "The recording reached the longest a WAV file can hold.".to_owned(),
            ));
        }
        for sample in samples {
            self.file.write_all(&sample.to_le_bytes())?;
        }
        self.samples = total;
        Ok(())
    }

    /// Samples appended so far.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Flushes, writes the final sizes into the header and syncs the file to disk. Returns the
    /// recording's duration in milliseconds.
    pub fn finish(self) -> AppResult<i64> {
        let WavWriter { file, samples } = self;
        let mut file = file
            .into_inner()
            .map_err(|error| AppError::io(error.error()))?;
        // `append` keeps the data within `MAX_DATA_BYTES`, so this always fits.
        let data_len = u32::try_from(samples.saturating_mul(2)).unwrap_or(u32::MAX - 36);
        file.seek(SeekFrom::Start(RIFF_SIZE_AT))?;
        file.write_all(&data_len.saturating_add(36).to_le_bytes())?;
        file.seek(SeekFrom::Start(DATA_SIZE_AT))?;
        file.write_all(&data_len.to_le_bytes())?;
        file.sync_all()?;
        Ok(samples_to_ms(samples))
    }
}

/// Sequential reader over the samples of a 16 kHz mono PCM16 WAV file.
#[derive(Debug)]
struct WavReader {
    file: BufReader<File>,
    /// Byte offset of the first sample.
    data_start: u64,
    /// Samples in the file.
    len: u64,
    /// Index of the next sample `read` returns.
    position: u64,
    bytes: Vec<u8>,
}

impl WavReader {
    fn open(path: &Path) -> AppResult<WavReader> {
        let file = File::open(path).map_err(|error| {
            AppError::Audio(format!("Could not open {}: {error}", path.display()))
        })?;
        let file_len = file.metadata()?.len();
        let mut file = BufReader::with_capacity(READ_BUFFER_BYTES, file);
        let mut riff = [0u8; 12];
        if file.read_exact(&mut riff).is_err() || &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE"
        {
            return Err(invalid("it is not a WAV file"));
        }
        let mut offset: u64 = 12;
        let mut format_ok = false;
        loop {
            let mut chunk = [0u8; 8];
            if file.read_exact(&mut chunk).is_err() {
                return Err(invalid("it has no audio data"));
            }
            let size = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
            let body_start = offset + 8;
            // Chunks are word-aligned: odd sizes carry one pad byte.
            let padded = size + (size & 1);
            match &chunk[0..4] {
                b"fmt " => {
                    if size > MAX_FORMAT_BYTES {
                        return Err(invalid("its format chunk is malformed"));
                    }
                    let mut body = vec![0u8; usize::try_from(padded).unwrap_or_default()];
                    file.read_exact(&mut body)
                        .map_err(|_| invalid("its format chunk is truncated"))?;
                    check_format(&body)?;
                    format_ok = true;
                }
                b"data" => {
                    if !format_ok {
                        return Err(invalid("the data chunk comes before the format chunk"));
                    }
                    let available = file_len.saturating_sub(body_start);
                    // Zero is what an unfinished `WavWriter` left; a size larger than the file
                    // is an interrupted recording from another writer. Both mean "to the end".
                    let bytes = if size == 0 || size > available {
                        available
                    } else {
                        size
                    };
                    return Ok(WavReader {
                        file,
                        data_start: body_start,
                        len: bytes / 2,
                        position: 0,
                        bytes: Vec::new(),
                    });
                }
                _ => {
                    let skip = i64::try_from(padded).unwrap_or(i64::MAX);
                    file.seek_relative(skip)?;
                }
            }
            offset = body_start.saturating_add(padded);
        }
    }

    /// Moves to sample `index` (clamped to the end).
    fn seek(&mut self, index: u64) -> AppResult<()> {
        let index = index.min(self.len);
        self.file
            .seek(SeekFrom::Start(self.data_start + index * 2))?;
        self.position = index;
        Ok(())
    }

    /// Fills the front of `out` with the next samples; returns how many (0 at the end).
    fn read(&mut self, out: &mut [i16]) -> AppResult<usize> {
        let left = self.len - self.position;
        let count = usize::try_from(left).map_or(out.len(), |left| left.min(out.len()));
        if count == 0 {
            return Ok(0);
        }
        self.bytes.resize(count * 2, 0);
        self.file.read_exact(&mut self.bytes)?;
        for (sample, pair) in out.iter_mut().zip(self.bytes.chunks_exact(2)) {
            *sample = i16::from_le_bytes([pair[0], pair[1]]);
        }
        self.position += count as u64;
        Ok(count)
    }
}

/// Samples in a 16 kHz mono PCM16 WAV file, read from its header without loading the audio.
pub fn wav_sample_count(path: &Path) -> AppResult<u64> {
    Ok(WavReader::open(path)?.len)
}

/// Copies samples `start..end` of `source` into a new WAV at `out`, a block at a time; `end`
/// past the end of the source stops at its end. Returns the samples written.
pub fn copy_wav_range(source: &Path, start: u64, end: u64, out: &Path) -> AppResult<u64> {
    let mut reader = WavReader::open(source)?;
    let end = end.min(reader.len);
    reader.seek(start)?;
    let mut writer = WavWriter::create(out)?;
    let mut block = vec![0i16; BLOCK_SAMPLES];
    let mut remaining = end.saturating_sub(reader.position);
    while remaining > 0 {
        let want = usize::try_from(remaining).map_or(BLOCK_SAMPLES, |r| r.min(BLOCK_SAMPLES));
        let read = reader.read(&mut block[..want])?;
        if read == 0 {
            break;
        }
        writer.append(&block[..read])?;
        remaining -= read as u64;
    }
    let written = writer.samples();
    writer.finish()?;
    Ok(written)
}

/// Mixes two 16 kHz mono PCM16 WAV files into `out`. The second recording started
/// `second_offset` samples after the first (negative: before it), so its audio is shifted by
/// that much; the result spans both. Samples are summed with saturation, so loud overlaps clip
/// instead of wrapping around. Streams one-second blocks; returns the mix's duration in ms.
pub fn mix_wav_files(
    first: &Path,
    second: &Path,
    second_offset: i64,
    out: &Path,
) -> AppResult<i64> {
    let mut first = WavReader::open(first)?;
    let mut second = WavReader::open(second)?;
    let shift = second_offset.unsigned_abs();
    let (first_delay, second_delay) = if second_offset >= 0 {
        (0, shift)
    } else {
        (shift, 0)
    };
    let total = first_delay
        .saturating_add(first.len)
        .max(second_delay.saturating_add(second.len));

    let mut writer = WavWriter::create(out)?;
    let mut first_block = vec![0i16; BLOCK_SAMPLES];
    let mut second_block = vec![0i16; BLOCK_SAMPLES];
    let mut position: u64 = 0;
    while position < total {
        let count =
            usize::try_from(total - position).map_or(BLOCK_SAMPLES, |n| n.min(BLOCK_SAMPLES));
        fill_delayed(&mut first, first_delay, position, &mut first_block[..count])?;
        fill_delayed(
            &mut second,
            second_delay,
            position,
            &mut second_block[..count],
        )?;
        for (mixed, other) in first_block[..count].iter_mut().zip(&second_block[..count]) {
            *mixed = mixed.saturating_add(*other);
        }
        writer.append(&first_block[..count])?;
        position += count as u64;
    }
    writer.finish()
}

/// Fills `out` with the output samples `position..position + out.len()` of a recording that
/// starts `delay` samples into the output: silence before it starts and after it ends. Reads
/// sequentially, so successive calls must cover consecutive output ranges.
fn fill_delayed(
    reader: &mut WavReader,
    delay: u64,
    position: u64,
    out: &mut [i16],
) -> AppResult<()> {
    out.fill(0);
    let silent = usize::try_from(delay.saturating_sub(position))
        .map_or(out.len(), |silent| silent.min(out.len()));
    let mut filled = silent;
    while filled < out.len() {
        let read = reader.read(&mut out[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(())
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

    /// A scratch directory removed when dropped (this crate has no tempfile dependency).
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("sv-audio-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn path(&self, file: &str) -> std::path::PathBuf {
            self.0.join(file)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_wav(path: &Path, samples: &[i16]) {
        let mut writer = WavWriter::create(path).unwrap();
        // Several appends, as the capture worker delivers them.
        for chunk in samples.chunks(7) {
            writer.append(chunk).unwrap();
        }
        writer.finish().unwrap();
    }

    fn read_wav(path: &Path) -> Vec<i16> {
        decode_wav(&std::fs::read(path).unwrap()).unwrap()
    }

    fn ramp(len: usize) -> Vec<i16> {
        (0..len)
            .map(|i| i16::try_from(i % 30_000).unwrap())
            .collect()
    }

    #[test]
    fn writer_produces_a_file_decode_reads_and_reports_duration() {
        let scratch = Scratch::new("writer");
        let path = scratch.path("mic.wav");
        let samples = ramp(24_000);
        let mut writer = WavWriter::create(&path).unwrap();
        writer.append(&samples[..10_000]).unwrap();
        writer.append(&samples[10_000..]).unwrap();
        assert_eq!(writer.samples(), 24_000);
        assert_eq!(writer.finish().unwrap(), 1_500);
        assert_eq!(read_wav(&path), samples);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..HEADER_BYTES], &header(48_000)[..]);
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&path).unwrap().permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn unfinished_file_still_reads_to_its_end() {
        let scratch = Scratch::new("unfinished");
        let path = scratch.path("mic.wav");
        {
            let mut writer = WavWriter::create(&path).unwrap();
            writer.append(&[5, 6, 7, 8]).unwrap();
            // Dropped without `finish`, as after a crash: sizes in the header stay zero.
        }
        assert_eq!(wav_sample_count(&path).unwrap(), 4);
        let copy = scratch.path("copy.wav");
        assert_eq!(copy_wav_range(&path, 0, u64::MAX, &copy).unwrap(), 4);
        assert_eq!(read_wav(&copy), vec![5, 6, 7, 8]);
    }

    #[test]
    fn copies_a_range_across_block_boundaries() {
        let scratch = Scratch::new("range");
        let source = scratch.path("source.wav");
        let samples = ramp(50_000);
        write_wav(&source, &samples);
        let window = scratch.path("window.wav");
        assert_eq!(
            copy_wav_range(&source, 10_000, 45_000, &window).unwrap(),
            35_000
        );
        assert_eq!(read_wav(&window), samples[10_000..45_000].to_vec());

        let tail = scratch.path("tail.wav");
        assert_eq!(copy_wav_range(&source, 49_990, 60_000, &tail).unwrap(), 10);
        assert_eq!(read_wav(&tail), samples[49_990..].to_vec());

        let empty = scratch.path("empty.wav");
        assert_eq!(copy_wav_range(&source, 70_000, 80_000, &empty).unwrap(), 0);
        assert!(read_wav(&empty).is_empty());
    }

    #[test]
    fn mixes_with_the_second_recording_shifted_later() {
        let scratch = Scratch::new("mix-later");
        let first = scratch.path("mic.wav");
        let second = scratch.path("system.wav");
        write_wav(&first, &[100; 20_000]);
        write_wav(&second, &[10; 20_000]);
        let out = scratch.path("mixed.wav");
        let duration = mix_wav_files(&first, &second, 5_000, &out).unwrap();
        assert_eq!(duration, samples_to_ms(25_000));
        let mixed = read_wav(&out);
        assert_eq!(mixed.len(), 25_000);
        assert!(mixed[..5_000].iter().all(|&s| s == 100));
        assert!(mixed[5_000..20_000].iter().all(|&s| s == 110));
        assert!(mixed[20_000..].iter().all(|&s| s == 10));
    }

    #[test]
    fn mixes_with_the_second_recording_starting_first() {
        let scratch = Scratch::new("mix-earlier");
        let first = scratch.path("mic.wav");
        let second = scratch.path("system.wav");
        write_wav(&first, &[1, 1, 1]);
        write_wav(&second, &[2, 2, 2, 2, 2, 2]);
        let out = scratch.path("mixed.wav");
        mix_wav_files(&first, &second, -2, &out).unwrap();
        assert_eq!(read_wav(&out), vec![2, 2, 3, 3, 3, 2]);
    }

    #[test]
    fn mixing_saturates_instead_of_wrapping() {
        let scratch = Scratch::new("mix-clip");
        let first = scratch.path("mic.wav");
        let second = scratch.path("system.wav");
        write_wav(&first, &[30_000, -30_000, 1_000]);
        write_wav(&second, &[10_000, -10_000, -500]);
        let out = scratch.path("mixed.wav");
        mix_wav_files(&first, &second, 0, &out).unwrap();
        assert_eq!(read_wav(&out), vec![i16::MAX, i16::MIN, 500]);
    }

    #[test]
    fn streaming_reader_rejects_what_is_not_16k_mono() {
        let scratch = Scratch::new("reject");
        let path = scratch.path("stereo.wav");
        let mut stereo = encode_wav(&[1, 2]);
        stereo[22..24].copy_from_slice(&2u16.to_le_bytes());
        std::fs::write(&path, stereo).unwrap();
        assert!(matches!(wav_sample_count(&path), Err(AppError::Audio(_))));
        assert!(wav_sample_count(&scratch.path("missing.wav")).is_err());
    }
}
