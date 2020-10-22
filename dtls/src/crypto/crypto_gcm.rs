// AES-GCM (Galois Counter Mode)
// The most widely used block cipher worldwide.
// Mandatory as of TLS 1.2 (2008) and used by default by most clients.
// RFC 5288 year 2008 https://tools.ietf.org/html/rfc5288

// https://github.com/RustCrypto/AEADs
// https://docs.rs/aes-gcm/0.8.0/aes_gcm/

use util::Error;

use rand::Rng;

use super::*;
use crate::record_layer::record_layer_header::*;
use crate::record_layer::*;

use aes_gcm::aead::{generic_array::GenericArray, AeadInPlace, NewAead};
use aes_gcm::Aes128Gcm; // what about Aes256Gcm?

const CRYPTO_GCM_TAG_LENGTH: usize = 16;
const CRYPTO_GCM_NONCE_LENGTH: usize = 12;

// State needed to handle encrypted input/output
pub struct CryptoGcm {
    local_gcm: Aes128Gcm,
    remote_gcm: Aes128Gcm,
    local_write_iv: Vec<u8>,
    remote_write_iv: Vec<u8>,
}

impl CryptoGcm {
    pub fn new(
        local_key: &[u8],
        local_write_iv: Vec<u8>,
        remote_key: &[u8],
        remote_write_iv: Vec<u8>,
    ) -> Result<Self, Error> {
        let key = GenericArray::from_slice(local_key);
        let local_gcm = Aes128Gcm::new(key);

        let key = GenericArray::from_slice(remote_key);
        let remote_gcm = Aes128Gcm::new(key);

        Ok(CryptoGcm {
            local_gcm,
            local_write_iv,
            remote_gcm,
            remote_write_iv,
        })
    }

    pub fn encrypt(&mut self, pkt: &RecordLayer, raw: &[u8]) -> Result<Vec<u8>, Error> {
        let payload = &raw[RECORD_LAYER_HEADER_SIZE..];
        let raw = &raw[..RECORD_LAYER_HEADER_SIZE];

        let mut nonce = vec![0u8; CRYPTO_GCM_NONCE_LENGTH];
        nonce[..4].copy_from_slice(&self.local_write_iv[..4]);
        rand::thread_rng().fill(&mut nonce[4..]);
        let nonce = GenericArray::from_slice(&nonce);

        let additional_data =
            generate_aead_additional_data(&pkt.record_layer_header, payload.len());

        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(payload);

        self.local_gcm
            .encrypt_in_place(nonce, &additional_data, &mut buffer)?;

        let mut r = Vec::with_capacity(raw.len() + nonce.len() + buffer.len());
        r.extend_from_slice(raw);
        r.extend_from_slice(&nonce[4..]);
        r.extend_from_slice(&buffer);

        // Update recordLayer size to include explicit nonce

        let r_len = (r.len() - RECORD_LAYER_HEADER_SIZE) as u16;
        r[RECORD_LAYER_HEADER_SIZE - 2..RECORD_LAYER_HEADER_SIZE]
            .copy_from_slice(&r_len.to_be_bytes());

        Ok(r)
    }

    /*
    pub fn decrypt(&mut self, r: &[u8]) ->Result<Vec<u8>, Error> {
        var h recordLayerHeader
        err := h.Unmarshal(in)
        switch {
        case err != nil:
            return nil, err
        case h.contentType == contentTypeChangeCipherSpec:
            // Nothing to encrypt with ChangeCipherSpec
            return in, nil
        case len(in) <= (8 + RECORD_LAYER_HEADER_SIZE):
            return nil, errNotEnoughRoomForNonce
        }

        nonce := make([]byte, 0, CRYPTO_GCMNONCE_LENGTH)
        nonce = append(append(nonce, c.remote_write_iv[:4]...), in[RECORD_LAYER_HEADER_SIZE:RECORD_LAYER_HEADER_SIZE+8]...)
        out := in[RECORD_LAYER_HEADER_SIZE+8:]

        additionalData := generate_aeadadditional_data(&h, len(out)-CRYPTO_GCMTAG_LENGTH)
        out, err = c.remoteGCM.Open(out[:0], nonce, out, additionalData)
        if err != nil {
            return nil, fmt.Errorf("decryptPacket: %v", err)
        }
        return append(in[:RECORD_LAYER_HEADER_SIZE], out...), nil
    }*/
}
