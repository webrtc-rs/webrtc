use super::*;

use byteorder::{BigEndian, ReadBytesExt};
use std::io::BufReader;

pub(crate) const MAX_SRTCP_INDEX: usize = 0x7FFFFFFF;

impl Context {
    /// DecryptRTCP decrypts a RTCP packet with an encrypted payload
    pub fn decrypt_rtcp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        {
            let mut reader = BufReader::new(encrypted);
            rtcp::header::Header::unmarshal(&mut reader)?;
        }

        let index = self.cipher.get_rtcp_index(encrypted);
        let ssrc = {
            let mut reader = BufReader::new(&encrypted[4..]);
            reader.read_u32::<BigEndian>()?
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
    pub fn encrypt_rtcp(&mut self, decrypted: &[u8]) -> Result<Vec<u8>, Error> {
        {
            let mut reader = BufReader::new(decrypted);
            rtcp::header::Header::unmarshal(&mut reader)?;
        }

        let ssrc = {
            let mut reader = BufReader::new(&decrypted[4..]);
            reader.read_u32::<BigEndian>()?
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

        Ok(self.cipher.encrypt_rtcp(decrypted, index, ssrc)?)
    }
}
