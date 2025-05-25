use std::io::Cursor;

use chacha20poly1305::aead::generic_array::GenericArray;
use chacha20poly1305::aead::AeadInPlace;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};

use super::*;
use crate::content::*;
use crate::error::*;
use crate::record_layer::record_layer_header::*; // what about Aes256Gcm?

const CRYPTO_CHACHA20_TAG_LENGTH: usize = 16;
const CRYPTO_CHACHA20_NONCE_LENGTH: usize = 12;

// State needed to handle encrypted input/output
#[derive(Clone)]
pub struct CryptoChaCha20 {
    local_cc: ChaCha20Poly1305,
    remote_cc: ChaCha20Poly1305,
    local_key: Vec<u8>,
    remote_key: Vec<u8>,
    local_write_iv: Vec<u8>,
    remote_write_iv: Vec<u8>,
}

fn noncegen(nonce: &mut [u8], epoch: u16, seqnum: u64) {
    let epoch: u64 = epoch.into();
    let seqnum = (seqnum & 0xFFFFFFFFFFFF) | (epoch << 48);
    for i in 0..8 {
        nonce[i + 4] ^= ((seqnum >> (8 * (7 - i))) & 0xFF) as u8;
    }
}

impl CryptoChaCha20 {
    pub fn new(
        local_key: &[u8],
        local_write_iv: &[u8],
        remote_key: &[u8],
        remote_write_iv: &[u8],
    ) -> Self {
        let key = GenericArray::from_slice(local_key);
        let local_cc = ChaCha20Poly1305::new(key);

        let key = GenericArray::from_slice(remote_key);
        let remote_cc = ChaCha20Poly1305::new(key);

        CryptoChaCha20 {
            local_cc,
            local_write_iv: local_write_iv.to_vec(),
            remote_cc,
            local_key: local_key.to_vec(),
            remote_key: remote_key.to_vec(),
            remote_write_iv: remote_write_iv.to_vec(),
        }
    }

    pub fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>> {
        let payload = &raw[RECORD_LAYER_HEADER_SIZE..];
        let raw = &raw[..RECORD_LAYER_HEADER_SIZE];

        let mut nonce = vec![0u8; CRYPTO_CHACHA20_NONCE_LENGTH];
        nonce[..CRYPTO_CHACHA20_NONCE_LENGTH]
            .copy_from_slice(&self.local_write_iv[..CRYPTO_CHACHA20_NONCE_LENGTH]);

        noncegen(&mut nonce[..], pkt_rlh.epoch, pkt_rlh.sequence_number);
        let nonce = GenericArray::from_slice(&nonce);

        let additional_data = generate_aead_additional_data(pkt_rlh, payload.len());

        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(payload);

        let tag = self
            .local_cc
            .encrypt_in_place_detached(nonce, &additional_data, &mut buffer)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut r = Vec::with_capacity(raw.len() + buffer.len() + tag.len());
        r.extend_from_slice(raw);
        r.extend_from_slice(&buffer);
        r.extend_from_slice(&tag);

        // Update recordLayer size to include explicit nonce
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

        let mut nonce = vec![];
        nonce.extend_from_slice(&self.remote_write_iv[..]);

        noncegen(&mut nonce[..], h.epoch, h.sequence_number);
        let nonce = GenericArray::from_slice(&nonce);

        let out = &r[RECORD_LAYER_HEADER_SIZE..];

        let additional_data =
            generate_aead_additional_data(&h, out.len() - CRYPTO_CHACHA20_TAG_LENGTH);

        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(out);

        self.remote_cc
            .decrypt_in_place(nonce, &additional_data, &mut buffer)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut d = Vec::with_capacity(RECORD_LAYER_HEADER_SIZE + buffer.len());
        d.extend_from_slice(&r[..RECORD_LAYER_HEADER_SIZE]);
        d.extend_from_slice(&buffer);

        Ok(d)
    }
}
