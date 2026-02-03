//! Audio stream mixing - combines microphone and loopback audio into one stream.

use crate::audio::AudioChunk;
use std::collections::VecDeque;
use tokio::sync::broadcast;

pub struct AudioMixer {
    mic_buffer: VecDeque<AudioChunk>,
    loopback_buffer: VecDeque<AudioChunk>,
    output_sample_rate: u32,
    output_channels: u16,
}

impl AudioMixer {
    pub fn new(output_sample_rate: u32, output_channels: u16) -> Self {
        Self {
            mic_buffer: VecDeque::new(),
            loopback_buffer: VecDeque::new(),
            output_sample_rate,
            output_channels,
        }
    }

    pub fn add_mic_chunk(&mut self, chunk: AudioChunk) {
        self.mic_buffer.push_back(chunk);
    }

    pub fn add_loopback_chunk(&mut self, chunk: AudioChunk) {
        self.loopback_buffer.push_back(chunk);
    }

    pub fn mix(&mut self) -> Option<AudioChunk> {
        let mic_chunk = self.mic_buffer.pop_front();
        let loopback_chunk = self.loopback_buffer.pop_front();

        match (mic_chunk, loopback_chunk) {
            (Some(mic), Some(loopback)) => Some(self.mix_chunks(mic, loopback)),
            (Some(mic), None) => Some(mic),
            (None, Some(loopback)) => Some(loopback),
            (None, None) => None,
        }
    }

    fn mix_chunks(&self, mic: AudioChunk, loopback: AudioChunk) -> AudioChunk {
        let len = mic.samples.len().max(loopback.samples.len());
        let mut mixed = vec![0.0f32; len];

        for (i, sample) in mic.samples.iter().enumerate() {
            if i < len {
                mixed[i] += sample * 0.5;
            }
        }

        for (i, sample) in loopback.samples.iter().enumerate() {
            if i < len {
                mixed[i] += sample * 0.5;
            }
        }

        for sample in &mut mixed {
            *sample = soft_clip(*sample);
        }

        AudioChunk::new(
            mixed,
            self.output_sample_rate,
            self.output_channels,
            mic.timestamp_ms.min(loopback.timestamp_ms),
        )
    }

    pub fn drain(&mut self) -> Vec<AudioChunk> {
        let mut output = Vec::new();
        while let Some(chunk) = self.mix() {
            output.push(chunk);
        }
        output.extend(self.mic_buffer.drain(..));
        output.extend(self.loopback_buffer.drain(..));
        output
    }
}

pub async fn mix_streams(
    mut mic_rx: broadcast::Receiver<AudioChunk>,
    mut loopback_rx: broadcast::Receiver<AudioChunk>,
    output_tx: broadcast::Sender<AudioChunk>,
    output_sample_rate: u32,
    output_channels: u16,
) {
    let mut mixer = AudioMixer::new(output_sample_rate, output_channels);

    loop {
        tokio::select! {
            Ok(chunk) = mic_rx.recv() => {
                mixer.add_mic_chunk(chunk);
                if let Some(mixed) = mixer.mix() {
                    let _ = output_tx.send(mixed);
                }
            }
            Ok(chunk) = loopback_rx.recv() => {
                mixer.add_loopback_chunk(chunk);
                if let Some(mixed) = mixer.mix() {
                    let _ = output_tx.send(mixed);
                }
            }
            else => break,
        }
    }

    for chunk in mixer.drain() {
        let _ = output_tx.send(chunk);
    }
}

// Attempt symmetric soft limiting at +/-1.0 using exponential curve
fn soft_clip(sample: f32) -> f32 {
    if sample > 1.0 {
        1.0 - (-sample + 1.0).exp() * 0.5
    } else if sample < -1.0 {
        -1.0 + (sample + 1.0).exp() * 0.5
    } else {
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixer_mic_only() {
        let mut mixer = AudioMixer::new(16000, 1);
        let chunk = AudioChunk::new(vec![0.5; 100], 16000, 1, 0);
        mixer.add_mic_chunk(chunk);

        let output = mixer.mix();
        assert!(output.is_some());
        assert_eq!(output.unwrap().samples.len(), 100);
    }

    #[test]
    fn test_mixer_loopback_only() {
        let mut mixer = AudioMixer::new(16000, 1);
        let chunk = AudioChunk::new(vec![0.3; 100], 16000, 1, 0);
        mixer.add_loopback_chunk(chunk);

        let output = mixer.mix();
        assert!(output.is_some());
        assert_eq!(output.unwrap().samples.len(), 100);
    }

    #[test]
    fn test_mixer_both_sources() {
        let mut mixer = AudioMixer::new(16000, 1);

        let mic = AudioChunk::new(vec![0.5; 100], 16000, 1, 0);
        let loopback = AudioChunk::new(vec![0.3; 100], 16000, 1, 0);

        mixer.add_mic_chunk(mic);
        mixer.add_loopback_chunk(loopback);

        let output = mixer.mix();
        assert!(output.is_some());
        let mixed = output.unwrap();
        // expected: (0.5 * 0.5) + (0.3 * 0.5) = 0.4
        assert!((mixed.samples[0] - 0.4).abs() < 0.01);
    }

    #[test]
    fn test_mixer_different_lengths() {
        let mut mixer = AudioMixer::new(16000, 1);

        let mic = AudioChunk::new(vec![0.5; 50], 16000, 1, 0);
        let loopback = AudioChunk::new(vec![0.3; 100], 16000, 1, 0);

        mixer.add_mic_chunk(mic);
        mixer.add_loopback_chunk(loopback);

        let output = mixer.mix();
        assert!(output.is_some());
        let mixed = output.unwrap();
        assert_eq!(mixed.samples.len(), 100);
        // mixed[0]: (0.5*0.5)+(0.3*0.5)=0.4, mixed[75]: 0.3*0.5=0.15
        assert!((mixed.samples[0] - 0.4).abs() < 0.01);
        assert!((mixed.samples[75] - 0.15).abs() < 0.01);
    }

    #[test]
    fn test_mixer_empty() {
        let mut mixer = AudioMixer::new(16000, 1);
        let output = mixer.mix();
        assert!(output.is_none());
    }

    #[test]
    fn test_mixer_drain() {
        let mut mixer = AudioMixer::new(16000, 1);

        mixer.add_mic_chunk(AudioChunk::new(vec![0.5; 100], 16000, 1, 0));
        mixer.add_mic_chunk(AudioChunk::new(vec![0.5; 100], 16000, 1, 10));
        mixer.add_loopback_chunk(AudioChunk::new(vec![0.3; 100], 16000, 1, 0));

        let drained = mixer.drain();
        assert_eq!(drained.len(), 2);
    }

    #[test]
    fn test_soft_clip() {
        assert!((soft_clip(0.5) - 0.5).abs() < 0.01);
        assert!(soft_clip(2.0) < 1.0);
        assert!(soft_clip(-2.0) > -1.0);
        assert!((soft_clip(0.0) - 0.0).abs() < 0.01);
        assert!((soft_clip(-0.5) - (-0.5)).abs() < 0.01);
    }

    #[test]
    fn test_timestamp_uses_earlier() {
        let mut mixer = AudioMixer::new(16000, 1);

        let mic = AudioChunk::new(vec![0.5; 100], 16000, 1, 100);
        let loopback = AudioChunk::new(vec![0.3; 100], 16000, 1, 50);

        mixer.add_mic_chunk(mic);
        mixer.add_loopback_chunk(loopback);

        let output = mixer.mix().unwrap();
        assert_eq!(output.timestamp_ms, 50);
    }
}
