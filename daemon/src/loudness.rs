//! Per-track gain so local files play at a similar loudness.
//!
//! ReplayGain track gain is used when the file has it. Otherwise a few seconds
//! of audio are measured and turned into a gain toward one target level, limited
//! so the peak is not driven into clipping.

use std::fs::File;
use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::read_from_path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Short-window loudness (dBFS) that every track is pulled toward.
const TARGET_DB: f32 = -14.0;
const MIN_ADJUST_DB: f32 = -12.0;
const MAX_ADJUST_DB: f32 = 12.0;
/// Leave a little headroom under full scale.
const PEAK_LIMIT: f32 = 0.98;
const SAMPLE_SECONDS: u64 = 8;

/// Linear gain applied on top of the user's volume. `1.0` leaves the file alone.
pub fn playback_gain(path: &Path) -> f32 {
    if let Some(adjust_db) = replaygain_db(path) {
        let linear = 10f32.powf(adjust_db.clamp(MIN_ADJUST_DB, MAX_ADJUST_DB) / 20.0);
        return if linear.is_finite() && linear > 0.0 {
            linear
        } else {
            1.0
        };
    }
    match measure(path) {
        Some((level_db, peak)) => gain_for_level(level_db, peak),
        None => 1.0,
    }
}

/// `level_db` is the track's loudness. `peak` is the largest absolute sample.
pub fn gain_for_level(level_db: f32, peak: f32) -> f32 {
    if !level_db.is_finite() {
        return 1.0;
    }
    let adjust = (TARGET_DB - level_db).clamp(MIN_ADJUST_DB, MAX_ADJUST_DB);
    let mut linear = 10f32.powf(adjust / 20.0);
    if peak.is_finite() && peak > 0.0 && peak * linear > PEAK_LIMIT {
        linear = PEAK_LIMIT / peak;
    }
    if !linear.is_finite() || linear <= 0.0 {
        1.0
    } else {
        linear
    }
}

fn replaygain_db(path: &Path) -> Option<f32> {
    let tagged = read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let text = tag.get_string(ItemKey::ReplayGainTrackGain)?;
    parse_replaygain_db(text)
}

fn parse_replaygain_db(text: &str) -> Option<f32> {
    let token = text.split_whitespace().next()?.trim();
    let value: f32 = token.parse().ok()?;
    value.is_finite().then_some(value)
}

/// Loudness of the louder parts of the file, in dBFS, plus the peak sample.
fn measure(path: &Path) -> Option<(f32, f32)> {
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)?
        .clone();
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut meter = Meter::default();
    decode_for(
        &mut format,
        &mut decoder,
        track_id,
        &mut meter,
        SAMPLE_SECONDS,
    );
    if let (Some(base), Some(n_frames)) =
        (track.codec_params.time_base, track.codec_params.n_frames)
    {
        let duration = base.calc_time(n_frames);
        if duration.seconds > SAMPLE_SECONDS * 2 {
            let at = duration.seconds / 2;
            if format
                .seek(
                    SeekMode::Coarse,
                    SeekTo::Time {
                        time: Time::new(at, 0.0),
                        track_id: Some(track_id),
                    },
                )
                .is_ok()
            {
                decoder.reset();
                decode_for(
                    &mut format,
                    &mut decoder,
                    track_id,
                    &mut meter,
                    SAMPLE_SECONDS,
                );
            }
        }
    }
    meter.level_db().map(|level| (level, meter.peak))
}

fn decode_for(
    format: &mut Box<dyn symphonia::core::formats::FormatReader>,
    decoder: &mut Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    meter: &mut Meter,
    seconds: u64,
) {
    let mut budget: Option<u64> = None;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    loop {
        if budget == Some(0) {
            break;
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(_) => continue,
        };
        if sample_buf.is_none() {
            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1) as u64;
            let rate = u64::from(spec.rate.max(1));
            meter.window_len = (rate * channels / 20).max(1);
            budget = Some(rate * channels * seconds);
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        }
        let Some(buf) = sample_buf.as_mut() else {
            break;
        };
        buf.copy_interleaved_ref(decoded);
        for sample in buf.samples() {
            meter.push(*sample);
            if let Some(left) = budget.as_mut() {
                *left = left.saturating_sub(1);
                if *left == 0 {
                    break;
                }
            }
        }
    }
}

#[derive(Default)]
struct Meter {
    window_len: u64,
    window_sum: f64,
    window_count: u64,
    windows: Vec<f32>,
    peak: f32,
}

impl Meter {
    fn push(&mut self, sample: f32) {
        if !sample.is_finite() {
            return;
        }
        let abs = sample.abs();
        if abs > self.peak {
            self.peak = abs;
        }
        let window = if self.window_len == 0 {
            1
        } else {
            self.window_len
        };
        self.window_sum += f64::from(sample) * f64::from(sample);
        self.window_count += 1;
        if self.window_count >= window {
            self.flush_window();
        }
    }

    fn flush_window(&mut self) {
        if self.window_count == 0 {
            return;
        }
        let rms = (self.window_sum / self.window_count as f64).sqrt();
        // Skip silence so a quiet intro does not set the level of the song.
        if rms > 0.01 {
            self.windows.push(rms as f32);
        }
        self.window_sum = 0.0;
        self.window_count = 0;
    }

    /// 80th percentile of the short-window levels, in dBFS.
    fn level_db(&mut self) -> Option<f32> {
        self.flush_window();
        if self.windows.is_empty() {
            return None;
        }
        self.windows
            .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let index = ((self.windows.len() - 1) as f32 * 0.8) as usize;
        let level = self.windows[index];
        if level <= 0.0 {
            return None;
        }
        Some(20.0 * level.log10())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_replaygain_text() {
        assert_eq!(parse_replaygain_db("-6.54 dB"), Some(-6.54));
        assert_eq!(parse_replaygain_db("3.0"), Some(3.0));
        assert_eq!(parse_replaygain_db("nope"), None);
    }

    #[test]
    fn quiet_audio_is_boosted_and_loud_audio_is_cut() {
        let quiet = gain_for_level(-28.0, 0.05);
        let loud = gain_for_level(-8.0, 0.9);
        assert!(quiet > 1.0);
        assert!(loud < 1.0);
        assert!(0.05 * quiet <= PEAK_LIMIT + 0.001);
    }

    #[test]
    fn a_hot_peak_is_not_boosted_into_clipping() {
        let gain = gain_for_level(-30.0, 1.0);
        assert!(gain <= PEAK_LIMIT + 0.001);
    }

    #[test]
    fn measured_wav_gain_lifts_a_quiet_tone_above_a_loud_one() {
        let dir = tempfile::tempdir().unwrap();
        let quiet = dir.path().join("quiet.wav");
        let loud = dir.path().join("loud.wav");
        write_tone(&quiet, 0.05);
        write_tone(&loud, 0.6);
        let quiet_gain = playback_gain(&quiet);
        let loud_gain = playback_gain(&loud);
        assert!(
            quiet_gain > loud_gain,
            "quiet {quiet_gain} should exceed loud {loud_gain}"
        );
        assert!(loud_gain < 1.0);
    }

    fn write_tone(path: &Path, amplitude: f32) {
        let rate = 8_000u32;
        let samples: Vec<i16> = (0..rate)
            .map(|index| {
                let t = index as f32 / rate as f32;
                let sample = (t * 440.0 * std::f32::consts::TAU).sin() * amplitude;
                (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
            })
            .collect();
        let data_len = samples.len() * 2;
        let mut bytes = Vec::with_capacity(44 + data_len);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }
}
