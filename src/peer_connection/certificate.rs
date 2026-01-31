use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dtls::crypto::{CryptoPrivateKey, CryptoPrivateKeyKind};
use rcgen::{CertificateParams, KeyPair};
use ring::rand::SystemRandom;
use ring::rsa;
use ring::signature::{EcdsaKeyPair, Ed25519KeyPair};
use sha2::{Digest, Sha256};

use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::error::{Error, Result};
use crate::peer_connection::math_rand_alpha;
use crate::stats::stats_collector::StatsCollector;
use crate::stats::{CertificateStats, StatsReportType};

/// Certificate represents a X.509 certificate used to authenticate WebRTC communications.
///
/// ## Specifications
///
/// * [MDN]
/// * [W3C]
///
/// [MDN]: https://developer.mozilla.org/en-US/docs/Web/API/RTCCertificate
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-rtccertificate
#[derive(Clone, Debug)]
pub struct RTCCertificate {
    /// DTLS certificate.
    pub(crate) dtls_certificate: dtls::crypto::Certificate,
    /// Timestamp after which this certificate is no longer valid.
    pub(crate) expires: SystemTime,
    /// Certificate's ID used for statistics.
    ///
    /// Example: "certificate-1667202302853538793"
    ///
    /// See [`CertificateStats`].
    pub(crate) stats_id: String,
}

impl PartialEq for RTCCertificate {
    fn eq(&self, other: &Self) -> bool {
        self.dtls_certificate == other.dtls_certificate
    }
}

impl RTCCertificate {
    /// Generates a new certificate from the given parameters.
    ///
    /// See [`rcgen::Certificate::from_params`].
    fn from_params(params: CertificateParams, key_pair: KeyPair) -> Result<Self> {
        let not_after = params.not_after;

        let x509_cert = params.self_signed(&key_pair).unwrap();
        let serialized_der = key_pair.serialize_der();

        let private_key = if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ed25519(
                    Ed25519KeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::new(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Ecdsa256(
                    EcdsaKeyPair::from_pkcs8(
                        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
                        &serialized_der,
                        &SystemRandom::new(),
                    )
                    .map_err(|e| Error::new(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    rsa::KeyPair::from_pkcs8(&serialized_der)
                        .map_err(|e| Error::new(e.to_string()))?,
                ),
                serialized_der,
            }
        } else {
            return Err(Error::new("Unsupported key_pair".to_owned()));
        };

        let expires = if cfg!(target_arch = "arm") {
            // Workaround for issue overflow when adding duration to instant on armv7
            // https://github.com/webrtc-rs/examples/issues/5 https://github.com/chronotope/chrono/issues/343
            SystemTime::now().add(Duration::from_secs(172800)) //60*60*48 or 2 days
        } else {
            not_after.into()
        };

        Ok(Self {
            dtls_certificate: dtls::crypto::Certificate {
                certificate: vec![x509_cert.der().to_owned()],
                private_key,
            },
            expires,
            stats_id: gen_stats_id(),
        })
    }

    /// Generates a new certificate with default [`CertificateParams`] using the given keypair.
    pub fn from_key_pair(key_pair: KeyPair) -> Result<Self> {
        if !(key_pair.is_compatible(&rcgen::PKCS_ED25519)
            || key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256)
            || key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256))
        {
            return Err(Error::new("Unsupported key_pair".to_owned()));
        }

        RTCCertificate::from_params(
            CertificateParams::new(vec![math_rand_alpha(16)]).unwrap(),
            key_pair,
        )
    }

    /// Parses a certificate from the ASCII PEM format.
    #[cfg(feature = "pem")]
    pub fn from_pem(pem_str: &str) -> Result<Self> {
        let mut pem_blocks = pem_str.split("\n\n");
        let first_block = if let Some(b) = pem_blocks.next() {
            b
        } else {
            return Err(Error::InvalidPEM("empty PEM".into()));
        };
        let expires_pem =
            pem::parse(first_block).map_err(|e| Error::new(format!("can't parse PEM: {e}")))?;
        if expires_pem.tag() != "EXPIRES" {
            return Err(Error::InvalidPEM(format!(
                "invalid tag (expected: 'EXPIRES', got '{}')",
                expires_pem.tag()
            )));
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&expires_pem.contents()[..8]);
        let expires = if let Some(e) =
            SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(u64::from_le_bytes(bytes)))
        {
            e
        } else {
            return Err(Error::InvalidPEM("failed to calculate SystemTime".into()));
        };
        let dtls_certificate =
            dtls::crypto::Certificate::from_pem(&pem_blocks.collect::<Vec<&str>>().join("\n\n"))?;
        Ok(RTCCertificate::from_existing(dtls_certificate, expires))
    }

    /// Builds a [`RTCCertificate`] using the existing DTLS certificate.
    ///
    /// Use this method when you have a persistent certificate (i.e. you don't want to generate a
    /// new one for each DTLS connection).
    ///
    /// NOTE: ID used for statistics will be different as it's neither derived from the given
    /// certificate nor persisted along it when using [`RTCCertificate::serialize_pem`].
    pub fn from_existing(dtls_certificate: dtls::crypto::Certificate, expires: SystemTime) -> Self {
        Self {
            dtls_certificate,
            expires,
            // TODO: figure out if it needs to be persisted
            stats_id: gen_stats_id(),
        }
    }

    /// Serializes the certificate (including the private key) in PKCS#8 format in PEM.
    #[cfg(any(doc, feature = "pem"))]
    pub fn serialize_pem(&self) -> String {
        // Encode `expires` as a PEM block.
        //
        // TODO: serialize as nanos when https://github.com/rust-lang/rust/issues/103332 is fixed.
        let expires_pem = pem::Pem::new(
            "EXPIRES".to_string(),
            self.expires
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("expires to be valid")
                .as_secs()
                .to_le_bytes()
                .to_vec(),
        );
        format!(
            "{}\n{}",
            pem::encode(&expires_pem),
            self.dtls_certificate.serialize_pem()
        )
    }

    /// get_fingerprints returns a SHA-256 fingerprint of this certificate.
    ///
    /// TODO: return a fingerprint computed with the digest algorithm used in the certificate
    /// signature.
    pub fn get_fingerprints(&self) -> Vec<RTCDtlsFingerprint> {
        let mut fingerprints = Vec::new();

        for c in &self.dtls_certificate.certificate {
            let mut h = Sha256::new();
            h.update(c.as_ref());
            let hashed = h.finalize();
            let values: Vec<String> = hashed.iter().map(|x| format! {"{x:02x}"}).collect();

            fingerprints.push(RTCDtlsFingerprint {
                algorithm: "sha-256".to_owned(),
                value: values.join(":"),
            });
        }

        fingerprints
    }

    pub(crate) async fn collect_stats(&self, collector: &StatsCollector) {
        if let Some(fingerprint) = self.get_fingerprints().into_iter().next() {
            let stats = CertificateStats::new(self, fingerprint);
            collector.insert(
                self.stats_id.clone(),
                StatsReportType::CertificateStats(stats),
            );
        }
    }
}

fn gen_stats_id() -> String {
    format!(
        "certificate-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_certificate_rsa() -> Result<()> {
        let key_pair = KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256);
        assert!(key_pair.is_err(), "RcgenError::KeyGenerationUnavailable");

        Ok(())
    }

    #[test]
    fn test_generate_certificate_ecdsa() -> Result<()> {
        let kp = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let _cert = RTCCertificate::from_key_pair(kp)?;

        Ok(())
    }

    #[test]
    fn test_generate_certificate_eddsa() -> Result<()> {
        let kp = KeyPair::generate_for(&rcgen::PKCS_ED25519)?;
        let _cert = RTCCertificate::from_key_pair(kp)?;

        Ok(())
    }

    #[test]
    fn test_certificate_equal() -> Result<()> {
        let kp1 = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert1 = RTCCertificate::from_key_pair(kp1)?;

        let kp2 = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert2 = RTCCertificate::from_key_pair(kp2)?;

        assert_ne!(cert1, cert2);

        Ok(())
    }

    #[test]
    fn test_generate_certificate_expires_and_stats_id() -> Result<()> {
        let kp = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert = RTCCertificate::from_key_pair(kp)?;

        let now = SystemTime::now();
        assert!(cert.expires.duration_since(now).is_ok());
        assert!(cert.stats_id.contains("certificate"));

        Ok(())
    }

    #[cfg(feature = "pem")]
    #[test]
    fn test_certificate_serialize_pem_and_from_pem() -> Result<()> {
        let kp = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert = RTCCertificate::from_key_pair(kp)?;

        let pem = cert.serialize_pem();
        let loaded_cert = RTCCertificate::from_pem(&pem)?;

        assert_eq!(loaded_cert, cert);

        Ok(())
    }
}
