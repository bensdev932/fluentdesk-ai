//! Audio format conversion, resampling, and unified RIFF/WAVE PCM16 codec (`T014`, `T015`, `T024`, `T041`).

use crate::domain::{
    AudioChunk, AudioFormat, DomainValidationError, SampleRepresentation, SessionError,
    SessionErrorKind,
};

const WAV_HEADER_BYTES: usize = 44;

/// Converts signed 16-bit PCM samples into normalized `[-1.0, 1.0]` `f32` samples.
pub fn pcm_i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|&s| (f32::from(s) / 32_768.0).clamp(-1.0, 1.0))
        .collect()
}

/// Downmixes interleaved stereo (`2` channels) `f32` samples into mono (`1` channel).
pub fn stereo_to_mono_f32(interleaved_stereo: &[f32]) -> Vec<f32> {
    interleaved_stereo
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[l, r]| (l + r) * 0.5)
        .collect()
}

/// Resamples mono `f32` PCM samples from `src_rate_hz` to `dst_rate_hz`.
pub fn resample_mono_f32(
    samples: &[f32],
    src_rate_hz: u32,
    dst_rate_hz: u32,
) -> Result<Vec<f32>, DomainValidationError> {
    if src_rate_hz == 0 || dst_rate_hz == 0 {
        return Err(DomainValidationError::InvalidAudioFormat {
            sample_rate_hz: 0,
            channels: 1,
        });
    }
    if src_rate_hz == dst_rate_hz || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    Ok(interpolate_linear(samples, src_rate_hz, dst_rate_hz))
}

fn interpolate_linear(samples: &[f32], src_rate_hz: u32, dst_rate_hz: u32) -> Vec<f32> {
    let out_len =
        ((samples.len() as u64 * u64::from(dst_rate_hz)) / u64::from(src_rate_hz)) as usize;
    let ratio = f64::from(src_rate_hz) / f64::from(dst_rate_hz);
    (0..out_len)
        .map(|i| sample_at_pos(samples, i as f64 * ratio))
        .collect()
}

fn sample_at_pos(samples: &[f32], pos: f64) -> f32 {
    let idx = (pos.floor() as usize).min(samples.len().saturating_sub(1));
    let next = (idx + 1).min(samples.len().saturating_sub(1));
    let frac = (pos - (idx as f64)) as f32;
    samples[idx] + (samples[next] - samples[idx]) * frac
}

/// Normalizes any valid `AudioChunk` into 16 kHz mono `f32` samples.
pub fn normalize_chunk_to_16khz_mono(chunk: &AudioChunk) -> Result<Vec<f32>, SessionError> {
    chunk.format.validate().map_err(format_err)?;
    let mono = if chunk.format.channels == 2 {
        stereo_to_mono_f32(&chunk.samples_f32)
    } else {
        chunk.samples_f32.clone()
    };
    resample_mono_f32(&mono, chunk.format.sample_rate_hz, 16_000).map_err(format_err)
}

/// Encodes normalized mono `f32` samples into a 44-byte RIFF/WAVE 16-bit PCM byte buffer.
pub fn encode_pcm16_wav(samples_f32: &[f32], sample_rate_hz: u32) -> Vec<u8> {
    let data_bytes = (samples_f32.len() * 2) as u32;
    let mut buf = Vec::with_capacity(WAV_HEADER_BYTES + data_bytes as usize);
    write_wav_header(&mut buf, sample_rate_hz, data_bytes);
    for &sample in samples_f32 {
        let pcm = (sample.clamp(-1.0, 1.0) * 32_767.0) as i16;
        buf.extend_from_slice(&pcm.to_le_bytes());
    }
    buf
}

fn write_wav_header(buf: &mut Vec<u8>, sample_rate_hz: u32, data_bytes: u32) {
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16_u32.to_le_bytes());
    buf.extend_from_slice(&1_u16.to_le_bytes());
    buf.extend_from_slice(&1_u16.to_le_bytes());
    buf.extend_from_slice(&sample_rate_hz.to_le_bytes());
    buf.extend_from_slice(&(sample_rate_hz * 2).to_le_bytes());
    buf.extend_from_slice(&2_u16.to_le_bytes());
    buf.extend_from_slice(&16_u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_bytes.to_le_bytes());
}

/// Decodes a 16-bit PCM WAV byte buffer using its header sample rate and channels (`T039`, `T041`).
pub fn decode_wav_bytes_to_audio_chunk(wav_bytes: &[u8]) -> Result<AudioChunk, SessionError> {
    let format = parse_wav_format(wav_bytes)?;
    let samples = decode_pcm16_payload(&wav_bytes[WAV_HEADER_BYTES..]);
    if samples.is_empty() {
        return Err(wav_err(
            "Synthesized WAV payload contained zero audio samples.",
        ));
    }
    let frames = (samples.len() as u64) / u64::from(format.channels.max(1));
    let duration_ms = ((frames * 1000) / u64::from(format.sample_rate_hz)).max(1) as u32;
    AudioChunk::new_synthesized(format, samples, 0, duration_ms)
        .map_err(|e| wav_err(&e.to_string()))
}

fn parse_wav_format(wav: &[u8]) -> Result<AudioFormat, SessionError> {
    if wav.len() <= WAV_HEADER_BYTES || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err(wav_err("Invalid or truncated WAV header."));
    }
    let channels = u8::try_from(u16::from_le_bytes([wav[22], wav[23]])).unwrap_or(0);
    let sample_rate_hz = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let format = AudioFormat {
        sample_rate_hz,
        channels,
        sample_format: SampleRepresentation::PcmF32,
    };
    format.validate().map_err(|e| wav_err(&e.to_string()))?;
    Ok(format)
}

fn decode_pcm16_payload(pcm_bytes: &[u8]) -> Vec<f32> {
    pcm_bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| f32::from(i16::from_le_bytes(pair)) / 32_768.0)
        .collect()
}

fn format_err(err: DomainValidationError) -> SessionError {
    SessionError::new(
        SessionErrorKind::AudioDeviceUnavailable,
        "Unsupported audio stream format.",
        true,
        Some(err.to_string()),
    )
}

fn wav_err(reason: &str) -> SessionError {
    SessionError::new(
        SessionErrorKind::TtsFailed,
        "Local voice synthesis failed.",
        true,
        Some(reason.to_string()),
    )
}
