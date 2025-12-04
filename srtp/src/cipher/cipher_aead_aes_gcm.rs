use std::marker::PhantomData;

use aead::consts::{U12, U16};
use aes::cipher::{BlockEncrypt, BlockSizeUser, Unsigned};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{AesGcm, KeyInit, Nonce};
use byteorder::{BigEndian, ByteOrder};
use bytes::{Bytes, BytesMut};
use util::marshal::*;

use super::Cipher;
use crate::error::{Error, Result};
use crate::key_derivation::*;
use crate::protection_profile::ProtectionProfile;

pub const CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN: usize = 16;

const RTCP_ENCRYPTION_FLAG: u8 = 0x80;

/// AEAD Cipher based on AES.
pub(crate) struct CipherAeadAesGcm<AES, NonceSize = U12>
where
    NonceSize: Unsigned,
{
    profile: ProtectionProfile,
    srtp_cipher: aes_gcm::AesGcm<AES, NonceSize>,
    srtcp_cipher: aes_gcm::AesGcm<AES, NonceSize>,
    srtp_session_salt: Vec<u8>,
    srtcp_session_salt: Vec<u8>,
    _tag: PhantomData<AES>,
}

impl<AES, NS> Cipher for CipherAeadAesGcm<AES, NS>
where
    NS: Unsigned,
    AES: BlockEncrypt + KeyInit + BlockSizeUser<BlockSize = U16> + 'static,
    AesGcm<AES, NS>: Aead,
{
    fn rtp_auth_tag_len(&self) -> usize {
        self.profile.rtp_auth_tag_len()
    }

    /// Get RTCP authenticated tag length.
    fn rtcp_auth_tag_len(&self) -> usize {
        self.profile.rtcp_auth_tag_len()
    }

    /// Get AEAD auth key length of the cipher.
    fn aead_auth_tag_len(&self) -> usize {
        self.profile.aead_auth_tag_len()
    }

    fn encrypt_rtp(
        &mut self,
        payload: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes> {
        // Grow the given buffer to fit the output.
        let header_len = header.marshal_size();
        let mut writer = BytesMut::with_capacity(payload.len() + self.aead_auth_tag_len());

        // Copy header unencrypted.
        writer.extend_from_slice(&payload[..header_len]);

        let nonce = self.rtp_initialization_vector(header, roc);

        let encrypted = self.srtp_cipher.encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &payload[header_len..],
                aad: &writer,
            },
        )?;

        writer.extend(encrypted);
        Ok(writer.freeze())
    }

    fn decrypt_rtp(
        &mut self,
        ciphertext: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes> {
        if ciphertext.len() < self.aead_auth_tag_len() {
            return Err(Error::ErrFailedToVerifyAuthTag);
        }

        let nonce = self.rtp_initialization_vector(header, roc);
        let payload_offset = header.marshal_size();
        let decrypted_msg: Vec<u8> = self.srtp_cipher.decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext[payload_offset..],
                aad: &ciphertext[..payload_offset],
            },
        )?;

        let mut writer = BytesMut::with_capacity(payload_offset + decrypted_msg.len());
        writer.extend_from_slice(&ciphertext[..payload_offset]);
        writer.extend(decrypted_msg);

        Ok(writer.freeze())
    }

    fn encrypt_rtcp(&mut self, decrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        let iv = self.rtcp_initialization_vector(srtcp_index, ssrc);
        let aad = self.rtcp_additional_authenticated_data(decrypted, srtcp_index);

        let encrypted_data = self.srtcp_cipher.encrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: &decrypted[8..],
                aad: &aad,
            },
        )?;

        let mut writer = BytesMut::with_capacity(encrypted_data.len() + aad.len());
        writer.extend_from_slice(&decrypted[..8]);
        writer.extend(encrypted_data);
        writer.extend_from_slice(&aad[8..]);

        Ok(writer.freeze())
    }

    fn decrypt_rtcp(&mut self, encrypted: &[u8], srtcp_index: usize, ssrc: u32) -> Result<Bytes> {
        if encrypted.len() < self.aead_auth_tag_len() + SRTCP_INDEX_SIZE {
            return Err(Error::ErrFailedToVerifyAuthTag);
        }

        let nonce = self.rtcp_initialization_vector(srtcp_index, ssrc);
        let aad = self.rtcp_additional_authenticated_data(encrypted, srtcp_index);

        let decrypted_data = self.srtcp_cipher.decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &encrypted[8..(encrypted.len() - SRTCP_INDEX_SIZE)],
                aad: &aad,
            },
        )?;

        let mut writer = BytesMut::with_capacity(8 + decrypted_data.len());
        writer.extend_from_slice(&encrypted[..8]);
        writer.extend(decrypted_data);

        Ok(writer.freeze())
    }

    fn get_rtcp_index(&self, input: &[u8]) -> usize {
        let pos = input.len() - 4;
        let val = BigEndian::read_u32(&input[pos..]);

        (val & !((RTCP_ENCRYPTION_FLAG as u32) << 24)) as usize
    }
}

impl<AES, NS> CipherAeadAesGcm<AES, NS>
where
    NS: Unsigned,
    AES: BlockEncrypt + KeyInit + BlockSizeUser<BlockSize = U16> + 'static,
    AesGcm<AES, NS>: Aead,
{
    /// Create a new AEAD instance.
    pub(crate) fn new(
        profile: ProtectionProfile,
        master_key: &[u8],
        master_salt: &[u8],
    ) -> Result<CipherAeadAesGcm<AES>> {
        assert_eq!(profile.aead_auth_tag_len(), AES::block_size());
        assert_eq!(profile.key_len(), AES::key_size());
        assert_eq!(profile.salt_len(), master_salt.len());

        type Kdf = fn(u8, &[u8], &[u8], usize, usize) -> Result<Vec<u8>>;
        let kdf: Kdf = match profile {
            ProtectionProfile::AeadAes128Gcm => aes_cm_key_derivation,
            // AES_256_GCM must use AES_256_CM_PRF as per https://datatracker.ietf.org/doc/html/rfc7714#section-11
            ProtectionProfile::AeadAes256Gcm => aes_256_cm_key_derivation,
            _ => unreachable!(),
        };

        let srtp_session_key = kdf(
            LABEL_SRTP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        let srtp_block = GenericArray::from_slice(&srtp_session_key);

        let srtp_cipher = AesGcm::<AES, U12>::new(srtp_block);

        let srtcp_session_key = kdf(
            LABEL_SRTCP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        let srtcp_block = GenericArray::from_slice(&srtcp_session_key);

        let srtcp_cipher = AesGcm::<AES, U12>::new(srtcp_block);

        let srtp_session_salt = kdf(
            LABEL_SRTP_SALT,
            master_key,
            master_salt,
            0,
            master_salt.len(),
        )?;

        let srtcp_session_salt = kdf(
            LABEL_SRTCP_SALT,
            master_key,
            master_salt,
            0,
            master_salt.len(),
        )?;

        Ok(CipherAeadAesGcm {
            profile,
            srtp_cipher,
            srtcp_cipher,
            srtp_session_salt,
            srtcp_session_salt,
            _tag: PhantomData,
        })
    }

    /// The 12-octet IV used by AES-GCM SRTP is formed by first concatenating
    /// 2 octets of zeroes, the 4-octet SSRC, the 4-octet rollover counter
    /// (ROC), and the 2-octet sequence number (SEQ).  The resulting 12-octet
    /// value is then XORed to the 12-octet salt to form the 12-octet IV.
    ///
    /// https://tools.ietf.org/html/rfc7714#section-8.1
    pub(crate) fn rtp_initialization_vector(
        &self,
        header: &rtp::header::Header,
        roc: u32,
    ) -> Vec<u8> {
        let mut iv = vec![0u8; 12];
        BigEndian::write_u32(&mut iv[2..], header.ssrc);
        BigEndian::write_u32(&mut iv[6..], roc);
        BigEndian::write_u16(&mut iv[10..], header.sequence_number);

        for (i, v) in iv.iter_mut().enumerate() {
            *v ^= self.srtp_session_salt[i];
        }

        iv
    }

    /// The 12-octet IV used by AES-GCM SRTCP is formed by first
    /// concatenating 2 octets of zeroes, the 4-octet SSRC identifier,
    /// 2 octets of zeroes, a single "0" bit, and the 31-bit SRTCP index.
    /// The resulting 12-octet value is then XORed to the 12-octet salt to
    /// form the 12-octet IV.
    ///
    /// https://tools.ietf.org/html/rfc7714#section-9.1
    pub(crate) fn rtcp_initialization_vector(&self, srtcp_index: usize, ssrc: u32) -> Vec<u8> {
        let mut iv = vec![0u8; 12];

        BigEndian::write_u32(&mut iv[2..], ssrc);
        BigEndian::write_u32(&mut iv[8..], srtcp_index as u32);

        for (i, v) in iv.iter_mut().enumerate() {
            *v ^= self.srtcp_session_salt[i];
        }

        iv
    }

    /// In an SRTCP packet, a 1-bit Encryption flag is prepended to the
    /// 31-bit SRTCP index to form a 32-bit value we shall call the
    /// "ESRTCP word"
    ///
    /// https://tools.ietf.org/html/rfc7714#section-17
    pub(crate) fn rtcp_additional_authenticated_data(
        &self,
        rtcp_packet: &[u8],
        srtcp_index: usize,
    ) -> Vec<u8> {
        let mut aad = vec![0u8; 12];

        aad[..8].copy_from_slice(&rtcp_packet[..8]);

        BigEndian::write_u32(&mut aad[8..], srtcp_index as u32);

        aad[8] |= RTCP_ENCRYPTION_FLAG;
        aad
    }
}

#[cfg(test)]
mod tests {
    use aes::{Aes128, Aes256};

    use super::*;

    #[test]
    fn test_aead_aes_gcm_128() {
        let profile = ProtectionProfile::AeadAes128Gcm;
        let master_key = vec![0u8; profile.key_len()];
        let master_salt = vec![0u8; 12];

        let mut cipher =
            CipherAeadAesGcm::<Aes128>::new(profile, &master_key, &master_salt).unwrap();

        let header = rtp::header::Header {
            ssrc: 0x12345678,
            ..Default::default()
        };

        let payload = vec![0u8; 100];
        let encrypted = cipher.encrypt_rtp(&payload, &header, 0).unwrap();

        let decrypted = cipher.decrypt_rtp(&encrypted, &header, 0).unwrap();
        assert_eq!(&decrypted[..], &payload[..]);
    }

    #[test]
    fn test_aead_aes_gcm_256() {
        let profile = ProtectionProfile::AeadAes256Gcm;
        let master_key = vec![0u8; profile.key_len()];
        let master_salt = vec![0u8; 12];

        let mut cipher =
            CipherAeadAesGcm::<Aes256>::new(profile, &master_key, &master_salt).unwrap();

        let header = rtp::header::Header {
            ssrc: 0x12345678,
            ..Default::default()
        };

        let payload = vec![0u8; 100];
        let encrypted = cipher.encrypt_rtp(&payload, &header, 0).unwrap();

        let decrypted = cipher.decrypt_rtp(&encrypted, &header, 0).unwrap();
        assert_eq!(&decrypted[..], &payload[..]);
    }
}
