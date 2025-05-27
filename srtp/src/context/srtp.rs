use bytes::Bytes;
use util::marshal::*;

use super::*;
use crate::error::Result;

impl Context {
    pub fn decrypt_rtp_with_header(
        &mut self,
        encrypted: &[u8],
        header: &rtp::header::Header,
    ) -> Result<Bytes> {
        let auth_tag_len = self.cipher.rtp_auth_tag_len();
        if encrypted.len() < header.marshal_size() + auth_tag_len {
            return Err(Error::ErrTooShortRtp);
        }

        let state = self.get_srtp_ssrc_state(header.ssrc);
        let (roc, diff, _) = state.next_rollover_count(header.sequence_number);
        if let Some(replay_detector) = &mut state.replay_detector {
            if !replay_detector.check(header.sequence_number as u64) {
                return Err(Error::SrtpSsrcDuplicated(
                    header.ssrc,
                    header.sequence_number,
                ));
            }
        }

        let dst = self.cipher.decrypt_rtp(encrypted, header, roc)?;
        {
            let state = self.get_srtp_ssrc_state(header.ssrc);
            if let Some(replay_detector) = &mut state.replay_detector {
                replay_detector.accept();
            }
            state.update_rollover_count(header.sequence_number, diff);
        }

        Ok(dst)
    }

    /// DecryptRTP decrypts a RTP packet with an encrypted payload
    pub fn decrypt_rtp(&mut self, encrypted: &[u8]) -> Result<Bytes> {
        let mut buf = encrypted;
        let header = rtp::header::Header::unmarshal(&mut buf)?;
        self.decrypt_rtp_with_header(encrypted, &header)
    }

    pub fn encrypt_rtp_with_header(
        &mut self,
        payload: &[u8],
        header: &rtp::header::Header,
    ) -> Result<Bytes> {
        let (roc, diff, ovf) = self
            .get_srtp_ssrc_state(header.ssrc)
            .next_rollover_count(header.sequence_number);
        if ovf {
            // ... when 2^48 SRTP packets or 2^31 SRTCP packets have been secured with the same key
            // (whichever occurs before), the key management MUST be called to provide new master key(s)
            // (previously stored and used keys MUST NOT be used again), or the session MUST be terminated.
            // https://www.rfc-editor.org/rfc/rfc3711#section-9.2
            return Err(Error::ErrExceededMaxPackets);
        }

        let dst = self.cipher.encrypt_rtp(payload, header, roc)?;

        self.get_srtp_ssrc_state(header.ssrc)
            .update_rollover_count(header.sequence_number, diff);

        Ok(dst)
    }

    /// EncryptRTP marshals and encrypts an RTP packet, writing to the dst buffer provided.
    /// If the dst buffer does not have the capacity to hold `len(plaintext) + 10` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtp(&mut self, plaintext: &[u8]) -> Result<Bytes> {
        let mut buf = plaintext;
        let header = rtp::header::Header::unmarshal(&mut buf)?;
        self.encrypt_rtp_with_header(plaintext, &header)
    }
}
