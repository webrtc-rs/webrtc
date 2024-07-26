use bytes::{BufMut, Bytes};
use openssl::cipher_ctx::CipherCtx;
use rtcp::header::{HEADER_LENGTH, SSRC_LENGTH};
use subtle::ConstantTimeEq;
use util::marshal::*;

use super::{Cipher, CipherInner};
use crate::protection_profile::ProtectionProfile;
use crate::{
    error::{Error, Result},
    key_derivation::*,
};

pub(crate) struct CipherAesCmHmacSha1 {
    inner: CipherInner,
    rtp_ctx: CipherCtx,
    rtcp_ctx: CipherCtx,
}

impl CipherAesCmHmacSha1 {
    pub fn new(profile: ProtectionProfile, master_key: &[u8], master_salt: &[u8]) -> Result<Self> {
        let inner = CipherInner::new(profile, master_key, master_salt)?;

        let srtp_session_key = aes_cm_key_derivation(
            LABEL_SRTP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;
        let srtcp_session_key = aes_cm_key_derivation(
            LABEL_SRTCP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        let t = openssl::cipher::Cipher::aes_128_ctr();
        let mut rtp_ctx = CipherCtx::new().map_err(|e| Error::Other(e.to_string()))?;
        rtp_ctx
            .encrypt_init(Some(t), Some(&srtp_session_key[..]), None)
            .map_err(|e| Error::Other(e.to_string()))?;

        let t = openssl::cipher::Cipher::aes_128_ctr();
        let mut rtcp_ctx = CipherCtx::new().map_err(|e| Error::Other(e.to_string()))?;
        rtcp_ctx
            .encrypt_init(Some(t), Some(&srtcp_session_key[..]), None)
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(Self {
            inner,
            rtp_ctx,
            rtcp_ctx,
        })
    }
}

impl Cipher for CipherAesCmHmacSha1 {
    /// Get RTP authenticated tag length.
    fn rtp_auth_tag_len(&self) -> usize {
        self.inner.profile.rtp_auth_tag_len()
    }

    /// Get RTCP authenticated tag length.
    fn rtcp_auth_tag_len(&self) -> usize {
        self.inner.profile.rtcp_auth_tag_len()
    }

    /// Get AEAD auth key length of the cipher.
    fn aead_auth_tag_len(&self) -> usize {
        self.inner.profile.aead_auth_tag_len()
    }

    fn get_rtcp_index(&self, input: &[u8]) -> usize {
        self.inner.get_rtcp_index(input)
    }

    fn encrypt_rtp(
        &mut self,
        plaintext: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes> {
        let header_len = header.marshal_size();
        let mut writer = Vec::with_capacity(plaintext.len() + self.rtp_auth_tag_len());

        // Copy the header unencrypted.
        writer.extend_from_slice(&plaintext[..header_len]);

        // Encrypt the payload
        let nonce = generate_counter(
            header.sequence_number,
            roc,
            header.ssrc,
            &self.inner.srtp_session_salt,
        );
        writer.resize(plaintext.len(), 0);
        self.rtp_ctx.encrypt_init(None, None, Some(&nonce)).unwrap();
        let count = self
            .rtp_ctx
            .cipher_update(&plaintext[header_len..], Some(&mut writer[header_len..]))
            .unwrap();
        self.rtp_ctx
            .cipher_final(&mut writer[header_len + count..])
            .unwrap();

        // Generate and write the auth tag.
        let auth_tag = &self.inner.generate_srtp_auth_tag(&writer, roc)[..self.rtp_auth_tag_len()];
        writer.extend_from_slice(auth_tag);

        Ok(Bytes::from(writer))
    }

    fn decrypt_rtp(
        &mut self,
        encrypted: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes> {
        let encrypted_len = encrypted.len();
        if encrypted_len < self.rtp_auth_tag_len() {
            return Err(Error::SrtpTooSmall(encrypted_len, self.rtp_auth_tag_len()));
        }
        let header_len = header.marshal_size();

        let mut writer = Vec::with_capacity(encrypted_len - self.rtp_auth_tag_len());

        // Split the auth tag and the cipher text into two parts.
        let actual_tag = &encrypted[encrypted_len - self.rtp_auth_tag_len()..];
        let cipher_text = &encrypted[..encrypted_len - self.rtp_auth_tag_len()];

        // Generate the auth tag we expect to see from the ciphertext.
        let expected_tag =
            &self.inner.generate_srtp_auth_tag(cipher_text, roc)[..self.rtp_auth_tag_len()];

        // See if the auth tag actually matches.
        // We use a constant time comparison to prevent timing attacks.
        if actual_tag.ct_eq(expected_tag).unwrap_u8() != 1 {
            return Err(Error::RtpFailedToVerifyAuthTag);
        }

        // Write cipher_text to the destination buffer.
        writer.extend_from_slice(&cipher_text[..header_len]);

        // Decrypt the ciphertext for the payload.
        let nonce = generate_counter(
            header.sequence_number,
            roc,
            header.ssrc,
            &self.inner.srtp_session_salt,
        );

        writer.resize(encrypted_len - self.rtp_auth_tag_len(), 0);
        self.rtp_ctx.decrypt_init(None, None, Some(&nonce)).unwrap();
        let count = self
            .rtp_ctx
            .cipher_update(&cipher_text[header_len..], Some(&mut writer[header_len..]))
            .unwrap();
        self.rtp_ctx
            .cipher_final(&mut writer[header_len + count..])
            .unwrap();

        Ok(Bytes::from(writer))
    }

    fn encrypt_rtcp(&mut self, decrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        let decrypted_len = decrypted.len();

        let mut writer =
            Vec::with_capacity(decrypted_len + SRTCP_INDEX_SIZE + self.rtcp_auth_tag_len());

        // Write the decrypted to the destination buffer.
        writer.extend_from_slice(&decrypted[..HEADER_LENGTH + SSRC_LENGTH]);

        // Encrypt everything after header
        let nonce = generate_counter(
            (srtcp_index & 0xFFFF) as u16,
            (srtcp_index >> 16) as u32,
            ssrc,
            &self.inner.srtcp_session_salt,
        );

        writer.resize(decrypted_len, 0);
        self.rtcp_ctx
            .encrypt_init(None, None, Some(&nonce))
            .unwrap();
        let count = self
            .rtcp_ctx
            .cipher_update(
                &decrypted[HEADER_LENGTH + SSRC_LENGTH..],
                Some(&mut writer[HEADER_LENGTH + SSRC_LENGTH..]),
            )
            .unwrap();
        self.rtcp_ctx
            .cipher_final(&mut writer[HEADER_LENGTH + SSRC_LENGTH + count..])
            .unwrap();

        // Add SRTCP index and set Encryption bit
        writer.put_u32(srtcp_index as u32 | (1u32 << 31));

        // Generate the auth tag.
        let auth_tag = &self.inner.generate_srtcp_auth_tag(&writer)[..self.rtcp_auth_tag_len()];
        writer.extend(auth_tag);

        Ok(Bytes::from(writer))
    }

    fn decrypt_rtcp(&mut self, encrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        let encrypted_len = encrypted.len();

        if encrypted_len < self.rtcp_auth_tag_len() + SRTCP_INDEX_SIZE {
            return Err(Error::SrtcpTooSmall(
                encrypted_len,
                self.rtcp_auth_tag_len() + SRTCP_INDEX_SIZE,
            ));
        }

        let tail_offset = encrypted_len - (self.rtcp_auth_tag_len() + SRTCP_INDEX_SIZE);

        let mut writer = Vec::with_capacity(tail_offset);

        writer.extend_from_slice(&encrypted[..HEADER_LENGTH + SSRC_LENGTH]);

        let is_encrypted = encrypted[tail_offset] >> 7;
        if is_encrypted == 0 {
            return Ok(Bytes::from(writer));
        }

        // Split the auth tag and the cipher text into two parts.
        let actual_tag = &encrypted[encrypted_len - self.rtcp_auth_tag_len()..];
        if actual_tag.len() != self.rtcp_auth_tag_len() {
            return Err(Error::RtcpInvalidLengthAuthTag(
                actual_tag.len(),
                self.rtcp_auth_tag_len(),
            ));
        }

        let cipher_text = &encrypted[..encrypted_len - self.rtcp_auth_tag_len()];

        // Generate the auth tag we expect to see from the ciphertext.
        let expected_tag =
            &self.inner.generate_srtcp_auth_tag(cipher_text)[..self.rtcp_auth_tag_len()];

        // See if the auth tag actually matches.
        // We use a constant time comparison to prevent timing attacks.
        if actual_tag.ct_eq(expected_tag).unwrap_u8() != 1 {
            return Err(Error::RtcpFailedToVerifyAuthTag);
        }

        let nonce = generate_counter(
            (srtcp_index & 0xFFFF) as u16,
            (srtcp_index >> 16) as u32,
            ssrc,
            &self.inner.srtcp_session_salt,
        );

        writer.resize(tail_offset, 0);
        self.rtcp_ctx
            .decrypt_init(None, None, Some(&nonce))
            .unwrap();
        let count = self
            .rtcp_ctx
            .cipher_update(
                &encrypted[HEADER_LENGTH + SSRC_LENGTH..tail_offset],
                Some(&mut writer[HEADER_LENGTH + SSRC_LENGTH..]),
            )
            .unwrap();
        self.rtcp_ctx
            .cipher_final(&mut writer[HEADER_LENGTH + SSRC_LENGTH + count..])
            .unwrap();

        Ok(Bytes::from(writer))
    }
}
