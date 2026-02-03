use crate::audio::AudioChunk;
use crate::error::{MuesliError, Result};

/// Target sample rate for Whisper
pub const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Convert multi-channel audio to mono by averaging channels
pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }

    let channels = channels as usize;
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resample audio to target sample rate using rubato
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }

    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        2.0,
        params,
        samples.len(),
        1, // mono
    )
    .map_err(|e| MuesliError::Audio(format!("Failed to create resampler: {}", e)))?;

    let input = vec![samples.to_vec()];
    let output = resampler
        .process(&input, None)
        .map_err(|e| MuesliError::Audio(format!("Resample failed: {}", e)))?;

    Ok(output.into_iter().next().unwrap_or_default())
}

/// Normalize samples to [-1.0, 1.0] range
pub fn normalize(samples: &mut [f32]) {
    let max_abs = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_abs > 1.0 {
        for sample in samples.iter_mut() {
            *sample /= max_abs;
        }
    }
}

/// Convert AudioChunk to Whisper-compatible format (16kHz, mono, normalized f32)
pub fn prepare_for_whisper(chunk: &AudioChunk) -> Result<Vec<f32>> {
    // Step 1: Convert to mono
    let mono = to_mono(&chunk.samples, chunk.channels);

    // Step 2: Resample to 16kHz
    let resampled = resample(&mono, chunk.sample_rate, WHISPER_SAMPLE_RATE)?;

    // Step 3: Normalize
    let mut output = resampled;
    normalize(&mut output);

    Ok(output)
}

/// Batch convert multiple chunks
pub fn prepare_chunks_for_whisper(chunks: &[AudioChunk]) -> Result<Vec<f32>> {
    let mut all_samples = Vec::new();

    for chunk in chunks {
        let converted = prepare_for_whisper(chunk)?;
        all_samples.extend(converted);
    }

    Ok(all_samples)
}

/// Convert i16 samples to f32
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect()
}

/// Convert i32 samples to f32
pub fn i32_to_f32(samples: &[i32]) -> Vec<f32> {
    samples
        .iter()
        .map(|&s| s as f32 / i32::MAX as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_mono_stereo() {
        let stereo = vec![0.5, 0.3, 0.7, 0.1];
        let mono = to_mono(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.4).abs() < 0.01); // (0.5 + 0.3) / 2
        assert!((mono[1] - 0.4).abs() < 0.01); // (0.7 + 0.1) / 2
    }

    #[test]
    fn test_to_mono_already_mono() {
        let mono = vec![0.5, 0.3, 0.7];
        let result = to_mono(&mono, 1);
        assert_eq!(result, mono);
    }

    #[test]
    fn test_normalize() {
        let mut samples = vec![2.0, -1.5, 0.5];
        normalize(&mut samples);
        assert!(samples.iter().all(|&s| s >= -1.0 && s <= 1.0));
        assert!((samples[0] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_resample_same_rate() {
        let samples = vec![0.5; 1000];
        let result = resample(&samples, 16000, 16000).unwrap();
        assert_eq!(result.len(), 1000);
    }

    #[test]
    fn test_resample_downsample() {
        let samples = vec![0.5; 48000]; // 1 second at 48kHz
        let result = resample(&samples, 48000, 16000).unwrap();
        // Should be approximately 16000 samples (1 second at 16kHz)
        assert!(result.len() > 15000 && result.len() < 17000);
    }

    #[test]
    fn test_i16_to_f32() {
        let samples = vec![i16::MAX, 0, i16::MIN];
        let converted = i16_to_f32(&samples);
        assert!((converted[0] - 1.0).abs() < 0.01);
        assert!((converted[1]).abs() < 0.01);
        assert!((converted[2] + 1.0).abs() < 0.01);
    }

    #[test]
    fn test_prepare_for_whisper() {
        let chunk = AudioChunk::new(vec![0.5; 48000], 48000, 2, 0);
        let result = prepare_for_whisper(&chunk).unwrap();
        // 48000 stereo at 48kHz -> mono -> 16kHz = ~8000 samples
        assert!(result.len() > 7000 && result.len() < 9000);
    }
}
