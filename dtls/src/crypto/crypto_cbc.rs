// AES-CBC (Cipher Block Chaining)
// First historic block cipher for AES.
// CBC mode is insecure and must not be used. It’s been progressively deprecated and
// removed from SSL libraries.
// Introduced with TLS 1.0 year 2002. Superseded by GCM in TLS 1.2 year 2008.
// Removed in TLS 1.3 year 2018.
// RFC 3268 year 2002 https://tools.ietf.org/html/rfc3268

// https://github.com/RustCrypto/block-ciphers

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use p256::elliptic_curve::subtle::ConstantTimeEq;
use rand::Rng;
use std::io::Cursor;
use std::ops::Not;

use super::padding::DtlsPadding;
use crate::content::*;
use crate::error::*;
use crate::prf::*;
use crate::record_layer::record_layer_header::*;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

// State needed to handle encrypted input/output
#[derive(Clone)]
pub struct CryptoCbc {
    local_key: Vec<u8>,
    remote_key: Vec<u8>,
    write_mac: Vec<u8>,
    read_mac: Vec<u8>,
}

impl CryptoCbc {
    const BLOCK_SIZE: usize = 16;
    const MAC_SIZE: usize = 20;

    pub fn new(
        local_key: &[u8],
        local_mac: &[u8],
        remote_key: &[u8],
        remote_mac: &[u8],
    ) -> Result<Self> {
        Ok(CryptoCbc {
            local_key: local_key.to_vec(),
            write_mac: local_mac.to_vec(),

            remote_key: remote_key.to_vec(),
            read_mac: remote_mac.to_vec(),
        })
    }

    pub fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>> {
        let mut payload = raw[RECORD_LAYER_HEADER_SIZE..].to_vec();
        let raw = &raw[..RECORD_LAYER_HEADER_SIZE];

        // Generate + Append MAC
        let h = pkt_rlh;

        let mac = prf_mac(
            h.epoch,
            h.sequence_number,
            h.content_type,
            h.protocol_version,
            &payload,
            &self.write_mac,
        )?;
        payload.extend_from_slice(&mac);

        let mut iv: Vec<u8> = vec![0; Self::BLOCK_SIZE];
        rand::thread_rng().fill(iv.as_mut_slice());

        let write_cbc = Aes256CbcEnc::new_from_slices(&self.local_key, &iv)?;
        let encrypted = write_cbc.encrypt_padded_vec_mut::<DtlsPadding>(&payload);

        // Prepend unencrypte header with encrypted payload
        let mut r = vec![];
        r.extend_from_slice(raw);
        r.extend_from_slice(&iv);
        r.extend_from_slice(&encrypted);

        let r_len = (r.len() - RECORD_LAYER_HEADER_SIZE) as u16;
        r[RECORD_LAYER_HEADER_SIZE - 2..RECORD_LAYER_HEADER_SIZE]
            .copy_from_slice(&r_len.to_be_bytes());

        Ok(r)
    }

    pub fn decrypt(&self, r: &[u8]) -> Result<Vec<u8>> {
        let mut reader = Cursor::new(r);
        let h = RecordLayerHeader::unmarshal(&mut reader)?;
        if h.content_type == ContentType::ChangeCipherSpec {
            // Nothing to encrypt with ChangeCipherSpec
            return Ok(r.to_vec());
        }

        let body = &r[RECORD_LAYER_HEADER_SIZE..];
        let iv = &body[0..Self::BLOCK_SIZE];
        let body = &body[Self::BLOCK_SIZE..];
        //TODO: add body.len() check

        let read_cbc = Aes256CbcDec::new_from_slices(&self.remote_key, iv)?;

        let decrypted = read_cbc
            .decrypt_padded_vec_mut::<DtlsPadding>(body)
            .map_err(|_| Error::ErrInvalidPacketLength)?;

        let recv_mac = &decrypted[decrypted.len() - Self::MAC_SIZE..];
        let decrypted = &decrypted[0..decrypted.len() - Self::MAC_SIZE];
        let mac = prf_mac(
            h.epoch,
            h.sequence_number,
            h.content_type,
            h.protocol_version,
            decrypted,
            &self.read_mac,
        )?;

        if recv_mac.ct_eq(&mac).not().into() {
            return Err(Error::ErrInvalidMac);
        }

        let mut d = Vec::with_capacity(RECORD_LAYER_HEADER_SIZE + decrypted.len());
        d.extend_from_slice(&r[..RECORD_LAYER_HEADER_SIZE]);
        d.extend_from_slice(decrypted);

        Ok(d)
    }
}
