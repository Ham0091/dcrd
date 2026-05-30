use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info};

use super::buffer::AudioBuffer;

/// Run speaker playback at 48kHz mono 16-bit.
///
/// Samples are read from the provided ring buffer and played through
/// the default output device. If the buffer is underrun, silence is played.
///
/// Returns when `stop` flag is set to true, or on stream error.
pub fn run_playback(buffer: AudioBuffer, stop: Arc<AtomicBool>) -> anyhow::Result<()> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No audio output device available"))?;

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("Audio playback: using device '{}'", device_name);

    // Request 48kHz mono 16-bit
    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(48_000),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
            for sample in data.iter_mut() {
                *sample = buffer.pop().unwrap_or(0); // Silence on underrun
            }
        },
        |err| {
            error!("Audio playback stream error: {}", err);
        },
        None, // No timeout
    )?;

    stream.play()?;

    info!("Audio playback started (48kHz mono 16-bit)");

    // Keep the thread alive while the stream is active.
    // Check the stop flag periodically so we can exit cleanly.
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    info!("Audio playback stopping");
    // stream is dropped here, which stops the audio playback
    Ok(())
}
