use super::*;

use anyhow::Result;
use dtls::crypto::{Certificate, CryptoPrivateKey};
use rcgen::KeyPair;
use rustls::internal::pemfile::certs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
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

    #[allow(non_camel_case_types)]
    #[error("{0}")]
    new(String),
}

impl Error {
    pub fn equal(&self, err: &anyhow::Error) -> bool {
        err.downcast_ref::<Self>().map_or(false, |e| e == self)
    }
}

/// chat simulates a simple text chat session over the connection
pub async fn chat(conn: Arc<dyn Conn + Send + Sync>) -> Result<()> {
    let conn_rx = Arc::clone(&conn);
    tokio::spawn(async move {
        let mut b = vec![0u8; BUF_SIZE];

        while let Ok(n) = conn_rx.recv(&mut b).await {
            let msg = String::from_utf8(b[..n].to_vec())?;
            print!("Got message: {}", msg);
        }

        Result::<()>::Ok(())
    });

    let input = std::io::stdin();
    let mut reader = BufReader::new(input.lock());
    loop {
        let mut msg = String::new();
        match reader.read_line(&mut msg) {
            Ok(0) => return Ok(()),
            Err(err) => {
                println!("stdin read err: {}", err);
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
) -> Result<Certificate> {
    let private_key = load_key(key_path)?;

    let certificate = load_certificate(certificate_path)?;

    Ok(Certificate {
        certificate,
        private_key,
    })
}

/// load_key Load/read key from file
pub fn load_key(path: PathBuf) -> Result<CryptoPrivateKey> {
    let f = File::open(&path)?;
    let mut reader = BufReader::new(f);
    let mut buf = vec![];
    reader.read_to_end(&mut buf)?;

    let s = String::from_utf8(buf)?;

    let key_pair = KeyPair::from_pem(s.as_str())?;

    CryptoPrivateKey::from_key_pair(&key_pair)
}

/// load_certificate Load/read certificate(s) from file
pub fn load_certificate(path: PathBuf) -> Result<Vec<rustls::Certificate>> {
    let f = File::open(&path)?;

    let mut reader = BufReader::new(f);
    match certs(&mut reader) {
        Ok(ders) => Ok(ders),
        Err(_) => Err(Error::ErrNoCertificateFound.into()),
    }
}
