//! PipeWire loopback/monitor device capture for system audio.

use crate::audio::{AudioChunk, AudioDevice};
use crate::error::{MuesliError, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct LoopbackCapture {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
}

impl LoopbackCapture {
    /// Find PipeWire monitor device (appears as input device with "monitor" in name).
    pub fn find_monitor() -> Result<Self> {
        let host = cpal::default_host();

        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if let Ok(name) = device.name() {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("monitor") || name_lower.contains("loopback") {
                        if let Ok(capture) = Self::from_input_device(device) {
                            return Ok(capture);
                        }
                    }
                }
            }
        }

        if let Some(device) = host.default_input_device() {
            if let Ok(name) = device.name() {
                let name_lower = name.to_lowercase();
                if name_lower.contains("monitor") {
                    if let Ok(capture) = Self::from_input_device(device) {
                        return Ok(capture);
                    }
                }
            }
        }

        Err(MuesliError::AudioDeviceNotFound(
            "No loopback/monitor device found. Ensure PipeWire is running.".to_string(),
        ))
    }

    pub fn from_device_name(name: &str) -> Result<Self> {
        let host = cpal::default_host();

        let device = host
            .input_devices()
            .map_err(|e| MuesliError::Audio(format!("Failed to enumerate devices: {}", e)))?
            .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
            .ok_or_else(|| MuesliError::AudioDeviceNotFound(name.to_string()))?;

        Self::from_input_device(device)
    }

    fn from_input_device(device: Device) -> Result<Self> {
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

    pub fn device_info(&self) -> Result<AudioDevice> {
        Ok(AudioDevice {
            name: self.device.name().unwrap_or_else(|_| "Unknown".to_string()),
            is_input: false,
            is_loopback: true,
            sample_rate: self.config.sample_rate.0,
            channels: self.config.channels,
        })
    }

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Start capture. Caller must keep Stream alive for duration of capture.
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
        T: cpal::Sample<Float = f32> + cpal::SizedSample + Send + 'static,
    {
        let err_fn = |err| eprintln!("Loopback stream error: {}", err);
        let start_time = std::time::Instant::now();

        let stream = self
            .device
            .build_input_stream(
                &self.config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if !is_running.load(Ordering::Relaxed) {
                        return;
                    }

                    let samples: Vec<f32> = data.iter().map(|s| s.to_float_sample()).collect();

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
            .map_err(|e| {
                MuesliError::AudioStream(format!("Failed to build loopback stream: {}", e))
            })?;

        Ok(stream)
    }
}

pub fn list_loopback_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let mut result = Vec::new();

    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if let Ok(name) = device.name() {
                let name_lower = name.to_lowercase();
                if name_lower.contains("monitor") || name_lower.contains("loopback") {
                    if let Ok(config) = device.default_input_config() {
                        result.push(AudioDevice {
                            name,
                            is_input: false,
                            is_loopback: true,
                            sample_rate: config.sample_rate().0,
                            channels: config.channels(),
                        });
                    }
                }
            }
        }
    }

    Ok(result)
}

pub fn is_loopback_available() -> bool {
    LoopbackCapture::find_monitor().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_loopback_devices() {
        let result = list_loopback_devices();
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_loopback_available() {
        let _ = is_loopback_available();
    }

    #[test]
    fn test_find_monitor_graceful_failure() {
        let result = LoopbackCapture::find_monitor();
        match result {
            Ok(capture) => {
                let info = capture.device_info();
                assert!(info.is_ok());
            }
            Err(MuesliError::AudioDeviceNotFound(_)) => {}
            Err(e) => panic!("Unexpected error type: {:?}", e),
        }
    }
}
