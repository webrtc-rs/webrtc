use super::*;

use rtp;

use std::io::BufReader;

impl Context {
    // DecryptRTP decrypts a RTP packet with an encrypted payload
    pub fn decrypt_rtp(&self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(encrypted);
        let rtp_packet = rtp::packet::Packet::unmarshal(&mut reader)?;

        Ok(vec![])
    }
}
