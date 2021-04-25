use super::*;

use bytes::{Buf, Bytes};

impl Context {
    /// DecryptRTCP decrypts a RTCP packet with an encrypted payload
    pub fn decrypt_rtcp(&mut self, encrypted: &Bytes) -> Result<Bytes, Error> {
        rtcp::header::Header::unmarshal(encrypted)?;

        let index = self.cipher.get_rtcp_index(encrypted);
        let ssrc = {
            let reader = &mut encrypted.slice(4..);
            reader.get_u32()
        };

        {
            if let Some(state) = self.get_srtcp_ssrc_state(ssrc) {
                if let Some(replay_detector) = &mut state.replay_detector {
                    if !replay_detector.check(index as u64) {
                        return Err(Error::SrtcpSsrcDuplicated(ssrc, index));
                    }
                }
            } else {
                return Err(Error::SsrcMissingFromSrtcp(ssrc));
            }
        }

        let dst = self.cipher.decrypt_rtcp(encrypted, index, ssrc)?;

        {
            if let Some(state) = self.get_srtcp_ssrc_state(ssrc) {
                if let Some(replay_detector) = &mut state.replay_detector {
                    replay_detector.accept();
                }
            }
        }

        Ok(dst)
    }

    /// EncryptRTCP marshals and encrypts an RTCP packet, writing to the dst buffer provided.
    /// If the dst buffer does not have the capacity to hold `len(plaintext) + 14` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtcp(&mut self, decrypted: &Bytes) -> Result<Bytes, Error> {
        rtcp::header::Header::unmarshal(decrypted)?;

        let ssrc = {
            let reader = &mut decrypted.slice(4..);
            reader.get_u32()
        };

        let index;
        {
            if let Some(state) = self.get_srtcp_ssrc_state(ssrc) {
                state.srtcp_index += 1;
                if state.srtcp_index > MAX_SRTCP_INDEX {
                    state.srtcp_index = 0;
                }
                index = state.srtcp_index;
            } else {
                return Err(Error::SsrcMissingFromSrtcp(ssrc));
            }
        }

        self.cipher.encrypt_rtcp(decrypted, index, ssrc)
    }
}
