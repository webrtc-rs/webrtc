// AES-CBC (Cipher Block Chaining)
// First historic block cipher for AES.
// CBC mode is insecure and must not be used. It’s been progressively deprecated and
// removed from SSL libraries.
// Introduced with TLS 1.0 year 2002. Superseded by GCM in TLS 1.2 year 2008.
// Removed in TLS 1.3 year 2018.
// RFC 3268 year 2002 https://tools.ietf.org/html/rfc3268

// https://github.com/RustCrypto/block-ciphers

use std::io::Cursor;

use crate::content::*;
use crate::prf::*;
use crate::record_layer::record_layer_header::*;

use aes::Aes256;
use anyhow::Result;
use block_modes::block_padding::Pkcs7;
use block_modes::{BlockMode, Cbc};
type Aes256Cbc = Cbc<Aes256, Pkcs7>;

// State needed to handle encrypted input/output
#[derive(Clone)]
pub struct CryptoCbc {
    write_cbc: Aes256Cbc,
    read_cbc: Aes256Cbc,
    write_mac: Vec<u8>,
    read_mac: Vec<u8>,
}

impl CryptoCbc {
    const BLOCK_SIZE: usize = 32;

    pub fn new(
        local_key: &[u8],
        local_write_iv: &[u8],
        local_mac: &[u8],
        remote_key: &[u8],
        remote_write_iv: &[u8],
        remote_mac: &[u8],
    ) -> Result<Self> {
        Ok(CryptoCbc {
            write_cbc: Aes256Cbc::new_var(local_key, local_write_iv)?,
            write_mac: local_mac.to_vec(),

            read_cbc: Aes256Cbc::new_var(remote_key, remote_write_iv)?,
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

        let encrypted = self.write_cbc.clone().encrypt_vec(&payload);

        // Prepend unencrypte header with encrypted payload
        let mut r = vec![];
        r.extend_from_slice(raw);
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
        //TODO: add body.len() check

        let decrypted = self.read_cbc.clone().decrypt_vec(body)?;

        let mut d = Vec::with_capacity(RECORD_LAYER_HEADER_SIZE + decrypted.len());
        d.extend_from_slice(&r[..RECORD_LAYER_HEADER_SIZE]);
        d.extend_from_slice(&decrypted);

        Ok(d)
    }
}
