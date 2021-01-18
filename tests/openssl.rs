
mod mocks;

use mocks::dtls::{self, Config, Event, Cert, CertConfig, CipherSuite, PSK, PSKIdHint, MTU};
use tokio::{sync::{mpsc, oneshot}, process::Command};
use std::io::Error;

async fn run_server(
    config: Config,
    err_chan: mpsc::Sender<String>
) -> Result<(), Error>
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
    if config.psk != None {
        let psk = match psk(None) {
            Ok(k) => k,
            Err(e) => return Err(e.into()),
        };
        args.push(format!("-psk={}", psk));
    }
    if config.psk_id_hint.length() > 0 {
        args.push(format!("-psk_hint={}", config.psk_id_hint))
    }
    if config.certificates.length() > 0 {
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
        Err(e) => return Err(e.into()),
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
    let ciphers = cipher_openssl(&config);
    if ciphers != "" {
        args.push(format!("-cipher={}", ciphers))
    }
    if config.psk != None {
        let psk = match psk(None) {
            Ok(k) => k,
            Err(e) => return Err(e.into()),
        };
        args.push(format!("-psk={}", psk));
    }
    if config.certificates.length() > 0 {
        // TODO drop the temp file
        let (cert_pem, key_pem) = match write_temp_pem(&config) {
            Ok((c,k)) => (c,k),
            Err(e) => return Err(e.into()),
        };
        args.push(format!("-CAfile={}", cert_pem))
    }
    let output = match Command::new("openssl").args(&args).output().await {
        Ok(o) => o,
        Err(e) => return Err(e.into()),
    };
    println!("{:?}", output);
    simple_read_write(stream).await
}

pub fn cipher_openssl(config: &Config) -> String {
    (match config.cipher_suites {
        dtls::TLS_ECDHE_ECDSA_WITH_AES_128_CCM        => "ECDHE-ECDSA-AES128-CCM",
		dtls::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8      => "ECDHE-ECDSA-AES128-CCM8",
		dtls::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => "ECDHE-ECDSA-AES128-GCM-SHA256",
		dtls::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256   => "ECDHE-RSA-AES128-GCM-SHA256",
		dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA    => "ECDHE-ECDSA-AES256-SHA",
		dtls::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA      => "ECDHE-RSA-AES128-SHA",
		dtls::TLS_PSK_WITH_AES_128_CCM                => "PSK-AES128-CCM",
		dtls::TLS_PSK_WITH_AES_128_CCM_8              => "PSK-AES128-CCM8",
		dtls::TLS_PSK_WITH_AES_128_GCM_SHA256         => "PSK-AES128-GCM-SHA256",
    }).to_string()
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
