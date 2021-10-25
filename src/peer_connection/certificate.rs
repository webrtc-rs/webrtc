use crate::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::error::{Error, Result};
use crate::utilities::math_rand_alpha;

use dtls::crypto::{CryptoPrivateKey, CryptoPrivateKeyKind};
use rcgen::{CertificateParams, KeyPair, RcgenError};
use ring::signature::{EcdsaKeyPair, Ed25519KeyPair, RsaKeyPair};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Certificate represents a x509Cert used to authenticate WebRTC communications.
pub struct RTCCertificate {
    pub(crate) certificate: dtls::crypto::Certificate,
    pub(crate) stats_id: String,
    pub(crate) x509_cert: rcgen::Certificate,
    pub(crate) expires: SystemTime,
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

        let expires = params.not_after.into();
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
            x509_cert,
            expires,
        })
    }

    /// expires returns the timestamp after which this certificate is no longer valid.
    pub fn expires(&self) -> SystemTime {
        self.expires
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

    func (c Certificate) collectStats(report *statsReportCollector) error {
        report.Collecting()

        fingerPrintAlgo, err := c.get_fingerprints()
        if err != nil {
            return err
        }

        base64Certificate := base64.RawURLEncoding.EncodeToString(c.x509Cert.Raw)

        stats := CertificateStats{
            Timestamp:            statsTimestampFrom(time.Now()),
            Type:                 StatsTypeCertificate,
            ID:                   c.statsID,
            Fingerprint:          fingerPrintAlgo[0].Value,
            FingerprintAlgorithm: fingerPrintAlgo[0].Algorithm,
            Base64Certificate:    base64Certificate,
            IssuerCertificateID:  c.x509Cert.Issuer.String(),
        }

        report.Collect(stats.ID, stats)
        return nil
    }*/

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

    /// PEM returns the certificate encoded as two pem block: once for the X509
    /// certificate and the other for the private key
    pub fn pem(&self) -> Result<String> {
        Ok(self.x509_cert.serialize_pem()?)
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
        let cert_pem = cert.x509_cert.serialize_pem()?;

        //_, err = tls.X509KeyPair(certPEM, skPEM)
        */
        Ok(())
    }

    #[test]
    fn test_generate_certificate_ecdsa() -> Result<()> {
        let kp = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let kp_pem = kp.serialize_pem();
        assert!(kp_pem.contains("PRIVATE KEY"));

        let cert = RTCCertificate::from_key_pair(kp)?;
        let cert_pem = cert.x509_cert.serialize_pem()?;
        assert!(cert_pem.contains("CERTIFICATE"));

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
        let cert1_pem = cert1.pem()?;

        let kp2 = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let _cert2 = RTCCertificate::from_key_pair(kp2)?;

        let kp3 = KeyPair::from_pem(kp1_pem.as_str())?;
        let kp3_pem = kp3.serialize_pem();
        let _cert3 = RTCCertificate::from_pem(cert1_pem.as_str(), kp3)?;

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
        let pem = cert.pem()?;
        log::info!("{}", pem);

        let kp2 = KeyPair::from_pem(kp_pem.as_str())?;
        let kp2_pem = kp2.serialize_pem();
        let cert2 = RTCCertificate::from_pem(pem.as_str(), kp2)?;
        let pem2 = cert2.pem()?;
        log::info!("{}", pem2);

        assert_eq!(kp_pem, kp2_pem);
        //TODO: assert_eq!(pem, pem2);

        Ok(())
    }
}
