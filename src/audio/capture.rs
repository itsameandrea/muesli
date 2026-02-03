//! Microphone capture using cpal
//!
//! Provides device enumeration and audio streaming from microphone inputs.

use crate::audio::{AudioChunk, AudioDevice};
use crate::error::{MuesliError, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Audio capture from microphone input
pub struct MicCapture {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
}

impl MicCapture {
    /// Create capture from default input device
    pub fn from_default() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| MuesliError::AudioDeviceNotFound("No default input device".into()))?;

        Self::from_device(device)
    }

    /// Create capture from specific device by name (partial match)
    pub fn from_device_name(name: &str) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| MuesliError::Audio(format!("Failed to enumerate devices: {}", e)))?
            .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
            .ok_or_else(|| MuesliError::AudioDeviceNotFound(name.into()))?;

        Self::from_device(device)
    }

    /// Create capture from cpal device
    pub fn from_device(device: Device) -> Result<Self> {
        let supported_config = device
            .default_input_config()
            .map_err(|e| MuesliError::Audio(format!("Failed to get device config: {}", e)))?;

        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();

        Ok(Self {
            device,
            config,
            sample_format,
        })
    }

    /// Get device info
    pub fn device_info(&self) -> Result<AudioDevice> {
        Ok(AudioDevice {
            name: self.device.name().unwrap_or_else(|_| "Unknown".into()),
            is_input: true,
            is_loopback: false,
            sample_rate: self.config.sample_rate.0,
            channels: self.config.channels,
        })
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    /// Get the number of channels
    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    /// Start capturing audio, returns stream handle and receiver
    ///
    /// The stream must be kept alive for capture to continue.
    /// Use `is_running` to signal graceful shutdown.
    pub fn start(
        &self,
        is_running: Arc<AtomicBool>,
    ) -> Result<(Stream, broadcast::Receiver<AudioChunk>)> {
        let (tx, rx) = broadcast::channel::<AudioChunk>(100);
        let channels = self.config.channels;
        let sample_rate = self.config.sample_rate.0;

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                self.build_stream::<f32>(tx.clone(), channels, sample_rate, is_running)?
            }
            SampleFormat::I16 => {
                self.build_stream::<i16>(tx.clone(), channels, sample_rate, is_running)?
            }
            SampleFormat::I32 => {
                self.build_stream::<i32>(tx.clone(), channels, sample_rate, is_running)?
            }
            format => {
                return Err(MuesliError::Audio(format!(
                    "Unsupported sample format: {:?}",
                    format
                )))
            }
        };

        stream
            .play()
            .map_err(|e| MuesliError::AudioStream(format!("Failed to start stream: {}", e)))?;

        Ok((stream, rx))
    }

    fn build_stream<T>(
        &self,
        tx: broadcast::Sender<AudioChunk>,
        channels: u16,
        sample_rate: u32,
        is_running: Arc<AtomicBool>,
    ) -> Result<Stream>
    where
        T: cpal::Sample + cpal::SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let err_fn = |err| eprintln!("Audio stream error: {}", err);
        let start_time = std::time::Instant::now();

        let stream = self
            .device
            .build_input_stream(
                &self.config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if !is_running.load(Ordering::Relaxed) {
                        return;
                    }

                    let samples: Vec<f32> =
                        data.iter().map(|s| cpal::Sample::from_sample(*s)).collect();

                    let chunk = AudioChunk::new(
                        samples,
                        sample_rate,
                        channels,
                        start_time.elapsed().as_millis() as u64,
                    );

                    let _ = tx.send(chunk);
                },
                err_fn,
                None,
            )
            .map_err(|e| MuesliError::AudioStream(format!("Failed to build stream: {}", e)))?;

        Ok(stream)
    }
}

/// List all available input devices
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| MuesliError::Audio(format!("Failed to enumerate devices: {}", e)))?;

    let mut result = Vec::new();
    for device in devices {
        if let Ok(config) = device.default_input_config() {
            result.push(AudioDevice {
                name: device.name().unwrap_or_else(|_| "Unknown".into()),
                is_input: true,
                is_loopback: false,
                sample_rate: config.sample_rate().0,
                channels: config.channels(),
            });
        }
    }

    Ok(result)
}

/// Get default input device info
pub fn default_input_device() -> Result<AudioDevice> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| MuesliError::AudioDeviceNotFound("No default input device".into()))?;

    let config = device
        .default_input_config()
        .map_err(|e| MuesliError::Audio(format!("Failed to get device config: {}", e)))?;

    Ok(AudioDevice {
        name: device.name().unwrap_or_else(|_| "Unknown".into()),
        is_input: true,
        is_loopback: false,
        sample_rate: config.sample_rate().0,
        channels: config.channels(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        let result = list_input_devices();
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_device() {
        let _ = default_input_device();
    }

    #[test]
    fn test_mic_capture_from_default() {
        let result = MicCapture::from_default();
        if let Ok(capture) = result {
            let info = capture.device_info();
            assert!(info.is_ok());
            if let Ok(device) = info {
                assert!(device.is_input);
                assert!(!device.is_loopback);
            }
        }
    }
}
