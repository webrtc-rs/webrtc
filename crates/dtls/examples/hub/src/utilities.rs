use super::*;

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
            println!("Got message: {}", msg);
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

/*
/// LoadKeyAndCertificate reads certificates or key from file
func LoadKeyAndCertificate(keyPath string, certificatePath string) (*tls.Certificate, error) {
    privateKey, err := LoadKey(keyPath)
    if err != nil {
        return nil, err
    }

    certificate, err := LoadCertificate(certificatePath)
    if err != nil {
        return nil, err
    }

    certificate.PrivateKey = privateKey

    return certificate, nil
}

/// LoadKey Load/read key from file
func LoadKey(path string) (crypto.PrivateKey, error) {
    rawData, err := ioutil.ReadFile(filepath.Clean(path))
    if err != nil {
        return nil, err
    }

    block, _ := pem.Decode(rawData)
    if block == nil || !strings.HasSuffix(block.Type, "PRIVATE KEY") {
        return nil, errBlockIsNotPrivateKey
    }

    if key, err := x509.ParsePKCS1PrivateKey(block.Bytes); err == nil {
        return key, nil
    }

    if key, err := x509.ParsePKCS8PrivateKey(block.Bytes); err == nil {
        switch key := key.(type) {
        case *rsa.PrivateKey, *ecdsa.PrivateKey:
            return key, nil
        default:
            return nil, errUnknownKeyTime
        }
    }

    if key, err := x509.ParseECPrivateKey(block.Bytes); err == nil {
        return key, nil
    }

    return nil, errNoPrivateKeyFound
}

/// LoadCertificate Load/read certificate(s) from file
func LoadCertificate(path string) (*tls.Certificate, error) {
    rawData, err := ioutil.ReadFile(filepath.Clean(path))
    if err != nil {
        return nil, err
    }

    var certificate tls.Certificate

    for {
        block, rest := pem.Decode(rawData)
        if block == nil {
            break
        }

        if block.Type != "CERTIFICATE" {
            return nil, errBlockIsNotCertificate
        }

        certificate.Certificate = append(certificate.Certificate, block.Bytes)
        rawData = rest
    }

    if len(certificate.Certificate) == 0 {
        return nil, errNoCertificateFound
    }

    return &certificate, nil
}
*/
