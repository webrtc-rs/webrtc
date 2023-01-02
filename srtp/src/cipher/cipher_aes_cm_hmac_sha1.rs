use super::Cipher;
use crate::error::Result;
use crate::{error::Error, key_derivation::*, protection_profile::*};
use util::marshal::*;

use aes::cipher::generic_array::GenericArray;
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use bytes::{BufMut, Bytes, BytesMut};
use ctr::cipher::{NewCipher, StreamCipher, StreamCipherSeek};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::io::BufWriter;
use subtle::ConstantTimeEq;

type HmacSha1 = Hmac<Sha1>;
type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

pub const CIPHER_AES_CM_HMAC_SHA1AUTH_TAG_LEN: usize = 10;

pub(crate) struct CipherAesCmHmacSha1 {
    srtp_session_key: Vec<u8>,
    srtp_session_salt: Vec<u8>,
    srtp_session_auth: HmacSha1,
    //srtp_session_auth_tag: Vec<u8>,
    srtcp_session_key: Vec<u8>,
    srtcp_session_salt: Vec<u8>,
    srtcp_session_auth: HmacSha1,
    //srtcp_session_auth_tag: Vec<u8>,
}

impl CipherAesCmHmacSha1 {
    pub fn new(master_key: &[u8], master_salt: &[u8]) -> Result<Self> {
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

        let srtp_session_salt = aes_cm_key_derivation(
            LABEL_SRTP_SALT,
            master_key,
            master_salt,
            0,
            master_salt.len(),
        )?;
        let srtcp_session_salt = aes_cm_key_derivation(
            LABEL_SRTCP_SALT,
            master_key,
            master_salt,
            0,
            master_salt.len(),
        )?;

        let auth_key_len = ProtectionProfile::Aes128CmHmacSha1_80.auth_key_len();

        let srtp_session_auth_tag = aes_cm_key_derivation(
            LABEL_SRTP_AUTHENTICATION_TAG,
            master_key,
            master_salt,
            0,
            auth_key_len,
        )?;
        let srtcp_session_auth_tag = aes_cm_key_derivation(
            LABEL_SRTCP_AUTHENTICATION_TAG,
            master_key,
            master_salt,
            0,
            auth_key_len,
        )?;

        let srtp_session_auth = HmacSha1::new_from_slice(&srtp_session_auth_tag)
            .map_err(|e| Error::Other(e.to_string()))?;
        let srtcp_session_auth = HmacSha1::new_from_slice(&srtcp_session_auth_tag)
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(CipherAesCmHmacSha1 {
            srtp_session_key,
            srtp_session_salt,
            srtp_session_auth,
            //srtp_session_auth_tag,
            srtcp_session_key,
            srtcp_session_salt,
            srtcp_session_auth,
            //srtcp_session_auth_tag,
        })
    }

    /// https://tools.ietf.org/html/rfc3711#section-4.2
    /// In the case of SRTP, M SHALL consist of the Authenticated
    /// Portion of the packet (as specified in Figure 1) concatenated with
    /// the roc, M = Authenticated Portion || roc;
    ///
    /// The pre-defined authentication transform for SRTP is HMAC-SHA1
    /// [RFC2104].  With HMAC-SHA1, the SRTP_PREFIX_LENGTH (Figure 3) SHALL
    /// be 0.  For SRTP (respectively SRTCP), the HMAC SHALL be applied to
    /// the session authentication key and M as specified above, i.e.,
    /// HMAC(k_a, M).  The HMAC output SHALL then be truncated to the n_tag
    /// left-most bits.
    /// - Authenticated portion of the packet is everything BEFORE MKI
    /// - k_a is the session message authentication key
    /// - n_tag is the bit-length of the output authentication tag
    fn generate_srtp_auth_tag(&mut self, buf: &[u8], roc: u32) -> Result<Vec<u8>> {
        self.srtp_session_auth.reset();

        self.srtp_session_auth.update(buf);

        // For SRTP only, we need to hash the rollover counter as well.
        let mut roc_buf: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(roc_buf.as_mut());
            writer.write_u32::<BigEndian>(roc)?;
        }

        self.srtp_session_auth.update(&roc_buf);

        let result = self.srtp_session_auth.clone().finalize();
        let code_bytes = result.into_bytes();

        // Truncate the hash to the first AUTH_TAG_SIZE bytes.
        Ok(code_bytes[0..self.auth_tag_len()].to_vec())
    }

    /// https://tools.ietf.org/html/rfc3711#section-4.2
    ///
    /// The pre-defined authentication transform for SRTP is HMAC-SHA1
    /// [RFC2104].  With HMAC-SHA1, the SRTP_PREFIX_LENGTH (Figure 3) SHALL
    /// be 0.  For SRTP (respectively SRTCP), the HMAC SHALL be applied to
    /// the session authentication key and M as specified above, i.e.,
    /// HMAC(k_a, M).  The HMAC output SHALL then be truncated to the n_tag
    /// left-most bits.
    /// - Authenticated portion of the packet is everything BEFORE MKI
    /// - k_a is the session message authentication key
    /// - n_tag is the bit-length of the output authentication tag
    fn generate_srtcp_auth_tag(&mut self, buf: &[u8]) -> Vec<u8> {
        self.srtcp_session_auth.reset();

        self.srtcp_session_auth.update(buf);

        let result = self.srtcp_session_auth.clone().finalize();
        let code_bytes = result.into_bytes();

        // Truncate the hash to the first AUTH_TAG_SIZE bytes.
        code_bytes[0..self.auth_tag_len()].to_vec()
    }
}

impl Cipher for CipherAesCmHmacSha1 {
    fn auth_tag_len(&self) -> usize {
        CIPHER_AES_CM_HMAC_SHA1AUTH_TAG_LEN
    }

    fn get_rtcp_index(&self, input: &[u8]) -> usize {
        let tail_offset = input.len() - (self.auth_tag_len() + SRTCP_INDEX_SIZE);
        (BigEndian::read_u32(&input[tail_offset..tail_offset + SRTCP_INDEX_SIZE]) & !(1 << 31))
            as usize
    }

    fn encrypt_rtp(
        &mut self,
        payload: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes> {
        let mut writer =
            BytesMut::with_capacity(header.marshal_size() + payload.len() + self.auth_tag_len());

        // Copy the header unencrypted.
        let data = header.marshal()?;
        writer.extend(data);

        // Write the plaintext header to the destination buffer.
        writer.extend_from_slice(payload);

        // Encrypt the payload
        let counter = generate_counter(
            header.sequence_number,
            roc,
            header.ssrc,
            &self.srtp_session_salt,
        )?;
        let key = GenericArray::from_slice(&self.srtp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);
        let payload_offset = header.marshal_size();
        stream.apply_keystream(&mut writer[payload_offset..]);

        // Generate the auth tag.
        let auth_tag = self.generate_srtp_auth_tag(&writer, roc)?;
        writer.extend(auth_tag);

        Ok(writer.freeze())
    }

    fn decrypt_rtp(
        &mut self,
        encrypted: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes> {
        if encrypted.len() < self.auth_tag_len() {
            return Err(Error::SrtpTooSmall(encrypted.len(), self.auth_tag_len()));
        }

        let mut writer = BytesMut::with_capacity(encrypted.len() - self.auth_tag_len());

        // Split the auth tag and the cipher text into two parts.
        let actual_tag = &encrypted[encrypted.len() - self.auth_tag_len()..];
        let cipher_text = &encrypted[..encrypted.len() - self.auth_tag_len()];

        // Generate the auth tag we expect to see from the ciphertext.
        let expected_tag = self.generate_srtp_auth_tag(cipher_text, roc)?;

        // See if the auth tag actually matches.
        // We use a constant time comparison to prevent timing attacks.
        if actual_tag.ct_eq(&expected_tag).unwrap_u8() != 1 {
            return Err(Error::RtpFailedToVerifyAuthTag);
        }

        // Write cipher_text to the destination buffer.
        writer.extend_from_slice(cipher_text);

        // Decrypt the ciphertext for the payload.
        let counter = generate_counter(
            header.sequence_number,
            roc,
            header.ssrc,
            &self.srtp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);
        let payload_offset = header.marshal_size();
        stream.seek(0);
        stream.apply_keystream(&mut writer[payload_offset..]);

        Ok(writer.freeze())
    }

    fn encrypt_rtcp(&mut self, decrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        let mut writer =
            BytesMut::with_capacity(decrypted.len() + SRTCP_INDEX_SIZE + self.auth_tag_len());

        // Write the decrypted to the destination buffer.
        writer.extend_from_slice(decrypted);

        // Encrypt everything after header
        let counter = generate_counter(
            (srtcp_index & 0xFFFF) as u16,
            (srtcp_index >> 16) as u32,
            ssrc,
            &self.srtcp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtcp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);

        stream.apply_keystream(
            &mut writer[rtcp::header::HEADER_LENGTH + rtcp::header::SSRC_LENGTH..],
        );

        // Add SRTCP index and set Encryption bit
        writer.put_u32(srtcp_index as u32 | (1u32 << 31));

        // Generate the auth tag.
        let auth_tag = self.generate_srtcp_auth_tag(&writer);
        writer.extend(auth_tag);

        Ok(writer.freeze())
    }

    fn decrypt_rtcp(&mut self, encrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        if encrypted.len() < self.auth_tag_len() + SRTCP_INDEX_SIZE {
            return Err(Error::SrtcpTooSmall(
                encrypted.len(),
                self.auth_tag_len() + SRTCP_INDEX_SIZE,
            ));
        }

        let tail_offset = encrypted.len() - (self.auth_tag_len() + SRTCP_INDEX_SIZE);

        let mut writer = BytesMut::with_capacity(tail_offset);

        writer.extend_from_slice(&encrypted[0..tail_offset]);

        let is_encrypted = encrypted[tail_offset] >> 7;
        if is_encrypted == 0 {
            return Ok(writer.freeze());
        }

        // Split the auth tag and the cipher text into two parts.
        let actual_tag = &encrypted[encrypted.len() - self.auth_tag_len()..];
        let cipher_text = &encrypted[..encrypted.len() - self.auth_tag_len()];

        // Generate the auth tag we expect to see from the ciphertext.
        let expected_tag = self.generate_srtcp_auth_tag(cipher_text);

        // See if the auth tag actually matches.
        // We use a constant time comparison to prevent timing attacks.
        if actual_tag.ct_eq(&expected_tag).unwrap_u8() != 1 {
            return Err(Error::RtcpFailedToVerifyAuthTag);
        }

        let counter = generate_counter(
            (srtcp_index & 0xFFFF) as u16,
            (srtcp_index >> 16) as u32,
            ssrc,
            &self.srtcp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtcp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(key, nonce);

        stream.seek(0);
        stream.apply_keystream(
            &mut writer[rtcp::header::HEADER_LENGTH + rtcp::header::SSRC_LENGTH..],
        );

        Ok(writer.freeze())
    }
}
