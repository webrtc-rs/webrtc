#[cfg(test)]
mod context_test;
#[cfg(test)]
mod srtcp_test;
#[cfg(test)]
mod srtp_test;

use std::collections::HashMap;

use util::replay_detector::*;

use crate::cipher::cipher_aead_aes_gcm::*;
use crate::cipher::cipher_aes_cm_hmac_sha1::*;
use crate::cipher::*;
use crate::error::{Error, Result};
use crate::option::*;
use crate::protection_profile::*;

pub mod srtcp;
pub mod srtp;

const MAX_ROC_DISORDER: u16 = 100;

/// Encrypt/Decrypt state for a single SRTP SSRC
#[derive(Default)]
pub(crate) struct SrtpSsrcState {
    ssrc: u32,
    rollover_counter: u32,
    rollover_has_processed: bool,
    last_sequence_number: u16,
    replay_detector: Option<Box<dyn ReplayDetector + Send + 'static>>,
}

/// Encrypt/Decrypt state for a single SRTCP SSRC
#[derive(Default)]
pub(crate) struct SrtcpSsrcState {
    srtcp_index: usize,
    ssrc: u32,
    replay_detector: Option<Box<dyn ReplayDetector + Send + 'static>>,
}

impl SrtpSsrcState {
    pub fn next_rollover_count(&self, sequence_number: u16) -> u32 {
        let mut roc = self.rollover_counter;

        if !self.rollover_has_processed {
        } else if sequence_number == 0 {
            // We exactly hit the rollover count

            // Only update rolloverCounter if lastSequenceNumber is greater then MAX_ROCDISORDER
            // otherwise we already incremented for disorder
            if self.last_sequence_number > MAX_ROC_DISORDER {
                roc += 1;
            }
        } else if self.last_sequence_number < MAX_ROC_DISORDER
            && sequence_number > (MAX_SEQUENCE_NUMBER - MAX_ROC_DISORDER)
        {
            // Our last sequence number incremented because we crossed 0, but then our current number was within MAX_ROCDISORDER of the max
            // So we fell behind, drop to account for jitter
            roc -= 1;
        } else if sequence_number < MAX_ROC_DISORDER
            && self.last_sequence_number > (MAX_SEQUENCE_NUMBER - MAX_ROC_DISORDER)
        {
            // our current is within a MAX_ROCDISORDER of 0
            // and our last sequence number was a high sequence number, increment to account for jitter
            roc += 1;
        }

        roc
    }

    /// https://tools.ietf.org/html/rfc3550#appendix-A.1
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

/// Context represents a SRTP cryptographic context
/// Context can only be used for one-way operations
/// it must either used ONLY for encryption or ONLY for decryption
pub struct Context {
    cipher: Box<dyn Cipher + Send>,

    srtp_ssrc_states: HashMap<u32, SrtpSsrcState>,
    srtcp_ssrc_states: HashMap<u32, SrtcpSsrcState>,

    new_srtp_replay_detector: ContextOption,
    new_srtcp_replay_detector: ContextOption,
}

impl Context {
    /// CreateContext creates a new SRTP Context
    pub fn new(
        master_key: &[u8],
        master_salt: &[u8],
        profile: ProtectionProfile,
        srtp_ctx_opt: Option<ContextOption>,
        srtcp_ctx_opt: Option<ContextOption>,
    ) -> Result<Context> {
        let key_len = profile.key_len();
        let salt_len = profile.salt_len();

        if master_key.len() != key_len {
            return Err(Error::SrtpMasterKeyLength(key_len, master_key.len()));
        } else if master_salt.len() != salt_len {
            return Err(Error::SrtpSaltLength(salt_len, master_salt.len()));
        }

        let cipher: Box<dyn Cipher + Send> = match profile {
            ProtectionProfile::Aes128CmHmacSha1_32 | ProtectionProfile::Aes128CmHmacSha1_80 => {
                Box::new(CipherAesCmHmacSha1::new(profile, master_key, master_salt)?)
            }

            ProtectionProfile::AeadAes128Gcm | ProtectionProfile::AeadAes256Gcm => {
                Box::new(CipherAeadAesGcm::new(profile, master_key, master_salt)?)
            }
        };

        let srtp_ctx_opt = if let Some(ctx_opt) = srtp_ctx_opt {
            ctx_opt
        } else {
            srtp_no_replay_protection()
        };

        let srtcp_ctx_opt = if let Some(ctx_opt) = srtcp_ctx_opt {
            ctx_opt
        } else {
            srtcp_no_replay_protection()
        };

        Ok(Context {
            cipher,
            srtp_ssrc_states: HashMap::new(),
            srtcp_ssrc_states: HashMap::new(),
            new_srtp_replay_detector: srtp_ctx_opt,
            new_srtcp_replay_detector: srtcp_ctx_opt,
        })
    }

    fn get_srtp_ssrc_state(&mut self, ssrc: u32) -> &mut SrtpSsrcState {
        let s = SrtpSsrcState {
            ssrc,
            replay_detector: Some((self.new_srtp_replay_detector)()),
            ..Default::default()
        };

        self.srtp_ssrc_states.entry(ssrc).or_insert(s)
    }

    fn get_srtcp_ssrc_state(&mut self, ssrc: u32) -> &mut SrtcpSsrcState {
        let s = SrtcpSsrcState {
            ssrc,
            replay_detector: Some((self.new_srtcp_replay_detector)()),
            ..Default::default()
        };
        self.srtcp_ssrc_states.entry(ssrc).or_insert(s)
    }

    /// roc returns SRTP rollover counter value of specified SSRC.
    fn get_roc(&self, ssrc: u32) -> Option<u32> {
        self.srtp_ssrc_states.get(&ssrc).map(|s| s.rollover_counter)
    }

    /// set_roc sets SRTP rollover counter value of specified SSRC.
    fn set_roc(&mut self, ssrc: u32, roc: u32) {
        self.get_srtp_ssrc_state(ssrc).rollover_counter = roc;
    }

    /// index returns SRTCP index value of specified SSRC.
    fn get_index(&self, ssrc: u32) -> Option<usize> {
        self.srtcp_ssrc_states.get(&ssrc).map(|s| s.srtcp_index)
    }

    /// set_index sets SRTCP index value of specified SSRC.
    fn set_index(&mut self, ssrc: u32, index: usize) {
        self.get_srtcp_ssrc_state(ssrc).srtcp_index = index;
    }
}
