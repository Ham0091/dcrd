use crypto_secretbox::aead::Aead;
use crypto_secretbox::{Key, KeyInit, Nonce, XSalsa20Poly1305};

/// XSalsa20-Poly1305 encryption/decryption for Discord voice packets.
///
/// Discord voice uses NaCl's `crypto_secretbox`:
/// - Key: 32 bytes (from SESSION_DESCRIPTION)
/// - Nonce: 24 bytes (first 12 bytes = RTP header, remaining 12 = zeros)
/// - Output: ciphertext || 16-byte Poly1305 tag
pub struct VoiceCipher {
    cipher: XSalsa20Poly1305,
}

impl VoiceCipher {
    /// Create a new cipher from the 32-byte encryption key provided by
    /// the Voice Gateway's SESSION_DESCRIPTION event.
    pub fn new(key_bytes: &[u8; 32]) -> Self {
        let key = Key::from_slice(key_bytes);
        let cipher = XSalsa20Poly1305::new(key);
        VoiceCipher { cipher }
    }

    /// Derive a 24-byte nonce from the 12-byte RTP header.
    ///
    /// The nonce is: [rtp_header (12 bytes)] [zeros (12 bytes)]
    pub fn derive_nonce(rtp_header: &[u8; 12]) -> [u8; 24] {
        let mut nonce_bytes = [0u8; 24];
        nonce_bytes[..12].copy_from_slice(rtp_header);
        // remaining 12 bytes are already zero
        nonce_bytes
    }

    /// Encrypt a plaintext payload (Opus audio frame).
    ///
    /// Returns the ciphertext with the 16-byte Poly1305 tag appended.
    /// The nonce is derived from the RTP header.
    pub fn encrypt(&self, rtp_header: &[u8; 12], plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
        let nonce_bytes = Self::derive_nonce(rtp_header);
        let nonce = Nonce::from_slice(&nonce_bytes);
        self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))
    }

    /// Decrypt a ciphertext payload (received voice audio).
    ///
    /// Expects ciphertext with 16-byte Poly1305 tag appended.
    /// The nonce is derived from the RTP header.
    pub fn decrypt(&self, rtp_header: &[u8; 12], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
        let nonce_bytes = Self::derive_nonce(rtp_header);
        let nonce = Nonce::from_slice(&nonce_bytes);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let key = [42u8; 32];
        let cipher = VoiceCipher::new(&key);
        let header = [0x80, 0x78, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01];
        let plaintext = b"hello voice world";

        let encrypted = cipher.encrypt(&header, plaintext).unwrap();
        let decrypted = cipher.decrypt(&header, &encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }
}
