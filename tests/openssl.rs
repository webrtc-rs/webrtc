
mod mocks;

use mocks::dtls::{Config, CipherSuite};
use tokio::{
    sync::{mpsc, oneshot},
    time::{sleep, Duration},
    process::Command,
};

async fn run_server(
    config: Config,
    err_chan: mpsc::Sender<String>
) -> Result<(), String>
{
    let args = vec!(
        "s_server",
        "-dtls1_2",
        "-quiet",
        "-verify_quiet",
        "-verify_return_error",
    );
    let ciphers = cipher_openssl(&config);
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
    if config.certificates.len() > 0 {
        // TODO drop the temp file
        let (cert_pem, key_pem) = match write_temp_pem(&config) {
            Ok((c,k)) => (c,k),
            Err(e) => return Err(e.into()),
        };
        args.push(format!("-cert={}", cert_pem));
        args.push(format!("-key={}", key_pem));
    } else {
        args.push(format!("-nocert"));
    }
    let output = match Command::new("openssl").args(&args).output().await {
        Ok(o) => o,
        Err(e) => return Err(e.to_string()),
    };
    println!("{:?}", output);
    simple_read_write(stream).await
}

async fn run_client(
    config: Config,
    server_port: u16,
    server_ready: oneshot::Receiver<()>
) -> Result<(), String>
{
    let timeout = Duration::from_secs(1);
    let sleep = sleep(timeout);
    tokio::pin!(sleep);
    tokio::select! {
        _ = server_ready => {}
        _ = &mut sleep => {
            return Err(format!("timed out waiting for server after {:?}", timeout));
        }
    }
    let args = vec!(
		"s_client",
		"-dtls1_2",
		"-quiet",
		"-verify_quiet",
		"-verify_return_error",
		"-servername=localhost",
		format!("-connect=127.0.0.1:{}", server_port),
    );
    let cipher_suites = cipher_openssl(*config.cipher_suites);
    if cipher_suites.len() > 0 {
        args.push(format!("-cipher={}", cipher_suites))
    }
    if config.psk_id_hint.len() > 0 {
        args.push(format!("-psk_hint={}", config.psk_id_hint))
    }
    if config.certificates.len() > 0 {
        // TODO drop the temp file
        let (cert_pem, key_pem) = match write_temp_pem(&config) {
            Ok((c,k)) => (c,k),
            Err(e) => return Err(e.into()),
        };
        args.push(format!("-CAfile={}", cert_pem))
    }
    let output = match Command::new("openssl").args(&args).output().await {
        Ok(o) => o,
        Err(e) => return Err(e.to_string()),
    };
    println!("{:?}", output);
    simple_read_write(stream).await
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

pub fn write_temp_pem() -> Result<(FilePath, FilePath), String> {
    // TODO write pem output to files and return file handles
}

#[test]
pub fn openssl_e2e_simple() {
}

#[test]
pub fn openssl_e2e_simple_psk() {
}

#[test]
pub fn openssl_e2e_mtus() {
}
