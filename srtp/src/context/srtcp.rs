use super::*;

use rtcp;

use subtle::ConstantTimeEq;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{BufReader, BufWriter};

use aes;
use ctr;
use ctr::stream_cipher::generic_array::GenericArray;
use ctr::stream_cipher::{NewStreamCipher, StreamCipher};

type Aes128Ctr = ctr::Ctr128<aes::Aes128>;

impl Context {
    // DecryptRTCP decrypts a RTCP packet with an encrypted payload
    pub fn decrypt_rtcp(&mut self, encrypted: &[u8]) -> Result<Vec<u8>, Error> {
        if encrypted.len() < AUTH_TAG_SIZE + SRTCP_INDEX_SIZE {
            return Err(Error::new(format!(
                "too short SRTCP packet: only {} bytes, expected > 14 bytes",
                encrypted.len()
            )));
        }
        let mut reader = BufReader::new(encrypted);
        rtcp::header::Header::unmarshal(&mut reader)?;

        let tail_offset = encrypted.len() - (AUTH_TAG_SIZE + SRTCP_INDEX_SIZE);
        let mut dst: Vec<u8> = vec![0; tail_offset];
        dst.copy_from_slice(&encrypted[0..tail_offset]);

        let is_encrypted = encrypted[tail_offset] >> 7;
        if is_encrypted == 0 {
            return Ok(dst);
        }

        // Split the auth tag and the cipher text into two parts.
        let actual_tag = &encrypted[encrypted.len() - AUTH_TAG_SIZE..];
        let cipher_text = &encrypted[..encrypted.len() - AUTH_TAG_SIZE];

        // Generate the auth tag we expect to see from the ciphertext.
        let expected_tag =
            Context::generate_srtcp_auth_tag(&mut self.srtcp_session_auth, cipher_text)?;

        // See if the auth tag actually matches.
        // We use a constant time comparison to prevent timing attacks.
        if actual_tag.ct_eq(&expected_tag).unwrap_u8() != 1 {
            return Err(Error::new("failed to verify auth tag".to_string()));
        }

        // Decode SRTCP Index and remove Encryption bit
        let srtcp_index_buffer = &encrypted[tail_offset..tail_offset + SRTCP_INDEX_SIZE];
        let mut reader = BufReader::new(srtcp_index_buffer);
        let index = (reader.read_u32::<BigEndian>()?) & 0x7FFFFFFF; // &^ (1 << 31) in golang

        // Decode SSRC
        let mut reader = BufReader::new(&encrypted[rtcp::header::HEADER_LENGTH..]);
        let ssrc = reader.read_u32::<BigEndian>()?;

        let counter = Context::generate_counter(
            (index & 0xFFFF) as u16,
            index >> 16,
            ssrc,
            &self.srtcp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtcp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(&key, &nonce);

        stream.decrypt(&mut dst[rtcp::header::HEADER_LENGTH + rtcp::header::SSRC_LENGTH..]);

        Ok(dst)
    }

    // EncryptRTCP marshals and encrypts an RTCP packet, writing to the dst buffer provided.
    // If the dst buffer does not have the capacity to hold `len(plaintext) + 14` bytes, a new one will be allocated and returned.
    pub fn encrypt_rtcp(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let mut reader = BufReader::new(plaintext);
        rtcp::header::Header::unmarshal(&mut reader)?;

        // We roll over early because MSB is used for marking as encrypted
        self.srtcp_index += 1;
        if self.srtcp_index >= 2147483647 {
            self.srtcp_index = 0;
        }
        let mut reader = BufReader::new(&plaintext[rtcp::header::HEADER_LENGTH..]);
        let ssrc = reader.read_u32::<BigEndian>()?;

        let mut dst: Vec<u8> = vec![0; plaintext.len()];

        // Write the plaintext to the destination buffer.
        dst.copy_from_slice(plaintext);

        // Encrypt everything after header
        let counter = Context::generate_counter(
            (self.srtcp_index & 0xFFFF) as u16,
            self.srtcp_index >> 16,
            ssrc,
            &self.srtcp_session_salt,
        )?;

        let key = GenericArray::from_slice(&self.srtcp_session_key);
        let nonce = GenericArray::from_slice(&counter);
        let mut stream = Aes128Ctr::new(&key, &nonce);

        stream.encrypt(&mut dst[rtcp::header::HEADER_LENGTH + rtcp::header::SSRC_LENGTH..]);

        // Add SRTCP Index and set Encryption bit
        let mut srtcp_index_buffer: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::new(&mut srtcp_index_buffer);
            writer.write_u32::<BigEndian>(self.srtcp_index | (1u32 << 31))?;
        }
        dst.extend_from_slice(&srtcp_index_buffer);

        // Generate the auth tag.
        let auth_tag = Context::generate_srtcp_auth_tag(&mut self.srtcp_session_auth, &dst)?;

        dst.extend_from_slice(&auth_tag);

        Ok(dst)
    }
}
