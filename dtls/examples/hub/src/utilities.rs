use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use dtls::crypto::{Certificate, CryptoPrivateKey};
use rcgen::KeyPair;
use rustls::pki_types::CertificateDer;
use thiserror::Error;

use super::*;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("block is not a private key, unable to load key")]
    ErrBlockIsNotPrivateKey,
    #[error("unknown key time in PKCS#8 wrapping, unable to load key")]
    ErrUnknownKeyTime,
    #[error("no private key found, unable to load key")]
    ErrNoPrivateKeyFound,
    #[error("block is not a certificate, unable to load certificates")]
    ErrBlockIsNotCertificate,
    #[error("no certificate found, unable to load certificates")]
    ErrNoCertificateFound,

    #[error("{0}")]
    Other(String),
}

impl From<Error> for dtls::Error {
    fn from(e: Error) -> Self {
        dtls::Error::Other(e.to_string())
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Other(e.to_string())
    }
}

/// chat simulates a simple text chat session over the connection
pub async fn chat(conn: Arc<dyn Conn + Send + Sync>) -> Result<(), Error> {
    let conn_rx = Arc::clone(&conn);
    tokio::spawn(async move {
        let mut b = vec![0u8; BUF_SIZE];

        while let Ok(n) = conn_rx.recv(&mut b).await {
            let msg = String::from_utf8(b[..n].to_vec()).expect("utf8");
            print!("Got message: {msg}");
        }

        Result::<(), Error>::Ok(())
    });

    let input = std::io::stdin();
    let mut reader = BufReader::new(input.lock());
    loop {
        let mut msg = String::new();
        match reader.read_line(&mut msg) {
            Ok(0) => return Ok(()),
            Err(err) => {
                println!("stdin read err: {err}");
                return Ok(());
            }
            _ => {}
        };
        if msg.trim() == "exit" {
            return Ok(());
        }

        let _ = conn.send(msg.as_bytes()).await;
    }
}

/// load_key_and_certificate reads certificates or key from file
pub fn load_key_and_certificate(
    key_path: PathBuf,
    certificate_path: PathBuf,
) -> Result<Certificate, Error> {
    let private_key = load_key(key_path)?;

    let certificate = load_certificate(certificate_path)?;

    Ok(Certificate {
        certificate,
        private_key,
    })
}

/// load_key Load/read key from file
pub fn load_key(path: PathBuf) -> Result<CryptoPrivateKey, Error> {
    let f = File::open(path)?;
    let mut reader = BufReader::new(f);
    let mut buf = vec![];
    reader.read_to_end(&mut buf)?;

    let s = String::from_utf8(buf).expect("utf8 of file");

    let key_pair = KeyPair::from_pem(s.as_str()).expect("key pair in file");

    Ok(CryptoPrivateKey::from_key_pair(&key_pair).expect("crypto key pair"))
}

/// load_certificate Load/read certificate(s) from file
pub fn load_certificate(path: PathBuf) -> Result<Vec<CertificateDer<'static>>, Error> {
    let f = File::open(path)?;

    let mut reader = BufReader::new(f);
    match rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>() {
        Ok(certs) => Ok(certs.into_iter().map(CertificateDer::from).collect()),
        Err(_) => Err(Error::ErrNoCertificateFound),
    }
}
