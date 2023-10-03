use bytes::Bytes;
use util::marshal::*;

use super::*;
use crate::error::Result;

impl Context {
    /// DecryptRTCP decrypts a RTCP packet with an encrypted payload
    pub fn decrypt_rtcp(&mut self, encrypted: &[u8]) -> Result<Bytes> {
        let mut buf = encrypted;
        rtcp::header::Header::unmarshal(&mut buf)?;

        let index = self.cipher.get_rtcp_index(encrypted);
        let ssrc = u32::from_be_bytes([encrypted[4], encrypted[5], encrypted[6], encrypted[7]]);

        if let Some(replay_detector) = &mut self.get_srtcp_ssrc_state(ssrc).replay_detector {
            if !replay_detector.check(index as u64) {
                return Err(Error::SrtcpSsrcDuplicated(ssrc, index));
            }
        }

        let dst = self.cipher.decrypt_rtcp(encrypted, index, ssrc)?;

        if let Some(replay_detector) = &mut self.get_srtcp_ssrc_state(ssrc).replay_detector {
            replay_detector.accept();
        }

        Ok(dst)
    }

    /// EncryptRTCP marshals and encrypts an RTCP packet, writing to the dst buffer provided.
    /// If the dst buffer does not have the capacity to hold `len(plaintext) + 14` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtcp(&mut self, decrypted: &[u8]) -> Result<Bytes> {
        let mut buf = decrypted;
        rtcp::header::Header::unmarshal(&mut buf)?;

        let ssrc = u32::from_be_bytes([decrypted[4], decrypted[5], decrypted[6], decrypted[7]]);

        let index = {
            let state = self.get_srtcp_ssrc_state(ssrc);
            state.srtcp_index += 1;
            if state.srtcp_index > MAX_SRTCP_INDEX {
                state.srtcp_index = 0;
            }
            state.srtcp_index
        };

        self.cipher.encrypt_rtcp(decrypted, index, ssrc)
    }
}
