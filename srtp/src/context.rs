use aes::block_cipher_trait::generic_array::GenericArray;
use aes::block_cipher_trait::BlockCipher;
use aes::Aes128;
use hmac::{Hmac, Mac};
use sha1::Sha1;

use std::collections::HashMap;
use std::io::BufWriter;

use byteorder::{BigEndian, WriteBytesExt};

use util::Error;

use super::protection_profile::*;

#[cfg(test)]
mod context_test;

#[cfg(test)]
mod srtp_test;

#[cfg(test)]
mod srtcp_test;

pub mod srtcp;
pub mod srtp;

pub const LABEL_SRTP_ENCRYPTION: u8 = 0x00;
pub const LABEL_SRTP_AUTHENTICATION_TAG: u8 = 0x01;
pub const LABEL_SRTP_SALT: u8 = 0x02;
pub const LABEL_SRTCP_ENCRYPTION: u8 = 0x03;
pub const LABEL_SRTCP_AUTHENTICATION_TAG: u8 = 0x04;
pub const LABEL_SRTCP_SALT: u8 = 0x05;

pub const KEY_LEN: usize = 16;
pub const SALT_LEN: usize = 14;

const MAX_ROC_DISORDER: u16 = 100;
pub(crate) const MAX_SEQUENCE_NUMBER: u16 = 65535;

const AUTH_TAG_SIZE: usize = 10;
const SRTCP_INDEX_SIZE: usize = 4;

type HmacSha1 = Hmac<Sha1>;

// Encode/Decode state for a single SSRC
#[derive(Debug, Clone, Default)]
pub struct SSRCState {
    ssrc: u32,
    rollover_counter: u32,
    rollover_has_processed: bool,
    last_sequence_number: u16,
}

impl SSRCState {
    // https://tools.ietf.org/html/rfc3550#appendix-A.1
    pub fn update_rollover_count(&mut self, sequence_number: u16) {
        if !self.rollover_has_processed {
            self.rollover_has_processed = true;
        } else if sequence_number == 0 {
            // We exactly hit the rollover count

            // Only update rolloverCounter if lastSequenceNumber is greater then MAX_ROCDISORDER
            // otherwise we already incremented for disorder
            if self.last_sequence_number > MAX_ROC_DISORDER {
                self.rollover_counter += 1;
            }
        } else if self.last_sequence_number < MAX_ROC_DISORDER
            && sequence_number > (MAX_SEQUENCE_NUMBER - MAX_ROC_DISORDER)
        {
            // Our last sequence number incremented because we crossed 0, but then our current number was within MAX_ROCDISORDER of the max
            // So we fell behind, drop to account for jitter
            self.rollover_counter -= 1;
        } else if sequence_number < MAX_ROC_DISORDER
            && self.last_sequence_number > (MAX_SEQUENCE_NUMBER - MAX_ROC_DISORDER)
        {
            // our current is within a MAX_ROCDISORDER of 0
            // and our last sequence number was a high sequence number, increment to account for jitter
            self.rollover_counter += 1;
        }
        self.last_sequence_number = sequence_number;
    }
}

// Context represents a SRTP cryptographic context
// Context can only be used for one-way operations
// it must either used ONLY for encryption or ONLY for decryption
#[derive(Debug)]
pub struct Context {
    master_key: Vec<u8>,
    master_salt: Vec<u8>,

    ssrc_states: HashMap<u32, SSRCState>,
    srtp_session_key: Vec<u8>,
    srtp_session_salt: Vec<u8>,
    srtp_session_auth: HmacSha1,
    srtp_session_auth_tag: Vec<u8>,
    //srtp_block: Aes128,
    srtcp_session_key: Vec<u8>,
    srtcp_session_salt: Vec<u8>,
    srtcp_session_auth: HmacSha1,
    srtcp_session_auth_tag: Vec<u8>,
    srtcp_index: u32,
    // srtcp_block: Aes128,
}

impl Context {
    // CreateContext creates a new SRTP Context
    pub fn new(
        master_key: Vec<u8>,
        master_salt: Vec<u8>,
        _profile: ProtectionProfile,
    ) -> Result<Context, Error> {
        if master_key.len() != KEY_LEN {
            return Err(Error::new(format!(
                "SRTP Master Key must be len {}, got {}",
                KEY_LEN,
                master_key.len()
            )));
        } else if master_salt.len() != SALT_LEN {
            return Err(Error::new(format!(
                "SRTP Salt must be len {}, got {}",
                SALT_LEN,
                master_salt.len()
            )));
        }

        let srtp_session_key =
            Context::generate_session_key(&master_key, &master_salt, LABEL_SRTP_ENCRYPTION)?;
        let srtp_session_salt =
            Context::generate_session_salt(&master_key, &master_salt, LABEL_SRTP_SALT)?;
        let srtp_session_auth_tag = Context::generate_session_auth_tag(
            &master_key,
            &master_salt,
            LABEL_SRTP_AUTHENTICATION_TAG,
        )?;

        //let srtp_block = Aes128::new(&GenericArray::from_slice(&srtp_session_key));

        let srtp_session_auth = match HmacSha1::new_varkey(&srtp_session_auth_tag) {
            Ok(srtp_session_auth) => srtp_session_auth,
            Err(err) => return Err(Error::new(err.to_string())),
        };

        let srtcp_session_key =
            Context::generate_session_key(&master_key, &master_salt, LABEL_SRTCP_ENCRYPTION)?;
        let srtcp_session_salt =
            Context::generate_session_salt(&master_key, &master_salt, LABEL_SRTCP_SALT)?;
        let srtcp_session_auth_tag = Context::generate_session_auth_tag(
            &master_key,
            &master_salt,
            LABEL_SRTCP_AUTHENTICATION_TAG,
        )?;

        //let srtcp_block = Aes128::new(&GenericArray::from_slice(&srtcp_session_key));

        let srtcp_session_auth = match HmacSha1::new_varkey(&srtcp_session_auth_tag) {
            Ok(srtcp_session_auth) => srtcp_session_auth,
            Err(err) => return Err(Error::new(err.to_string())),
        };

        Ok(Context {
            master_key,
            master_salt,

            srtp_session_key,
            srtp_session_salt,
            srtp_session_auth,
            srtp_session_auth_tag,
            //srtp_block,
            srtcp_session_key,
            srtcp_session_salt,
            srtcp_session_auth,
            srtcp_session_auth_tag,
            //srtcp_block,
            ssrc_states: HashMap::new(),
            srtcp_index: 0,
        })
    }

    fn generate_session_key(
        master_key: &[u8],
        master_salt: &[u8],
        label: u8,
    ) -> Result<Vec<u8>, Error> {
        assert!(master_key.len() == KEY_LEN);
        assert!(master_salt.len() == SALT_LEN);

        // https://tools.ietf.org/html/rfc3711#appendix-B.3
        // The input block for AES-CM is generated by exclusive-oring the master salt with the
        // concatenation of the encryption key label 0x00 with (index DIV kdr),
        // - index is 'rollover count' and DIV is 'divided by'
        let mut session_key: Vec<u8> = vec![0; master_salt.len()];
        session_key.copy_from_slice(master_salt);

        let label_and_index_over_kdr: Vec<u8> = vec![label, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (mut i, mut j) = (
            label_and_index_over_kdr.len() as i32 - 1,
            session_key.len() as i32 - 1,
        );

        while i >= 0 && j >= 0 {
            session_key[j as usize] ^= label_and_index_over_kdr[i as usize];
            i = i - 1;
            j = j - 1;
        }

        // then padding on the right with two null octets (which implements the multiply-by-2^16 operation, see Section 4.3.3).
        session_key.extend_from_slice(&[0x00, 0x00]);

        //The resulting value is then AES-CM- encrypted using the master key to get the cipher key.
        let key = GenericArray::from_slice(master_key);
        let block = Aes128::new(&key);

        let session_key = GenericArray::from_mut_slice(&mut session_key);
        block.encrypt_block(session_key);
        Ok(session_key.to_vec())
    }

    fn generate_session_salt(
        master_key: &[u8],
        master_salt: &[u8],
        label: u8,
    ) -> Result<Vec<u8>, Error> {
        assert!(master_key.len() == KEY_LEN);
        assert!(master_salt.len() == SALT_LEN);

        // https://tools.ietf.org/html/rfc3711#appendix-B.3
        // The input block for AES-CM is generated by exclusive-oring the master salt with
        // the concatenation of the encryption salt label
        let mut session_salt: Vec<u8> = vec![0; master_salt.len()];
        session_salt.copy_from_slice(master_salt);

        let label_and_index_over_kdr: Vec<u8> = vec![label, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (mut i, mut j) = (
            label_and_index_over_kdr.len() as i32 - 1,
            session_salt.len() as i32 - 1,
        );

        while i >= 0 && j >= 0 {
            session_salt[j as usize] ^= label_and_index_over_kdr[i as usize];
            i = i - 1;
            j = j - 1;
        }

        // That value is padded and encrypted as above.
        session_salt.extend_from_slice(&[0x00, 0x00]);

        let key = GenericArray::from_slice(master_key);
        let block = Aes128::new(&key);

        let session_salt = GenericArray::from_mut_slice(&mut session_salt);
        block.encrypt_block(session_salt);
        Ok(session_salt[0..SALT_LEN].to_vec())
    }

    fn generate_session_auth_tag(
        master_key: &[u8],
        master_salt: &[u8],
        label: u8,
    ) -> Result<Vec<u8>, Error> {
        assert!(master_key.len() == KEY_LEN);
        assert!(master_salt.len() == SALT_LEN);

        // https://tools.ietf.org/html/rfc3711#appendix-B.3
        // We now show how the auth key is generated.  The input block for AES-
        // CM is generated as above, but using the authentication key label.
        let mut session_auth_tag: Vec<u8> = vec![0; master_salt.len()];
        session_auth_tag.copy_from_slice(master_salt);

        let label_and_index_over_kdr: Vec<u8> = vec![label, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (mut i, mut j) = (
            label_and_index_over_kdr.len() as i32 - 1,
            session_auth_tag.len() as i32 - 1,
        );

        while i >= 0 && j >= 0 {
            session_auth_tag[j as usize] ^= label_and_index_over_kdr[i as usize];
            i = i - 1;
            j = j - 1;
        }

        // That value is padded and encrypted as above.
        // - We need to do multiple runs at key size (20) is larger then source
        let mut first_run = session_auth_tag.clone();
        let mut second_run = session_auth_tag.clone();
        first_run.extend_from_slice(&[0x00, 0x00]);
        second_run.extend_from_slice(&[0x00, 0x01]);

        let first_run = GenericArray::from_mut_slice(&mut first_run);
        let second_run = GenericArray::from_mut_slice(&mut second_run);

        let key = GenericArray::from_slice(master_key);
        let block = Aes128::new(&key);

        block.encrypt_block(first_run);
        block.encrypt_block(second_run);
        let mut result = first_run.to_vec();
        result.extend_from_slice(&second_run[0..4].to_vec());
        Ok(result)
    }

    // Generate IV https://tools.ietf.org/html/rfc3711#section-4.1.1
    // where the 128-bit integer value IV SHALL be defined by the SSRC, the
    // SRTP packet index i, and the SRTP session salting key k_s, as below.
    // - ROC = a 32-bit unsigned rollover counter (ROC), which records how many
    // -       times the 16-bit RTP sequence number has been reset to zero after
    // -       passing through 65,535
    // i = 2^16 * ROC + SEQ
    // IV = (salt*2 ^ 16) | (ssrc*2 ^ 64) | (i*2 ^ 16)
    fn generate_counter(
        sequence_number: u16,
        rollover_counter: u32,
        ssrc: u32,
        session_salt: &[u8],
    ) -> Result<Vec<u8>, Error> {
        assert!(session_salt.len() <= 16);

        let mut counter: Vec<u8> = vec![0; 16];
        {
            let mut writer = BufWriter::<&mut [u8]>::new(counter[4..].as_mut());
            writer.write_u32::<BigEndian>(ssrc)?;
            writer.write_u32::<BigEndian>(rollover_counter)?;
            writer.write_u32::<BigEndian>((sequence_number as u32) << 16)?;
        }

        for i in 0..session_salt.len() {
            counter[i] ^= session_salt[i];
        }

        Ok(counter)
    }

    fn generate_srtp_auth_tag(
        srtp_session_auth: &mut HmacSha1,
        buf: &[u8],
        roc: u32,
    ) -> Result<Vec<u8>, Error> {
        // https://tools.ietf.org/html/rfc3711#section-4.2
        // In the case of SRTP, M SHALL consist of the Authenticated
        // Portion of the packet (as specified in Figure 1) concatenated with
        // the ROC, M = Authenticated Portion || ROC;
        //
        // The pre-defined authentication transform for SRTP is HMAC-SHA1
        // [RFC2104].  With HMAC-SHA1, the SRTP_PREFIX_LENGTH (Figure 3) SHALL
        // be 0.  For SRTP (respectively SRTCP), the HMAC SHALL be applied to
        // the session authentication key and M as specified above, i.e.,
        // HMAC(k_a, M).  The HMAC output SHALL then be truncated to the n_tag
        // left-most bits.
        // - Authenticated portion of the packet is everything BEFORE MKI
        // - k_a is the session message authentication key
        // - n_tag is the bit-length of the output authentication tag
        srtp_session_auth.reset();

        srtp_session_auth.input(buf);

        // For SRTP only, we need to hash the rollover counter as well.
        let mut roc_buf: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(roc_buf.as_mut());
            writer.write_u32::<BigEndian>(roc)?;
        }

        srtp_session_auth.input(&roc_buf);

        let result = srtp_session_auth.clone().result();
        let code_bytes = result.code();

        // Truncate the hash to the first AUTH_TAG_SIZE bytes.
        Ok(code_bytes[0..AUTH_TAG_SIZE].to_vec())
    }

    fn generate_srtcp_auth_tag(
        srtcp_session_auth: &mut HmacSha1,
        buf: &[u8],
    ) -> Result<Vec<u8>, Error> {
        // https://tools.ietf.org/html/rfc3711#section-4.2
        //
        // The pre-defined authentication transform for SRTP is HMAC-SHA1
        // [RFC2104].  With HMAC-SHA1, the SRTP_PREFIX_LENGTH (Figure 3) SHALL
        // be 0.  For SRTP (respectively SRTCP), the HMAC SHALL be applied to
        // the session authentication key and M as specified above, i.e.,
        // HMAC(k_a, M).  The HMAC output SHALL then be truncated to the n_tag
        // left-most bits.
        // - Authenticated portion of the packet is everything BEFORE MKI
        // - k_a is the session message authentication key
        // - n_tag is the bit-length of the output authentication tag
        srtcp_session_auth.reset();

        srtcp_session_auth.input(buf);

        let result = srtcp_session_auth.clone().result();
        let code_bytes = result.code();

        // Truncate the hash to the first AUTH_TAG_SIZE bytes.
        Ok(code_bytes[0..AUTH_TAG_SIZE].to_vec())
    }

    fn get_ssrc_state(&mut self, ssrc: u32) -> Option<&mut SSRCState> {
        if !self.ssrc_states.contains_key(&ssrc) {
            let s = SSRCState {
                ssrc,
                ..Default::default()
            };
            self.ssrc_states.insert(ssrc, s);
        }
        self.ssrc_states.get_mut(&ssrc)
    }
}
