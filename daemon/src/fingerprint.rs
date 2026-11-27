//! AcoustID fingerprints via the Chromaprint algorithm (preset test2).
//!
//! Only the first two minutes are decoded. The result is the base64 string
//! `fpcalc` would send, not the audio itself.

use std::fs::File;
use std::path::Path;

use anyhow::{bail, Context, Result};
use base64::Engine;
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const MAX_SECONDS: u64 = 120;

pub fn fingerprint_file(path: &Path) -> Result<String> {
    let decoded =
        decode_prefix(path).with_context(|| format!("could not decode {}", path.display()))?;
    if decoded.pcm.is_empty() {
        bail!("no samples to fingerprint in {}", path.display());
    }
    let config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&config);
    printer
        .start(decoded.rate, decoded.channels)
        .context("could not start chromaprint")?;
    printer.consume(&decoded.pcm);
    printer.finish();
    let raw = printer.fingerprint();
    if raw.is_empty() {
        bail!(
            "chromaprint produced an empty fingerprint for {}",
            path.display()
        );
    }
    let compressed = FingerprintCompressor::from(&config).compress(raw);
    Ok(base64::engine::general_purpose::STANDARD.encode(compressed))
}

struct Decoded {
    rate: u32,
    channels: u32,
    pcm: Vec<i16>,
}

fn decode_prefix(path: &Path) -> Result<Decoded> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .context("file has no audio track")?
        .clone();
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut decoded = Decoded {
        rate: 0,
        channels: 0,
        pcm: Vec::new(),
    };
    let mut budget: Option<u64> = None;

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
        let audio = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(_) => continue,
        };
        if sample_buf.is_none() {
            let spec = *audio.spec();
            let channels = u32::try_from(spec.channels.count()).unwrap_or(0);
            if channels == 0 || spec.rate == 0 {
                bail!("audio has no channels or sample rate");
            }
            decoded.rate = spec.rate;
            decoded.channels = channels;
            budget = Some(u64::from(spec.rate) * u64::from(channels) * MAX_SECONDS);
            sample_buf = Some(SampleBuffer::<f32>::new(audio.capacity() as u64, spec));
        }
        let Some(buf) = sample_buf.as_mut() else {
            break;
        };
        buf.copy_interleaved_ref(audio);
        for sample in buf.samples() {
            decoded.pcm.push(to_i16(*sample));
            if let Some(left) = budget.as_mut() {
                *left = left.saturating_sub(1);
                if *left == 0 {
                    break;
                }
            }
        }
    }

    Ok(decoded)
}

fn to_i16(sample: f32) -> i16 {
    if !sample.is_finite() {
        return 0;
    }
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_has_a_stable_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        write_tone(&path);
        let first = fingerprint_file(&path).unwrap();
        let second = fingerprint_file(&path).unwrap();
        assert!(!first.is_empty());
        assert_eq!(first, second);
    }

    fn write_tone(path: &Path) {
        let rate = 11_025u32;
        let samples: Vec<i16> = (0..rate * 3)
            .map(|index| {
                let t = index as f32 / rate as f32;
                let sample = (t * 440.0 * std::f32::consts::TAU).sin() * 0.4;
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
