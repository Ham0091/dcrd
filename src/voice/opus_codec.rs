/// Opus encoder/decoder wrapper for Discord voice.
///
/// Configuration:
/// - Sample rate: 48000 Hz
/// - Channels: 1 (Mono — Discord uses mono for voice)
/// - Frame size: 960 samples (20ms at 48kHz)
/// - Bitrate: 64 kbps (Discord default)
/// - Application: VOIP
pub const SAMPLE_RATE: u32 = 48_000;
#[allow(dead_code)]
pub const CHANNELS: u16 = 1;
pub const FRAME_SIZE: usize = 960; // 20ms at 48kHz
pub const BITRATE: i32 = 64_000;   // 64 kbps

/// Maximum Opus frame size in bytes (for output buffer allocation).
pub const MAX_FRAME_BYTES: usize = 4000;

/// Wraps the opus encoder for encoding PCM → Opus.
pub struct OpusEncoder {
    encoder: opus::Encoder,
    output_buffer: Vec<u8>,
}

impl OpusEncoder {
    /// Create a new Opus encoder with VOIP settings.
    pub fn new() -> anyhow::Result<Self> {
        let mut encoder = opus::Encoder::new(
            SAMPLE_RATE,
            opus::Channels::Mono,
            opus::Application::Voip,
        )?;
        encoder.set_bitrate(opus::Bitrate::Bits(BITRATE))?;
        // Disable DTX to keep the stream continuous
        encoder.set_dtx(false)?;

        Ok(OpusEncoder {
            encoder,
            output_buffer: vec![0u8; MAX_FRAME_BYTES],
        })
    }

    /// Encode a frame of 960 PCM samples (i16) into an Opus packet.
    ///
    /// Returns the encoded bytes. The output is valid until the next call.
    pub fn encode(&mut self, pcm: &[i16]) -> anyhow::Result<&[u8]> {
        let len = self.encoder.encode(pcm, &mut self.output_buffer)?;
        Ok(&self.output_buffer[..len])
    }
}

/// Wraps the opus decoder for decoding Opus → PCM.
pub struct OpusDecoder {
    decoder: opus::Decoder,
    output_buffer: Vec<i16>,
}

impl OpusDecoder {
    /// Create a new Opus decoder.
    pub fn new() -> anyhow::Result<Self> {
        let decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono)?;
        Ok(OpusDecoder {
            decoder,
            output_buffer: vec![0i16; FRAME_SIZE],
        })
    }

    /// Decode an Opus packet into PCM samples (i16).
    ///
    /// Returns the decoded PCM samples. The output is valid until the next call.
    /// If `packet` is empty/None, generates concealment (PLC) audio.
    pub fn decode(&mut self, packet: &[u8]) -> anyhow::Result<&[i16]> {
        let len = self
            .decoder
            .decode(packet, &mut self.output_buffer, false)?;
        Ok(&self.output_buffer[..len])
    }

    /// Decode with packet loss concealment (when a packet is lost).
    #[allow(dead_code)]
    pub fn decode_plc(&mut self) -> anyhow::Result<&[i16]> {
        let len = self
            .decoder
            .decode(&[], &mut self.output_buffer, false)?;
        Ok(&self.output_buffer[..len])
    }
}
