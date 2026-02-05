#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub mod capture;
pub mod convert;
pub mod loopback;
pub mod mixer;
pub mod recorder;

/// Audio sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    I16,
    I32,
    F32,
}

/// Audio device info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_loopback: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

/// A chunk of audio data
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp_ms: u64,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16, timestamp_ms: u64) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
            timestamp_ms,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        let samples_per_channel = self.samples.len() / self.channels as usize;
        (samples_per_channel as u64 * 1000) / self.sample_rate as u64
    }
}
