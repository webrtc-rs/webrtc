use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::error::{Error, Result};
use crate::peer_connection::math_rand_alpha;
use crate::stats::stats_collector::StatsCollector;
use crate::stats::{CertificateStats, StatsReportType};

use dtls::crypto::{CryptoPrivateKey, CryptoPrivateKeyKind};
use rcgen::{CertificateParams, KeyPair, RcgenError};
use ring::signature::{EcdsaKeyPair, Ed25519KeyPair, RsaKeyPair};
use sha2::{Digest, Sha256};
use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Certificate represents a x509Cert used to authenticate WebRTC communications.
#[derive(Clone)]
pub struct RTCCertificate {
    pub(crate) certificate: dtls::crypto::Certificate,
    pub(crate) stats_id: String,

    pem: String,
    expires: SystemTime,
}

/// Equals determines if two certificates are identical by comparing only certificate
impl PartialEq for RTCCertificate {
    fn eq(&self, other: &Self) -> bool {
        self.certificate == other.certificate
    }
}

impl RTCCertificate {
    /// from_params generates a new x509 compliant Certificate to be used
    /// by DTLS for encrypting data sent over the wire. This method differs from
    /// generate_certificate by allowing to specify a template x509.Certificate to
    /// be used in order to define certificate parameters.
    pub fn from_params(mut params: CertificateParams) -> Result<Self> {
        let key_pair = if let Some(key_pair) = params.key_pair.take() {
            if !key_pair.is_compatible(params.alg) {
                return Err(RcgenError::CertificateKeyPairMismatch.into());
            }
            key_pair
        } else {
            KeyPair::generate(params.alg)?
        };

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
        params.key_pair = Some(key_pair);

        let expires = if cfg!(target_arch = "arm") {
            // Workaround for issue overflow when adding duration to instant on armv7
            // https://github.com/webrtc-rs/examples/issues/5 https://github.com/chronotope/chrono/issues/343
            SystemTime::now().add(Duration::from_secs(172800)) //60*60*48 or 2 days
        } else {
            params.not_after.into()
        };

        let x509_cert = rcgen::Certificate::from_params(params)?;
        let certificate = x509_cert.serialize_der()?;

        Ok(RTCCertificate {
            certificate: dtls::crypto::Certificate {
                certificate: vec![rustls::Certificate(certificate)],
                private_key,
            },
            stats_id: format!(
                "certificate-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64
            ),
            pem: x509_cert.serialize_pem()?,
            expires,
        })
    }

    /// Constructs a `RTCCertificate` from an existing certificate.
    ///
    /// Use this method when you have a persistent certificate (i.e. you don't want to generate a
    /// new one for each DTLS connection).
    pub fn from_existing(
        certificate: dtls::crypto::Certificate,
        pem: &str,
        expires: SystemTime,
    ) -> Self {
        Self {
            certificate,
            stats_id: format!(
                "certificate-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64
            ),
            pem: pem.to_owned(),
            expires,
        }
    }

    /// expires returns the timestamp after which this certificate is no longer valid.
    pub fn expires(&self) -> SystemTime {
        self.expires
    }

    /// pem returns the certificate encoded as two PEM blocks: one for the X509 certificate and the
    /// other for the private key.
    pub fn pem(&self) -> &str {
        &self.pem
    }

    /// get_fingerprints returns certificate fingerprints, one of which
    /// is computed with the digest algorithm used in the certificate signature.
    pub fn get_fingerprints(&self) -> Result<Vec<RTCDtlsFingerprint>> {
        let mut fingerpints = vec![];

        for certificate in &self.certificate.certificate {
            let mut h = Sha256::new();
            h.update(&certificate.0);
            let hashed = h.finalize();
            let values: Vec<String> = hashed.iter().map(|x| format! {"{:02x}", x}).collect();

            fingerpints.push(RTCDtlsFingerprint {
                algorithm: "sha-256".to_owned(),
                value: values.join(":"),
            });
        }

        Ok(fingerpints)
    }

    /// from_key_pair causes the creation of an X.509 certificate and
    /// corresponding private key.
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

        /*log::debug!(
            "from_key: alg:{:?}, nb:{:?}, na:{:?}, sn:{:?}, san:{:?}, dn:{:?}, ic:{:?}, ku:{:?}, eku:{:?}, ce:{:?}, uakie:{:?}, kim:{:?}, kp:{:?}",
            params.alg,
            params.not_before,
            params.not_after,
            params.serial_number,
            params.subject_alt_names,
            params.distinguished_name,
            params.is_ca,
            params.key_usages,
            params.extended_key_usages,
            params.custom_extensions,
            params.use_authority_key_identifier_extension,
            params.key_identifier_method,
            params.key_pair,
        );*/

        RTCCertificate::from_params(params)
    }

    /*TODO:
    // CertificateFromX509 creates a new WebRTC Certificate from a given PrivateKey and Certificate
    //
    // This can be used if you want to share a certificate across multiple PeerConnections
    func CertificateFromX509(privateKey crypto.PrivateKey, certificate *x509.Certificate) Certificate {
        return Certificate{privateKey, certificate, fmt.Sprintf("certificate-%d", time.Now().UnixNano())}
    }
    */

    pub(crate) async fn collect_stats(&self, collector: &StatsCollector) {
        let fingerprints = self.get_fingerprints().unwrap();
        if let Some(fingerprint) = fingerprints.into_iter().next() {
            let stats = CertificateStats::new(self, fingerprint);
            collector.insert(
                self.stats_id.clone(),
                StatsReportType::CertificateStats(stats),
            );
        }
    }

    /// from_pem creates a fresh certificate based on a string containing
    /// pem blocks fort the private key and x509 certificate
    pub fn from_pem(pem_str: &str, key_pair: KeyPair) -> Result<Self> {
        let params = CertificateParams::from_ca_cert_pem(pem_str, key_pair)?;

        /*log::debug!(
            "from_pem: alg:{:?}, nb:{:?}, na:{:?}, sn:{:?}, san:{:?}, dn:{:?}, ic:{:?}, ku:{:?}, eku:{:?}, ce:{:?}, uakie:{:?}, kim:{:?}, kp:{:?}",
            params.alg,
            params.not_before,
            params.not_after,
            params.serial_number,
            params.subject_alt_names,
            params.distinguished_name,
            params.is_ca,
            params.key_usages,
            params.extended_key_usages,
            params.custom_extensions,
            params.use_authority_key_identifier_extension,
            params.key_identifier_method,
            params.key_pair,
        );*/

        RTCCertificate::from_params(params)
    }
}

#[cfg(test)]
mod test {
    use super::*;

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

        let cert = RTCCertificate::from_key_pair(kp)?;
        assert!(cert.pem().contains("CERTIFICATE"));

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
        let cert1_pem = cert1.pem();

        let kp2 = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let _cert2 = RTCCertificate::from_key_pair(kp2)?;

        let kp3 = KeyPair::from_pem(kp1_pem.as_str())?;
        let kp3_pem = kp3.serialize_pem();
        let _cert3 = RTCCertificate::from_pem(cert1_pem, kp3)?;

        assert_eq!(kp1_pem, kp3_pem);
        //assert!(cert1 != cert2);
        //TODO: assert!(cert1 == cert3);

        Ok(())
    }

    #[test]
    fn test_generate_certificate_expires() -> Result<()> {
        let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let cert = RTCCertificate::from_key_pair(kp)?;

        let now = SystemTime::now();
        assert!(cert.expires().duration_since(now).is_ok());

        //TODO: x509Cert := CertificateFromX509(sk, &x509.Certificate{})
        //assert.NotNil(t, x509Cert)
        //assert.Contains(t, x509Cert.statsID, "certificate")

        Ok(())
    }

    #[test]
    fn test_pem() -> Result<()> {
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

        let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let kp_pem = kp.serialize_pem();
        let cert = RTCCertificate::from_key_pair(kp)?;
        let pem = cert.pem();
        log::info!("{}", pem);

        let kp2 = KeyPair::from_pem(kp_pem.as_str())?;
        let kp2_pem = kp2.serialize_pem();
        let cert2 = RTCCertificate::from_pem(pem, kp2)?;
        let pem2 = cert2.pem();
        log::info!("{}", pem2);

        assert_eq!(kp_pem, kp2_pem);
        //TODO: assert_eq!(pem, pem2);

        Ok(())
    }

    #[test]
    fn test_from_existing() -> Result<()> {
        // NOTE `dtls_cert` key pair and `key_pair` are different, but it's fine here.
        let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let dtls_cert = dtls::crypto::Certificate::generate_self_signed(["localhost".to_owned()])?;

        let expires = SystemTime::now();
        let pem = key_pair.serialize_pem();

        let cert = RTCCertificate::from_existing(dtls_cert, &pem, expires);

        assert_ne!("", cert.stats_id);
        assert_eq!(expires, cert.expires());
        assert_eq!(pem, cert.pem());

        Ok(())
    }
}
