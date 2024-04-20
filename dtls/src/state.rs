use std::io::{BufWriter, Cursor};
use std::marker::{Send, Sync};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use async_trait::async_trait;
use portable_atomic::AtomicU16;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use util::{KeyingMaterialExporter, KeyingMaterialExporterError};

use super::cipher_suite::*;
use super::conn::*;
use super::curve::named_curve::*;
use super::extension::extension_use_srtp::SrtpProtectionProfile;
use super::handshake::handshake_random::*;
use super::prf::*;
use crate::error::*;

// State holds the dtls connection state and implements both encoding.BinaryMarshaler and encoding.BinaryUnmarshaler
pub struct State {
    pub(crate) local_epoch: Arc<AtomicU16>,
    pub(crate) remote_epoch: Arc<AtomicU16>,
    pub(crate) local_sequence_number: Arc<Mutex<Vec<u64>>>, // uint48
    pub(crate) local_random: HandshakeRandom,
    pub(crate) remote_random: HandshakeRandom,
    pub(crate) master_secret: Vec<u8>,
    pub(crate) cipher_suite: Arc<Mutex<Option<Box<dyn CipherSuite + Send + Sync>>>>, // nil if a cipher_suite hasn't been chosen

    pub(crate) srtp_protection_profile: SrtpProtectionProfile, // Negotiated srtp_protection_profile
    pub peer_certificates: Vec<Vec<u8>>,
    pub identity_hint: Vec<u8>,

    pub(crate) is_client: bool,

    pub(crate) pre_master_secret: Vec<u8>,
    pub(crate) extended_master_secret: bool,

    pub(crate) named_curve: NamedCurve,
    pub(crate) local_keypair: Option<NamedCurveKeypair>,
    pub(crate) cookie: Vec<u8>,
    pub(crate) handshake_send_sequence: isize,
    pub(crate) handshake_recv_sequence: isize,
    pub(crate) server_name: String,
    pub(crate) remote_requested_certificate: bool, // Did we get a CertificateRequest
    pub(crate) local_certificates_verify: Vec<u8>, // cache CertificateVerify
    pub(crate) local_verify_data: Vec<u8>,         // cached VerifyData
    pub(crate) local_key_signature: Vec<u8>,       // cached keySignature
    pub(crate) peer_certificates_verified: bool,
    //pub(crate) replay_detector: Vec<Box<dyn ReplayDetector + Send + Sync>>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct SerializedState {
    local_epoch: u16,
    remote_epoch: u16,
    local_random: [u8; HANDSHAKE_RANDOM_LENGTH],
    remote_random: [u8; HANDSHAKE_RANDOM_LENGTH],
    cipher_suite_id: u16,
    master_secret: Vec<u8>,
    sequence_number: u64,
    srtp_protection_profile: u16,
    peer_certificates: Vec<Vec<u8>>,
    identity_hint: Vec<u8>,
    is_client: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            local_epoch: Arc::new(AtomicU16::new(0)),
            remote_epoch: Arc::new(AtomicU16::new(0)),
            local_sequence_number: Arc::new(Mutex::new(vec![])),
            local_random: HandshakeRandom::default(),
            remote_random: HandshakeRandom::default(),
            master_secret: vec![],
            cipher_suite: Arc::new(Mutex::new(None)), // nil if a cipher_suite hasn't been chosen

            srtp_protection_profile: SrtpProtectionProfile::Unsupported, // Negotiated srtp_protection_profile
            peer_certificates: vec![],
            identity_hint: vec![],

            is_client: false,

            pre_master_secret: vec![],
            extended_master_secret: false,

            named_curve: NamedCurve::Unsupported,
            local_keypair: None,
            cookie: vec![],
            handshake_send_sequence: 0,
            handshake_recv_sequence: 0,
            server_name: "".to_string(),
            remote_requested_certificate: false, // Did we get a CertificateRequest
            local_certificates_verify: vec![],   // cache CertificateVerify
            local_verify_data: vec![],           // cached VerifyData
            local_key_signature: vec![],         // cached keySignature
            peer_certificates_verified: false,
            //replay_detector: vec![],
        }
    }
}

impl State {
    pub(crate) async fn clone(&self) -> Self {
        let mut state = State::default();

        if let Ok(serialized) = self.serialize().await {
            let _ = state.deserialize(&serialized).await;
        }

        state
    }

    async fn serialize(&self) -> Result<SerializedState> {
        let mut local_rand = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(local_rand.as_mut());
            self.local_random.marshal(&mut writer)?;
        }
        let mut remote_rand = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(remote_rand.as_mut());
            self.remote_random.marshal(&mut writer)?;
        }

        let mut local_random = [0u8; HANDSHAKE_RANDOM_LENGTH];
        let mut remote_random = [0u8; HANDSHAKE_RANDOM_LENGTH];

        local_random.copy_from_slice(&local_rand);
        remote_random.copy_from_slice(&remote_rand);

        let local_epoch = self.local_epoch.load(Ordering::SeqCst);
        let remote_epoch = self.remote_epoch.load(Ordering::SeqCst);
        let sequence_number = {
            let lsn = self.local_sequence_number.lock().await;
            lsn[local_epoch as usize]
        };
        let cipher_suite_id = {
            let cipher_suite = self.cipher_suite.lock().await;
            match &*cipher_suite {
                Some(cipher_suite) => cipher_suite.id() as u16,
                None => return Err(Error::ErrCipherSuiteUnset),
            }
        };

        Ok(SerializedState {
            local_epoch,
            remote_epoch,
            local_random,
            remote_random,
            cipher_suite_id,
            master_secret: self.master_secret.clone(),
            sequence_number,
            srtp_protection_profile: self.srtp_protection_profile as u16,
            peer_certificates: self.peer_certificates.clone(),
            identity_hint: self.identity_hint.clone(),
            is_client: self.is_client,
        })
    }

    async fn deserialize(&mut self, serialized: &SerializedState) -> Result<()> {
        // Set epoch values
        self.local_epoch
            .store(serialized.local_epoch, Ordering::SeqCst);
        self.remote_epoch
            .store(serialized.remote_epoch, Ordering::SeqCst);
        {
            let mut lsn = self.local_sequence_number.lock().await;
            while lsn.len() <= serialized.local_epoch as usize {
                lsn.push(0);
            }
            lsn[serialized.local_epoch as usize] = serialized.sequence_number;
        }

        // Set random values
        let mut reader = Cursor::new(&serialized.local_random);
        self.local_random = HandshakeRandom::unmarshal(&mut reader)?;

        let mut reader = Cursor::new(&serialized.remote_random);
        self.remote_random = HandshakeRandom::unmarshal(&mut reader)?;

        self.is_client = serialized.is_client;

        // Set master secret
        self.master_secret = serialized.master_secret.clone();

        // Set cipher suite
        self.cipher_suite = Arc::new(Mutex::new(Some(cipher_suite_for_id(
            serialized.cipher_suite_id.into(),
        )?)));

        self.srtp_protection_profile = serialized.srtp_protection_profile.into();

        // Set remote certificate
        self.peer_certificates = serialized.peer_certificates.clone();
        self.identity_hint = serialized.identity_hint.clone();

        Ok(())
    }

    pub async fn init_cipher_suite(&mut self) -> Result<()> {
        let mut cipher_suite = self.cipher_suite.lock().await;
        if let Some(cipher_suite) = &mut *cipher_suite {
            if cipher_suite.is_initialized() {
                return Ok(());
            }

            let mut local_random = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(local_random.as_mut());
                self.local_random.marshal(&mut writer)?;
            }
            let mut remote_random = vec![];
            {
                let mut writer = BufWriter::<&mut Vec<u8>>::new(remote_random.as_mut());
                self.remote_random.marshal(&mut writer)?;
            }

            if self.is_client {
                cipher_suite.init(&self.master_secret, &local_random, &remote_random, true)
            } else {
                cipher_suite.init(&self.master_secret, &remote_random, &local_random, false)
            }
        } else {
            Err(Error::ErrCipherSuiteUnset)
        }
    }

    // marshal_binary is a binary.BinaryMarshaler.marshal_binary implementation
    pub async fn marshal_binary(&self) -> Result<Vec<u8>> {
        let serialized = self.serialize().await?;

        match bincode::serialize(&serialized) {
            Ok(enc) => Ok(enc),
            Err(err) => Err(Error::Other(err.to_string())),
        }
    }

    // unmarshal_binary is a binary.BinaryUnmarshaler.unmarshal_binary implementation
    pub async fn unmarshal_binary(&mut self, data: &[u8]) -> Result<()> {
        let serialized: SerializedState = match bincode::deserialize(data) {
            Ok(dec) => dec,
            Err(err) => return Err(Error::Other(err.to_string())),
        };
        self.deserialize(&serialized).await?;
        self.init_cipher_suite().await?;

        Ok(())
    }
}

#[async_trait]
impl KeyingMaterialExporter for State {
    /// export_keying_material returns length bytes of exported key material in a new
    /// slice as defined in RFC 5705.
    /// This allows protocols to use DTLS for key establishment, but
    /// then use some of the keying material for their own purposes
    async fn export_keying_material(
        &self,
        label: &str,
        context: &[u8],
        length: usize,
    ) -> std::result::Result<Vec<u8>, KeyingMaterialExporterError> {
        use KeyingMaterialExporterError::*;

        if self.local_epoch.load(Ordering::SeqCst) == 0 {
            return Err(HandshakeInProgress);
        } else if !context.is_empty() {
            return Err(ContextUnsupported);
        } else if INVALID_KEYING_LABELS.contains(&label) {
            return Err(ReservedExportKeyingMaterial);
        }

        let mut local_random = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(local_random.as_mut());
            self.local_random.marshal(&mut writer)?;
        }
        let mut remote_random = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(remote_random.as_mut());
            self.remote_random.marshal(&mut writer)?;
        }

        let mut seed = label.as_bytes().to_vec();
        if self.is_client {
            seed.extend_from_slice(&local_random);
            seed.extend_from_slice(&remote_random);
        } else {
            seed.extend_from_slice(&remote_random);
            seed.extend_from_slice(&local_random);
        }

        let cipher_suite = self.cipher_suite.lock().await;
        if let Some(cipher_suite) = &*cipher_suite {
            match prf_p_hash(&self.master_secret, &seed, length, cipher_suite.hash_func()) {
                Ok(v) => Ok(v),
                Err(err) => Err(Hash(err.to_string())),
            }
        } else {
            Err(CipherSuiteUnset)
        }
    }
}
