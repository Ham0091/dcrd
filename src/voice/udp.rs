use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tracing::info;

/// UDP transport for Discord voice data.
///
/// Handles:
/// - IP discovery (70-byte handshake to discover external IP/port)
/// - Sending encrypted voice packets with RTP framing
/// - Receiving and parsing incoming voice packets
#[allow(dead_code)]
pub struct VoiceUdp {
    socket: UdpSocket,
    /// Discord voice server address (from VOICE_SERVER_UPDATE)
    remote_addr: SocketAddr,
    /// Our SSRC (from Voice Gateway READY)
    ssrc: u32,
    /// Outgoing RTP sequence number
    sequence: u16,
    /// Outgoing RTP timestamp (increments by 960 per frame)
    timestamp: u32,
}

/// Result of the UDP IP discovery handshake.
#[derive(Debug)]
pub struct IpDiscoveryResult {
    pub ip: String,
    pub port: u16,
}

impl VoiceUdp {
    /// Create a new UDP transport bound to any local port.
    pub async fn new(remote_addr: SocketAddr, ssrc: u32) -> anyhow::Result<Self> {
        // Bind to any available local port
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(remote_addr).await?;

        info!("UDP socket bound, remote={}", remote_addr);

        Ok(VoiceUdp {
            socket,
            remote_addr,
            ssrc,
            sequence: 0,
            timestamp: 0,
        })
    }

    /// Perform the IP discovery handshake.
    ///
    /// Sends a 70-byte packet (4 bytes SSRC + 66 zero bytes) to the voice
    /// server, which responds with our external IP and port.
    pub async fn ip_discovery(&self) -> anyhow::Result<IpDiscoveryResult> {
        // Build 70-byte discovery packet
        let mut packet = [0u8; 70];
        packet[0..4].copy_from_slice(&self.ssrc.to_be_bytes());

        self.socket.send(&packet).await?;

        // Wait for response (70 bytes: null-terminated IP string + port)
        let mut buf = [0u8; 70];
        let len = self.socket.recv(&mut buf).await?;

        if len < 8 {
            return Err(anyhow::anyhow!("IP discovery response too short: {} bytes", len));
        }

        // Parse IP: bytes 4..68 are null-terminated IP string
        let ip_bytes = &buf[4..68];
        let ip_end = ip_bytes.iter().position(|&b| b == 0).unwrap_or(ip_bytes.len());
        let ip = String::from_utf8_lossy(&ip_bytes[..ip_end]).to_string();

        // Parse port: last 2 bytes (big-endian)
        let port = u16::from_be_bytes([buf[68], buf[69]]);

        info!("IP discovery: external={}:{}", ip, port);

        Ok(IpDiscoveryResult { ip, port })
    }

    /// Send an encrypted voice packet.
    ///
    /// The packet format is:
    /// [RTP header (12 bytes)] [encrypted Opus data] [Poly1305 tag (16 bytes)]
    pub async fn send_voice_packet(&mut self, encrypted_payload: &[u8]) -> anyhow::Result<()> {
        let header = self.build_rtp_header();
        let mut packet = Vec::with_capacity(12 + encrypted_payload.len());
        packet.extend_from_slice(&header);
        packet.extend_from_slice(encrypted_payload);

        self.socket.send(&packet).await?;

        // Advance sequence and timestamp
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(960); // 960 samples per 20ms frame

        Ok(())
    }

    /// Receive a voice packet from the UDP socket.
    ///
    /// Returns (rtp_header, encrypted_payload) where:
    /// - rtp_header: 12 bytes
    /// - encrypted_payload: remaining bytes (ciphertext + tag)
    pub async fn recv_voice_packet(&self) -> anyhow::Result<([u8; 12], Vec<u8>)> {
        let mut buf = vec![0u8; 4096];
        let len = self.socket.recv(&mut buf).await?;

        if len < 12 {
            return Err(anyhow::anyhow!("Voice packet too short: {} bytes", len));
        }

        let mut header = [0u8; 12];
        header.copy_from_slice(&buf[..12]);
        let payload = buf[12..len].to_vec();

        Ok((header, payload))
    }

    /// Get the SSRC of the sender from an RTP header.
    #[allow(dead_code)]
    pub fn parse_ssrc(header: &[u8; 12]) -> u32 {
        u32::from_be_bytes([header[8], header[9], header[10], header[11]])
    }

    /// Get the sequence number from an RTP header.
    #[allow(dead_code)]
    pub fn parse_sequence(header: &[u8; 12]) -> u16 {
        u16::from_be_bytes([header[2], header[3]])
    }

    /// Get the timestamp from an RTP header.
    #[allow(dead_code)]
    pub fn parse_timestamp(header: &[u8; 12]) -> u32 {
        u32::from_be_bytes([header[4], header[5], header[6], header[7]])
    }

    /// Build the 12-byte RTP header for outgoing packets.
    fn build_rtp_header(&self) -> [u8; 12] {
        let mut header = [0u8; 12];
        header[0] = 0x80; // Version 2, no padding, no extension, no CSRC
        header[1] = 0x78; // Payload type 120 (0x78)
        header[2..4].copy_from_slice(&self.sequence.to_be_bytes());
        header[4..8].copy_from_slice(&self.timestamp.to_be_bytes());
        header[8..12].copy_from_slice(&self.ssrc.to_be_bytes());
        header
    }

    /// Get the local address this socket is bound to.
    #[allow(dead_code)]
    pub fn local_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }
}
