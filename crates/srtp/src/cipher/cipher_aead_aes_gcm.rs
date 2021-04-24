use aes_gcm::{
    aead::{generic_array::GenericArray, Aead, NewAead, Nonce, Payload},
    Aes128Gcm,
};
use byteorder::{BigEndian, ByteOrder};
use bytes::{Bytes, BytesMut};
use rtp::packetizer::Marshaller;

use super::Cipher;
use crate::{context, error::Error, key_derivation};

pub(crate) const CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN: usize = 16;
const RTCP_ENCRYPTION_FLAG: u8 = 0x80;

/// AEAD Cipher based on AES.
pub(crate) struct CipherAeadAesGcm {
    srtp_cipher: aes_gcm::Aes128Gcm,
    srtcp_cipher: aes_gcm::Aes128Gcm,
    srtp_session_salt: Vec<u8>,
    srtcp_session_salt: Vec<u8>,
}

impl Cipher for CipherAeadAesGcm {
    fn auth_tag_len(&self) -> usize {
        CIPHER_AEAD_AES_GCM_AUTH_TAG_LEN
    }

    fn encrypt_rtp(
        &mut self,
        payload: &[u8],
        header: &rtp::header::Header,
        roc: u32,
    ) -> Result<Bytes, Error> {
        let mut writer =
            BytesMut::with_capacity(header.marshal_size() + payload.len() + self.auth_tag_len());

        header.marshal_to(&mut writer)?;

        let nonce = self.rtp_initialization_vector(header, roc);

        let encrypted = self.srtp_cipher.encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &payload,
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
    ) -> Result<Vec<u8>, Error> {
        let nonce = self.rtp_initialization_vector(header, roc);

        let decrypted_msg: Vec<u8> = self.srtp_cipher.decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext[header.payload_offset..],
                aad: &ciphertext[..header.payload_offset],
            },
        )?;

        let mut decrypted_msg = [vec![0; header.payload_offset], decrypted_msg].concat();

        decrypted_msg[..header.payload_offset]
            .copy_from_slice(&ciphertext[..header.payload_offset]);

        Ok(decrypted_msg)
    }

    fn encrypt_rtcp(
        &mut self,
        decrypted: &[u8],
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<Vec<u8>, Error> {
        let iv = self.rtcp_initialization_vector(srtcp_index, ssrc);

        let aad = self.rtcp_additional_authenticated_data(decrypted, srtcp_index);

        let encrypted_data = self.srtcp_cipher.encrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: &decrypted[8..],
                aad: &aad,
            },
        )?;

        let mut encrypted_data = [vec![0; 8], encrypted_data].concat();

        encrypted_data[..8].copy_from_slice(&decrypted[..8]);
        encrypted_data.append(&mut aad[8..].to_vec());

        Ok(encrypted_data)
    }

    fn decrypt_rtcp(
        &mut self,
        encrypted: &[u8],
        srtcp_index: usize,
        ssrc: u32,
    ) -> Result<Vec<u8>, Error> {
        let nonce = self.rtcp_initialization_vector(srtcp_index, ssrc);

        let aad = self.rtcp_additional_authenticated_data(&encrypted, srtcp_index);

        let decrypted_data = self.srtcp_cipher.decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &encrypted[8..(encrypted.len() - context::SRTCP_INDEX_SIZE)],
                aad: &aad,
            },
        )?;

        let decrypted_data = [encrypted[..8].to_vec(), decrypted_data].concat();
        Ok(decrypted_data)
    }

    fn get_rtcp_index(&self, input: &[u8]) -> usize {
        let pos = input.len() - 4;
        let val = BigEndian::read_u32(&input[pos..]);

        (val & !((RTCP_ENCRYPTION_FLAG as u32) << 24)) as usize
    }
}

impl CipherAeadAesGcm {
    /// Create a new AEAD instance.
    pub(crate) fn new(master_key: &[u8], master_salt: &[u8]) -> Result<CipherAeadAesGcm, Error> {
        let srtp_session_key = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        let srtp_block = GenericArray::from_slice(&srtp_session_key);

        let srtp_cipher = Aes128Gcm::new(srtp_block);

        let srtcp_session_key = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTCP_ENCRYPTION,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        let srtcp_block = GenericArray::from_slice(&srtcp_session_key);

        let srtcp_cipher = Aes128Gcm::new(srtcp_block);

        let srtp_session_salt = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTP_SALT,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        let srtcp_session_salt = key_derivation::aes_cm_key_derivation(
            context::LABEL_SRTCP_SALT,
            master_key,
            master_salt,
            0,
            master_key.len(),
        )?;

        Ok(CipherAeadAesGcm {
            srtp_cipher,
            srtcp_cipher,
            srtp_session_salt,
            srtcp_session_salt,
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
