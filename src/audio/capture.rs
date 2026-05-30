use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tracing::{error, info};

use super::buffer::AudioBuffer;

/// Run microphone capture at 48kHz mono 16-bit.
///
/// Captured samples are pushed into the provided ring buffer.
/// This function blocks and should be run on a dedicated thread.
///
/// Returns only on error or when the stream is dropped.
pub fn run_capture(buffer: AudioBuffer) -> anyhow::Result<()> {
    let host = cpal::default_host();

    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No audio input device available"))?;

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("Audio capture: using device '{}'", device_name);

    // Request 48kHz mono 16-bit
    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(48_000),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device.build_input_stream(
        &config,
        move |data: &[i16], _: &cpal::InputCallbackInfo| {
            for &sample in data {
                // Push samples into the ring buffer.
                // If the buffer is full, drop the oldest sample to make room.
                if buffer.push(sample).is_err() {
                    // Buffer full — drop oldest by popping one
                    buffer.pop();
                    buffer.push(sample).ok();
                }
            }
        },
        |err| {
            error!("Audio capture stream error: {}", err);
        },
        None, // No timeout
    )?;

    stream.play()?;

    info!("Audio capture started (48kHz mono 16-bit)");

    // Keep the thread alive while the stream is active.
    // The stream will be dropped when this function returns.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        // Check if the stream is still alive by trying to do something
        // The stream callback runs in the background
    }
}
