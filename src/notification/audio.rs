use crate::config::settings::AudioCuesConfig;
use crate::error::Result;
use rodio::source::{SineWave, Source};
use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

pub fn play_recording_start(config: &AudioCuesConfig) {
    if !config.enabled {
        return;
    }

    if let Some(ref custom_path) = config.start_sound {
        if let Err(e) = play_custom_sound(custom_path, config.volume) {
            tracing::warn!("Failed to play custom start sound: {}", e);
            play_default_start_sound(config.volume);
        }
    } else {
        play_default_start_sound(config.volume);
    }
}

pub fn play_recording_stop(config: &AudioCuesConfig) {
    if !config.enabled {
        return;
    }

    if let Some(ref custom_path) = config.stop_sound {
        if let Err(e) = play_custom_sound(custom_path, config.volume) {
            tracing::warn!("Failed to play custom stop sound: {}", e);
            play_default_stop_sound(config.volume);
        }
    } else {
        play_default_stop_sound(config.volume);
    }
}

fn play_default_start_sound(volume: f32) {
    std::thread::spawn(move || {
        if let Err(e) = play_ascending_tone(volume) {
            tracing::warn!("Failed to play start sound: {}", e);
        }
    });
}

fn play_default_stop_sound(volume: f32) {
    std::thread::spawn(move || {
        if let Err(e) = play_descending_tone(volume) {
            tracing::warn!("Failed to play stop sound: {}", e);
        }
    });
}

fn play_ascending_tone(volume: f32) -> Result<()> {
    let (_stream, stream_handle) =
        OutputStream::try_default().map_err(|e| crate::error::MuesliError::Audio(e.to_string()))?;
    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| crate::error::MuesliError::Audio(e.to_string()))?;
    sink.set_volume(volume);

    let tone1 = SineWave::new(523.25)
        .take_duration(Duration::from_millis(100))
        .amplify(0.3);
    let tone2 = SineWave::new(659.25)
        .take_duration(Duration::from_millis(100))
        .amplify(0.3);
    let tone3 = SineWave::new(783.99)
        .take_duration(Duration::from_millis(150))
        .amplify(0.3);

    sink.append(tone1);
    sink.append(tone2);
    sink.append(tone3);
    sink.sleep_until_end();

    Ok(())
}

fn play_descending_tone(volume: f32) -> Result<()> {
    let (_stream, stream_handle) =
        OutputStream::try_default().map_err(|e| crate::error::MuesliError::Audio(e.to_string()))?;
    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| crate::error::MuesliError::Audio(e.to_string()))?;
    sink.set_volume(volume);

    let tone1 = SineWave::new(783.99)
        .take_duration(Duration::from_millis(100))
        .amplify(0.3);
    let tone2 = SineWave::new(659.25)
        .take_duration(Duration::from_millis(100))
        .amplify(0.3);
    let tone3 = SineWave::new(523.25)
        .take_duration(Duration::from_millis(150))
        .amplify(0.3);

    sink.append(tone1);
    sink.append(tone2);
    sink.append(tone3);
    sink.sleep_until_end();

    Ok(())
}

fn play_custom_sound(path: &Path, volume: f32) -> Result<()> {
    let file = File::open(path)
        .map_err(|e| crate::error::MuesliError::Audio(format!("Cannot open sound file: {}", e)))?;
    let reader = BufReader::new(file);

    let (_stream, stream_handle) =
        OutputStream::try_default().map_err(|e| crate::error::MuesliError::Audio(e.to_string()))?;
    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| crate::error::MuesliError::Audio(e.to_string()))?;

    let source = Decoder::new(reader).map_err(|e| {
        crate::error::MuesliError::Audio(format!("Cannot decode sound file: {}", e))
    })?;

    sink.set_volume(volume);
    sink.append(source);

    let path_clone = path.to_path_buf();
    std::thread::spawn(move || {
        sink.sleep_until_end();
        tracing::debug!("Finished playing custom sound: {:?}", path_clone);
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_audio_cues() {
        let config = AudioCuesConfig {
            enabled: false,
            volume: 0.5,
            start_sound: None,
            stop_sound: None,
        };
        play_recording_start(&config);
        play_recording_stop(&config);
    }
}
