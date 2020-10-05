use super::*;

use std::io::BufReader;

pub(crate) const MAX_SRTCP_INDEX: u64 = 0x7FFFFFFF;

impl Context {
    // DecryptRTCP decrypts a RTCP packet with an encrypted payload
    pub fn decrypt_rtcp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(encrypted);
        rtcp::header::Header::unmarshal(&mut reader)?;
        self.cipher.decrypt_rtcp(encrypted, 0, 0)
    }

    // EncryptRTCP marshals and encrypts an RTCP packet, writing to the dst buffer provided.
    // If the dst buffer does not have the capacity to hold `len(plaintext) + 14` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtcp(&mut self, decrypted: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(decrypted);
        rtcp::header::Header::unmarshal(&mut reader)?;
        self.cipher.encrypt_rtcp(decrypted, 0, 0)
    }
}
