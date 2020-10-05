use super::*;

use std::io::BufReader;

impl Context {
    pub fn decrypt_rtp_with_header(
        &mut self,
        encrypted: &[u8],
        header: &rtp::header::Header,
    ) -> Result<Vec<u8>, Error> {
        self.cipher.decrypt_rtp(encrypted, header, 0)
    }

    // DecryptRTP decrypts a RTP packet with an encrypted payload
    pub fn decrypt_rtp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(encrypted);
        let header = rtp::header::Header::unmarshal(&mut reader)?;
        self.decrypt_rtp_with_header(encrypted, &header)
    }

    pub fn encrypt_rtp_with_header(
        &mut self,
        plaintext: &[u8],
        header: &rtp::header::Header,
    ) -> Result<Vec<u8>, Error> {
        self.cipher
            .encrypt_rtp(&plaintext[header.payload_offset..], header, 0)
    }

    // EncryptRTP marshals and encrypts an RTP packet, writing to the dst buffer provided.
    // If the dst buffer does not have the capacity to hold `len(plaintext) + 10` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtp(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(plaintext);
        let header = rtp::header::Header::unmarshal(&mut reader)?;
        self.encrypt_rtp_with_header(plaintext, &header)
    }
}
