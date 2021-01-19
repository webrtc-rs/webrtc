
mod mocks;

use mocks::{
    pem,
    x509,
    dtls::{self, Config, CipherSuite, Certificate, TcpPort},
    test_runner::{simple_read_write, check_comms},
};
use tokio::{
    sync::oneshot,
    time::{sleep, Duration},
    process::Command,
};
use std::{
    env,
    fs::{self, OpenOptions},
};

/// Create server cert and key files in a temp dir
/// Returns a channel to delete the temp dir
async fn create_server_openssl(config: Config)
-> Result<Option<oneshot::Sender<()>>, String>
{
    // Determine server openssl args
    let args = vec!(
        "s_server",
        "-dtls1_2",
        "-quiet",
        "-verify_quiet",
        "-verify_return_error",
    );
    let ciphers = cipher_openssl(*config.cipher_suites);
    if ciphers != "" {
        args.push(format!("-cipher={}", ciphers))
    }
    match config.psk_callback {
        Some(cb) => match cb(None) {
            Ok(psk) => args.push(format!("-psk={}", psk)),
            Err(e) => return Err(e),
        }
        None => {}
    }
    if config.psk_id_hint.len() > 0 {
        args.push(format!("-psk_hint={}", config.psk_id_hint))
    }
    let mut cleanup: Option<oneshot::Sender<()>> = None;
    if config.certificates.len() > 0 {
        let (cert_pem, key_pem, release_certs) = match write_temp_pem(config.certificates[0]) {
            Ok((c,k,f)) => (c,k,f),
            Err(e) => return Err(e.into()),
        };
        cleanup = Some(release_certs);
        args.push(format!("-cert={}", cert_pem));
        args.push(format!("-key={}", key_pem));
    } else {
        args.push(format!("-nocert"));
    }

    // Run server openssl command
    let output = match Command::new("openssl").args(&args).output().await {
        Ok(o) => o,
        Err(e) => return Err(e.to_string()),
    };
    println!("{:?}", output);
    return Ok(cleanup);
}

async fn create_client_openssl(config: Config, port: TcpPort)
-> Result<Option<oneshot::Sender<()>>, String>
{
    // Determine client openssl args
    let args = vec!(
		"s_client",
		"-dtls1_2",
		"-quiet",
		"-verify_quiet",
		"-verify_return_error",
		"-servername=localhost",
		format!("-connect=127.0.0.1:{}", port),
    );
    let cipher_suites = cipher_openssl(*config.cipher_suites);
    if cipher_suites.len() > 0 {
        args.push(format!("-cipher={}", cipher_suites))
    }
    if config.psk_id_hint.len() > 0 {
        args.push(format!("-psk_hint={}", config.psk_id_hint))
    }
    let mut cleanup: Option<oneshot::Sender<()>> = None;
    if config.certificates.len() > 0 {
        // TODO drop the temp file
        let (cert_pem, key_pem, release_certs) = match write_temp_pem(config.certificates[0]) {
            Ok((c,k,f)) => (c,k,f),
            Err(e) => return Err(e.to_string()),
        };
        cleanup = Some(release_certs);
        args.push(format!("-cert={}", cert_pem));
        args.push(format!("-key={}", key_pem));
    } else {
        args.push(format!("-nocert"));
    }

    // Run client openssl command
    let output = match Command::new("openssl").args(&args).output().await {
        Ok(o) => o,
        Err(e) => return Err(e.to_string()),
    };
    println!("{:?}", output);
    return Ok(cleanup);
}

async fn run_server(
    config: Config,
    server_port: TcpPort,
    ready_tx: oneshot::Sender<()>,
) -> Result<(), String>
{
    // Listen for new connections
    let listen = dtls::listen(
        "udp".to_string(),
        "127.0.0.1".to_string(),
        server_port,
        config
    );
    let listener = match listen.await {
        Ok(listener) => listener,
        Err(e) => return Err(e.to_string()),
    };

    // Notify client
    match ready_tx.send(()) {
        Ok(_) => {},
        Err(_) => return Err("failed to send server ready signal".to_string()),
    }

    // Accept client connection
    // TODO verify _addr
    let (stream, _addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => return Err(e.to_string()),
    };

    // create server ssl files
    let cleanup = match create_server_openssl(config).await {
        Ok(c) => c,
        Err(e) => return Err(e.to_string())
    };

    // Read and write on the stream
    simple_read_write(stream).await;

    // Cleanup
    match cleanup {
        Some(x) => { x.send(()); },
        None => {},
    }
    Ok(())
}

async fn run_client(
    config: Config,
    port: TcpPort,
    server_ready: oneshot::Receiver<()>
) -> Result<(), String>
{
    // Wait for server to start listening
    let timeout = Duration::from_secs(1);
    let sleep = sleep(timeout);
    tokio::pin!(sleep);
    tokio::select! {
        _ = server_ready => {}
        _ = &mut sleep => {
            return Err(format!("timed out waiting for server after {:?}", timeout));
        }
    }

    // Create client openssl files
    let cleanup = match create_client_openssl(config, port).await {
        Ok(c) => c,
        Err(e) => return Err(e.to_string())
    };
    
    // Dial the server
    let dial = dtls::dial(
        "udp".to_string(),
        "127.0.0.1".to_string(),
        port,
        config
    );
    let stream = match dial.await {
        Ok(stream) => stream,
        Err(e) => return Err(e.to_string()),
    };

    // Send and recv on stream
    simple_read_write(stream).await;

    // Cleanup
    match cleanup {
        Some(x) => { x.send(()); },
        None => {},
    }
    Ok(())
}

pub fn cipher_openssl(cipher_suites: Vec<CipherSuite>) -> String {
    cipher_suites.iter().map( |cs| {
        (match cs {
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CCM        => "ECDHE-ECDSA-AES128-CCM",
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8      => "ECDHE-ECDSA-AES128-CCM8",
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => "ECDHE-ECDSA-AES128-GCM-SHA256",
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256   => "ECDHE-RSA-AES128-GCM-SHA256",
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA    => "ECDHE-ECDSA-AES256-SHA",
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA      => "ECDHE-RSA-AES128-SHA",
            CipherSuite::TLS_PSK_WITH_AES_128_CCM                => "PSK-AES128-CCM",
            CipherSuite::TLS_PSK_WITH_AES_128_CCM_8              => "PSK-AES128-CCM8",
            CipherSuite::TLS_PSK_WITH_AES_128_GCM_SHA256         => "PSK-AES128-GCM-SHA256",
        }).to_string()
    }).fold("".to_string(), |acc, x| format!("{},{}", acc, x))
}

pub fn write_temp_pem<F>(cert: Certificate)
-> Result<(String, String, oneshot::Sender<()>), String>
{
    let mut dir = env::temp_dir();
    dir.push("dtls-webrtc-rs-test");

    let der_bytes = cert.certificate[0];
    let cert_path = dir.clone();
    cert_path.push("cert.pem");
    let cert_out = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(cert_path)
        .unwrap();
    match pem::encode(cert_out, pem::Block::new("CERTIFICATE".to_string(), der_bytes)) {
        Ok(_) => {},
        Err(e) => return Err(e.to_string())
    }
    
    let key_path = dir.clone();
    key_path.push("key.pem");
    let key_out = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(key_path)
        .unwrap();
    let priv_key = cert.private_key;
    let priv_bytes = match x509::marshal_pkcs8_private_key(priv_key) {
        Ok(b) => b,
        Err(e) => return Err(e.to_string())
    };
    match pem::encode(key_out, pem::Block::new("PRIVATE KEY".to_string(), priv_bytes)) {
        Ok(_) => {},
        Err(e) => return Err(e.to_string())
    }
    
    let (tx, rx) = oneshot::channel();
    let release_certs = tokio::spawn( async move {
        rx.await;
        fs::remove_dir_all(dir);
    });
    Ok((
        cert_path
            .into_os_string()
            .into_string()
            .unwrap(),
        key_path
            .into_os_string()
            .into_string()
            .unwrap(),
        tx
    ))
}

#[test]
pub fn openssl_e2e_simple() {
    check_comms(run_client_basic, run_server_openssl);
    check_comms(run_client_openssl, run_server_basic);
}

#[test]
pub fn openssl_e2e_simple_psk() {
}

#[test]
pub fn openssl_e2e_mtus() {
}
