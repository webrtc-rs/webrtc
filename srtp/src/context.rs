use hmac::{Hmac, Mac};
use sha1::Sha1;

use std::collections::HashMap;
use std::io::BufWriter;

use byteorder::{BigEndian, WriteBytesExt};

use util::Error;

// ProtectionProfile specifies Cipher and AuthTag details, similar to TLS cipher suite
pub type ProtectionProfile = u16;

// Supported protection profiles
const ProtectionProfileAes128CmHmacSha1_80: ProtectionProfile = 0x0001;

const labelSRTPEncryption: u8 = 0x00;
const labelSRTPAuthenticationTag: u8 = 0x01;
const labelSRTPSalt: u8 = 0x02;

const labelSRTCPEncryption: u8 = 0x03;
const labelSRTCPAuthenticationTag: u8 = 0x04;
const labelSRTCPSalt: u8 = 0x05;

const keyLen: usize = 16;
const saltLen: usize = 14;

const maxROCDisorder: u16 = 100;
const maxSequenceNumber: u16 = 65535;

const authTagSize: usize = 10;
const srtcpIndexSize: usize = 4;

type HmacSha1 = Hmac<Sha1>;

// Encode/Decode state for a single SSRC
#[derive(Debug, Default)]
struct SSRCState {
    ssrc: u32,
    rollover_counter: u32,
    rollover_has_processed: bool,
    last_sequence_number: u16,
}

// Context represents a SRTP cryptographic context
// Context can only be used for one-way operations
// it must either used ONLY for encryption or ONLY for decryption
#[derive(Debug)]
struct Context {
    master_key: Vec<u8>,
    master_salt: Vec<u8>,

    ssrc_states: HashMap<u32, SSRCState>,
    srtp_session_key: Vec<u8>,
    srtp_session_salt: Vec<u8>,
    srtp_session_auth: HmacSha1,
    srtp_session_auth_tag: Vec<u8>,
    //srtp_block          cipher.Block //TODO
    srtcp_session_key: Vec<u8>,
    srtcp_session_salt: Vec<u8>,
    srtcp_session_auth: HmacSha1,
    srtcp_session_auth_tag: Vec<u8>,
    srtcp_index: u32,
    //srtcp_block          cipher.Block, //TODO
}

impl Context {
    // CreateContext creates a new SRTP Context
    pub fn new(
        master_key: Vec<u8>,
        master_salt: Vec<u8>,
        profile: ProtectionProfile,
    ) -> Result<Context, Error> {
        if master_key.len() != keyLen {
            return Err(Error::new(format!(
                "SRTP Master Key must be len {}, got {}",
                keyLen,
                master_key.len()
            )));
        } else if master_salt.len() != saltLen {
            return Err(Error::new(format!(
                "SRTP Salt must be len {}, got {}",
                saltLen,
                master_salt.len()
            )));
        }

        /*
        if c.srtpSessionKey, err = c.generateSessionKey(labelSRTPEncryption); err != nil {
            return nil, err
        } else if c.srtpSessionSalt, err = c.generateSessionSalt(labelSRTPSalt); err != nil {
            return nil, err
        } else if c.srtpSessionAuthTag, err = c.generateSessionAuthTag(labelSRTPAuthenticationTag); err != nil {
            return nil, err
        } else if c.srtpBlock, err = aes.NewCipher(c.srtpSessionKey); err != nil {
            return nil, err
        }

        c.srtpSessionAuth = hmac.New(sha1.New, c.srtpSessionAuthTag)

        if c.srtcpSessionKey, err = c.generateSessionKey(labelSRTCPEncryption); err != nil {
            return nil, err
        } else if c.srtcpSessionSalt, err = c.generateSessionSalt(labelSRTCPSalt); err != nil {
            return nil, err
        } else if c.srtcpSessionAuthTag, err = c.generateSessionAuthTag(labelSRTCPAuthenticationTag); err != nil {
            return nil, err
        } else if c.srtcpBlock, err = aes.NewCipher(c.srtcpSessionKey); err != nil {
            return nil, err
        }

        c.srtcpSessionAuth = hmac.New(sha1.New, c.srtcpSessionAuthTag)
        */

        Err(Error::new("".to_string()))
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
        &self,
        sequence_number: u16,
        rollover_counter: u32,
        ssrc: u32,
        session_salt: &[u8],
    ) -> Result<Vec<u8>, Error> {
        if session_salt.len() != 16 {
            return Err(Error::new(format!(
                "invalid session_salt length: expected 16, but got {}",
                session_salt.len()
            )));
        }

        let mut counter: Vec<u8> = vec![0; 16];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(counter.as_mut());
            writer.write_u32::<BigEndian>(ssrc)?;
            writer.write_u32::<BigEndian>(rollover_counter)?;
            writer.write_u32::<BigEndian>((sequence_number as u32) << 16)?;
        }

        for i in 0..16 {
            counter[i] ^= session_salt[i];
        }

        Ok(counter)
    }

    fn generate_srtp_auth_tag(&mut self, buf: &[u8], roc: u32) -> Result<Vec<u8>, Error> {
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
        self.srtp_session_auth.reset();

        self.srtp_session_auth.input(buf);

        // For SRTP only, we need to hash the rollover counter as well.
        let mut roc_buf: Vec<u8> = vec![0; 4];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(roc_buf.as_mut());
            writer.write_u32::<BigEndian>(roc)?;
        }

        self.srtp_session_auth.input(roc_buf.as_slice());

        let result = self.srtp_session_auth.clone().result();
        let code_bytes = result.code();

        // Truncate the hash to the first 10 bytes.
        Ok(code_bytes[0..10].to_vec())
    }

    fn generate_srtcp_auth_tag(&mut self, buf: &[u8]) -> Result<Vec<u8>, Error> {
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
        self.srtcp_session_auth.reset();

        self.srtcp_session_auth.input(buf);

        let result = self.srtcp_session_auth.clone().result();
        let code_bytes = result.code();

        // Truncate the hash to the first 10 bytes.
        Ok(code_bytes[0..10].to_vec())
    }

    // https://tools.ietf.org/html/rfc3550#appendix-A.1
    fn update_rollover_count(&self, sequence_number: u16, s: &mut SSRCState) {
        if !s.rollover_has_processed {
            s.rollover_has_processed = true;
        } else if sequence_number == 0 {
            // We exactly hit the rollover count

            // Only update rolloverCounter if lastSequenceNumber is greater then maxROCDisorder
            // otherwise we already incremented for disorder
            if s.last_sequence_number > maxROCDisorder {
                s.rollover_counter += 1;
            }
        } else if s.last_sequence_number < maxROCDisorder
            && sequence_number > (maxSequenceNumber - maxROCDisorder)
        {
            // Our last sequence number incremented because we crossed 0, but then our current number was within maxROCDisorder of the max
            // So we fell behind, drop to account for jitter
            s.rollover_counter -= 1;
        } else if sequence_number < maxROCDisorder
            && s.last_sequence_number > (maxSequenceNumber - maxROCDisorder)
        {
            // our current is within a maxROCDisorder of 0
            // and our last sequence number was a high sequence number, increment to account for jitter
            s.rollover_counter += 1;
        }
        s.last_sequence_number = sequence_number;
    }

    fn get_ssrc_state(&self, ssrc: &u32) -> Option<&SSRCState> {
        self.ssrc_states.get(ssrc)
    }
}
