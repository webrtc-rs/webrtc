use dtls::crypto::{CryptoPrivateKey, CryptoPrivateKeyKind};
use rcgen::{CertificateParams, KeyPair};
use ring::signature::{EcdsaKeyPair, Ed25519KeyPair, RsaKeyPair};
use sha2::{Digest, Sha256};

use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::error::{Error, Result};
use crate::peer_connection::math_rand_alpha;
use crate::stats::stats_collector::StatsCollector;
use crate::stats::{CertificateStats, StatsReportType};

/// Certificate represents a X.509 certificate used to authenticate WebRTC communications.
#[derive(Clone, Debug)]
pub struct RTCCertificate {
    /// DTLS certificate.
    pub dtls_certificate: dtls::crypto::Certificate,
    /// Timestamp after which this certificate is no longer valid.
    pub expires: SystemTime,
    /// ID used for statistics.
    pub stats_id: String,
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
    pub fn from_params(params: CertificateParams) -> Result<Self> {
        let not_after = params.not_after;
        let x509_cert = rcgen::Certificate::from_params(params)?;

        let key_pair = x509_cert.get_key_pair();
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
                    )
                    .map_err(|e| Error::new(e.to_string()))?,
                ),
                serialized_der,
            }
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            CryptoPrivateKey {
                kind: CryptoPrivateKeyKind::Rsa256(
                    RsaKeyPair::from_pkcs8(&serialized_der)
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
                certificate: vec![rustls::Certificate(x509_cert.serialize_der()?)],
                private_key,
            },
            stats_id: format!(
                "certificate-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64
            ),
            expires,
        })
    }

    /// Generates a new certificate with default [`CertificateParams`] using the given keypair.
    pub fn from_key_pair(key_pair: KeyPair) -> Result<Self> {
        let mut params = CertificateParams::new(vec![math_rand_alpha(16)]);

        if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            params.alg = &rcgen::PKCS_ED25519;
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            params.alg = &rcgen::PKCS_RSA_SHA256;
        } else {
            return Err(Error::new("Unsupported key_pair".to_owned()));
        };
        params.key_pair = Some(key_pair);

        RTCCertificate::from_params(params)
    }

    /// Parses a ca certificate from the ASCII PEM format for signing and uses it to create a new
    /// certificate.
    ///
    /// See [`CertificateParams::from_ca_cert_pem`].
    pub fn from_pem(pem_str: &str, key_pair: KeyPair) -> Result<Self> {
        let params = CertificateParams::from_ca_cert_pem(pem_str, key_pair)?;
        RTCCertificate::from_params(params)
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
            let values: Vec<String> = hashed.iter().map(|x| format! {"{:02x}", x}).collect();

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

    /*TODO:
    // CertificateFromX509 creates a new WebRTC Certificate from a given PrivateKey and Certificate
    //
    // This can be used if you want to share a certificate across multiple PeerConnections
    func CertificateFromX509(privateKey crypto.PrivateKey, certificate *x509.Certificate) Certificate {
        return Certificate{privateKey, certificate, fmt.Sprintf("certificate-%d", time.Now().UnixNano())}
    }
    */
}

#[cfg(test)]
mod test {
    use super::*;
    use pem::Pem;

    fn pem(c: &RTCCertificate) -> String {
        let p = Pem {
            tag: "CERTIFICATE".to_string(),
            contents: c.dtls_certificate.certificate[0].0.clone(),
        };
        pem::encode(&p)
    }

    #[test]
    fn test_generate_certificate_rsa() -> Result<()> {
        let kp = KeyPair::generate(&rcgen::PKCS_RSA_SHA256);
        assert!(kp.is_err(), "RcgenError::KeyGenerationUnavailable");
        /*
        let kp_pem = kp.serialize_pem();

        let cert = Certificate::generate_certificate(kp)?;

        //_, err = tls.X509KeyPair(cert.pem(), skPEM)
        */
        Ok(())
    }

    #[test]
    fn test_generate_certificate_ecdsa() -> Result<()> {
        let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let kp_pem = kp.serialize_pem();
        assert!(kp_pem.contains("PRIVATE KEY"));

        RTCCertificate::from_key_pair(kp)?;

        //_, err = tls.X509KeyPair(certPEM, skPEM)

        Ok(())
    }

    //use log::LevelFilter;
    //use std::io::Write;

    #[test]
    fn test_generate_certificate_equal() -> Result<()> {
        /*env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} [{}] {} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.level(),
                chrono::Local::now().format("%H:%M:%S.%6f"),
                record.args()
            )
        })
        .filter(None, LevelFilter::Trace)
        .init();*/

        let kp1 = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let kp1_pem = kp1.serialize_pem();
        let cert1 = RTCCertificate::from_key_pair(kp1)?;
        let cert1_pem = pem(&cert1);

        let kp2 = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert2 = RTCCertificate::from_key_pair(kp2)?;

        let kp3 = KeyPair::from_pem(kp1_pem.as_str())?;
        let kp3_pem = kp3.serialize_pem();
        let cert3 = RTCCertificate::from_pem(&cert1_pem, kp3)?;

        assert_eq!(kp1_pem, kp3_pem);
        assert_ne!(cert1, cert2);
        // TODO: assert!(cert1 == cert3);
        //
        // Certificates are not equal because `from_pem` uses `cert1` as a CA.
        assert_ne!(cert1, cert3);

        Ok(())
    }

    #[test]
    fn test_generate_certificate_expires() -> Result<()> {
        let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert = RTCCertificate::from_key_pair(kp)?;

        let now = SystemTime::now();
        assert!(cert.expires.duration_since(now).is_ok());

        //TODO: x509Cert := CertificateFromX509(sk, &x509.Certificate{})
        //assert.NotNil(t, x509Cert)
        //assert.Contains(t, x509Cert.statsID, "certificate")

        Ok(())
    }
}
