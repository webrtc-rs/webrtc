use aes::cipher::generic_array::GenericArray;
use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use bytes::{BufMut, Bytes};
use rtcp::header::{HEADER_LENGTH, SSRC_LENGTH};
use subtle::ConstantTimeEq;
use util::marshal::*;

use super::{Cipher, CipherInner};
use crate::error::{Error, Result};
use crate::key_derivation::*;
use crate::protection_profile::ProtectionProfile;

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

pub(crate) struct CipherAesCmHmacSha1 {
    inner: CipherInner,
    srtp_session_key: Vec<u8>,
    srtcp_session_key: Vec<u8>,
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

        Ok(CipherAesCmHmacSha1 {
            inner,
            srtp_session_key,
            srtcp_session_key,
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
        let mut writer = Vec::with_capacity(plaintext.len() + self.rtp_auth_tag_len());

        // Write the plaintext to the destination buffer.
        writer.extend_from_slice(plaintext);

        // Encrypt the payload
        let counter = generate_counter(
            header.sequence_number,
            roc,
            header.ssrc,
            &self.inner.srtp_session_salt,
        );
        let key = GenericArray::from_slice(&self.srtp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);
        stream.apply_keystream(&mut writer[header.marshal_size()..]);

        // Generate the auth tag.
        let auth_tag = &self.inner.generate_srtp_auth_tag(&writer, roc)[..self.rtp_auth_tag_len()];
        writer.extend(auth_tag);

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
        writer.extend_from_slice(cipher_text);

        // Decrypt the ciphertext for the payload.
        let counter = generate_counter(
            header.sequence_number,
            roc,
            header.ssrc,
            &self.inner.srtp_session_salt,
        );

        let key = GenericArray::from_slice(&self.srtp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);
        stream.seek(0);
        stream.apply_keystream(&mut writer[header.marshal_size()..]);

        Ok(Bytes::from(writer))
    }

    fn encrypt_rtcp(&mut self, decrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        let mut writer =
            Vec::with_capacity(decrypted.len() + SRTCP_INDEX_SIZE + self.rtcp_auth_tag_len());

        // Write the decrypted to the destination buffer.
        writer.extend_from_slice(decrypted);

        // Encrypt everything after header
        let counter = generate_counter(
            (srtcp_index & 0xFFFF) as u16,
            (srtcp_index >> 16) as u32,
            ssrc,
            &self.inner.srtcp_session_salt,
        );

        let key = GenericArray::from_slice(&self.srtcp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);

        stream.apply_keystream(&mut writer[HEADER_LENGTH + SSRC_LENGTH..]);

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

        writer.extend_from_slice(&encrypted[0..tail_offset]);

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

        let counter = generate_counter(
            (srtcp_index & 0xFFFF) as u16,
            (srtcp_index >> 16) as u32,
            ssrc,
            &self.inner.srtcp_session_salt,
        );

        let key = GenericArray::from_slice(&self.srtcp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);

        stream.seek(0);
        stream.apply_keystream(&mut writer[HEADER_LENGTH + SSRC_LENGTH..]);

        Ok(Bytes::from(writer))
    }
}
