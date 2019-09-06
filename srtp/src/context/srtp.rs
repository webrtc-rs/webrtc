use super::*;

use rtp;

use subtle::ConstantTimeEq;

use std::io::BufReader;

use aes;
use ctr;
use ctr::stream_cipher::generic_array::GenericArray;
use ctr::stream_cipher::{NewStreamCipher, StreamCipher};

type Aes128Ctr = ctr::Ctr128<aes::Aes128>;

impl Context {
    // DecryptRTP decrypts a RTP packet with an encrypted payload
    pub fn decrypt_rtp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        if encrypted.len() < AUTH_TAG_SIZE {
            return Err(Error::new(format!(
                "too short SRTP packet: only {} bytes, expected > 10 bytes",
                encrypted.len()
            )));
        }
        let mut reader = BufReader::new(encrypted);
        let header = rtp::packet::Header::unmarshal(&mut reader)?;

        let mut dst: Vec<u8> = vec![0; encrypted.len() - AUTH_TAG_SIZE];
        let s: SSRCState;
        {
            if let Some(ss) = self.get_ssrc_state(header.ssrc) {
                ss.update_rollover_count(header.sequence_number);
                s = ss.clone();
            } else {
                return Err(Error::new(format!(
                    "can't find ssrc: {} in ssrc_states",
                    header.ssrc
                )));
            }
        }

        // Split the auth tag and the cipher text into two parts.
        let actual_tag = &encrypted[encrypted.len() - AUTH_TAG_SIZE..];
        let cipher_text = &encrypted[..encrypted.len() - AUTH_TAG_SIZE];

        // Generate the auth tag we expect to see from the ciphertext.
        let expected_tag = Context::generate_srtp_auth_tag(
            &mut self.srtp_session_auth,
            cipher_text,
            s.rollover_counter,
        )?;

        // See if the auth tag actually matches.
        // We use a constant time comparison to prevent timing attacks.
        if actual_tag.ct_eq(&expected_tag).unwrap_u8() != 1 {
            return Err(Error::new("failed to verify auth tag".to_string()));
        }

        // Write the plaintext header to the destination buffer.
        dst.copy_from_slice(cipher_text);

        // Decrypt the ciphertext for the payload.
        let counter = Context::generate_counter(
            header.sequence_number,
            s.rollover_counter,
            s.ssrc,
            &self.srtp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(&key, &nonce);

        stream.decrypt(&mut dst[header.payload_offset..]);

        Ok(dst)
    }

    // EncryptRTP marshals and encrypts an RTP packet, writing to the dst buffer provided.
    // If the dst buffer does not have the capacity to hold `len(plaintext) + 10` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtp(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(plaintext);
        let header = rtp::packet::Header::unmarshal(&mut reader)?;

        let mut dst: Vec<u8> = vec![0; plaintext.len()];

        let s: SSRCState;
        {
            if let Some(ss) = self.get_ssrc_state(header.ssrc) {
                ss.update_rollover_count(header.sequence_number);
                s = ss.clone();
            } else {
                return Err(Error::new(format!(
                    "can't find ssrc: {} in ssrc_states",
                    header.ssrc
                )));
            }
        }

        // Write the plaintext header to the destination buffer.
        dst.copy_from_slice(plaintext);

        // Encrypt the payload
        let counter = Context::generate_counter(
            header.sequence_number,
            s.rollover_counter,
            s.ssrc,
            &self.srtp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(&key, &nonce);

        stream.encrypt(&mut dst[header.payload_offset..]);

        // Generate the auth tag.
        let auth_tag =
            Context::generate_srtp_auth_tag(&mut self.srtp_session_auth, &dst, s.rollover_counter)?;

        dst.extend_from_slice(&auth_tag);

        Ok(dst)
    }
}
