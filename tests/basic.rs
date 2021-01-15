
mod mocks;

use mocks::dtls::{self, Config, Cert, CertConfig, CipherSuite, PSK, PSKIdHint, MTU};
use mocks::transport;
use tokio;
use tokio::time::{sleep, Duration};

const TEST_MESSAGE: &str = "Hello world";
const TEST_TIME_LIMIT: Duration = Duration::from_secs(5);
const MESSAGE_RETRY: Duration = Duration::from_millis(200);

pub async fn random_port() -> u16 {
    let addr = "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap();
    let sock = match tokio::net::UdpSocket::bind(addr).await {
        Ok(s) => s,
        Err(e) => panic!(e),
    };
    let local_addr: std::net::SocketAddr = match sock.local_addr() {
        Ok(s) => s,
        Err(e) => panic!(e),
    };
    local_addr.port()
}

#[test]
pub fn e2e_basic() {
    let cipher_suites: [u16; 2] = [
        dtls::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
        dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
    ];
    for cipher in cipher_suites.iter() {
        let cert = Cert::new(CertConfig::new().self_signed());
        let conf = Config::new()
            .cipher_suite(*cipher)
            .cert(cert)
            .insecure_skip_verify();
        check_comms(conf);
    }
}

#[test]
pub fn e2e_simple_psk() {
    let cipher_suites: [CipherSuite; 3] = [
        dtls::TLS_PSK_WITH_AES_128_CCM,
        dtls::TLS_PSK_WITH_AES_128_CCM_8,
        dtls::TLS_PSK_WITH_AES_128_GCM_SHA256,
    ];
    for cipher in cipher_suites.iter() {
        let (psk, psk_id_hint) = create_psk();
        let conf = Config::new()
            .psk(psk, psk_id_hint)
            .cipher_suite(*cipher);
        check_comms(conf);
    }      
}

#[test]
pub fn e2e_mtu() {
    let mtus: &'static [MTU; 3] = &[
        10_000,
        1000,
        100
    ];
    for mtu in mtus.iter() {
        let cert = Cert::new(CertConfig::new().self_signed().host("localhost"));
        let conf: Config = Config::new()
            .cert(cert)
            .cipher_suite(dtls::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256)
            .mtu(*mtu)
            .insecure_skip_verify();
        check_comms(conf);
    }
}

pub async fn run_client(
    client_config: Config,
    server_port: u16,
    server_ready: tokio::sync::oneshot::Receiver<()>,
    err_chan: tokio::sync::mpsc::Sender<std::io::Error>,
) {
    let mut sleep = sleep(Duration::from_secs(1));
    tokio::select! {
        _ = server_ready => {}  // Do nothing
        _ = &mut sleep => { err_chan.send(format!("server timeout after {:?}", sleep)); }
    }
    match dtls::dial("udp", "127.0.0.1", server_port, client_config).await {
        Ok(stream) => {
            stream.try_write(TEST_MESSAGE.as_bytes());
        }
        Err(e) => { err_chan.send(e); return; }
    }
}

pub async fn run_server(
    server_config: Config,
    out_chan: tokio::sync::mpsc::Sender<&str>,
    server_port: u16,
    server_ready: tokio::sync::oneshot::Sender<()>,
    err_chan: tokio::sync::mpsc::Sender<std::io::Error>,
) {
    match dtls::listen("udp", "127.0.0.1", server_port, server_config).await {
        Ok(listener) => {
            server_ready.send(());
            match listener.accept().await {
                Ok((stream, addr)) => {
                    loop {
                        match stream.readable().await {
                            Ok(_) => {}
                            Err(e) => { err_chan.send(e); return; }
                        }
                        let mut buf = [0; 8192];
                        let n = match stream.try_read(&mut buf) {
                            Ok(n) => n,
                            Err(e) => { err_chan.send(e); break; }
                        };
                        match std::str::from_utf8(&buf[0..n]) {
                            Ok(v) => {
                                out_chan.send(v);
                                loop {
                                    match stream.writable().await {
                                        Ok(_) => {}
                                        Err(e) => { err_chan.send(e); return; }
                                    }
                                    match stream.try_write(TEST_MESSAGE.as_bytes()) {
                                        Ok(n) => {
                                            break;
                                        }
                                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                            continue;
                                        }
                                        Err(e) => { err_chan.send(e); return; }
                                    }
                                }
                                break;
                            },
                            Err(e) => {
                                err_chan.send(
                                    std::io::Error::new(
                                        std::io::ErrorKind::Other,
                                        "failed to parse utf8"
                                    )
                                );
                                return;
                            }
                        }
                    }
                },
                Err(e) => { err_chan.send(e); return; }
            }
        }
        Err(e) => { err_chan.send(e); return; }
    }
}

// TODO
fn create_psk() -> (PSK, PSKIdHint) { ((), ()) }

fn check_comms(config: Config) {
    println!("Checking client server comunnication:\n{:?}\n", config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let mut sleep = sleep(TEST_TIME_LIMIT);
        let mut event_count: u8 = 0;  // break after two events have been emitted
        let mut client_seen = false;
        let mut server_seen = false;
        let conn = tokio::sync::RwLock::new(transport::Connection::new());
        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
        let (client_chan_tx, client_chan_rx) = tokio::sync::mpsc::channel(1);
        let (server_chan_tx, server_chan_rx) = tokio::sync::mpsc::channel(1);
        let (err_chan_tx, err_chan_rx) = tokio::sync::mpsc::channel(1);
        let server_port = random_port().await;
        let client_jh = tokio::spawn(run_client(
            config,
            server_port,
            server_ready_rx,
            err_chan_tx,
        ));
        let server_jh = tokio::spawn(run_server(
            config,
            server_chan_tx,
            server_port,
            server_ready_tx,
            err_chan_tx,
        ));
        tokio::pin!(sleep);
        tokio::pin!(client_chan_rx);
        tokio::pin!(server_chan_rx);
        loop {
            tokio::select! {
                reason = err_chan_rx => {
                    assert!(false, "Comm test failed to run: {}", reason);
                    break
                }
                _ = &mut sleep => {
                    assert!(false, "test timed out after {:?}", TEST_TIME_LIMIT);
                    break
                }
                msg = client_chan_rx => {
                    assert_eq!(msg, TEST_MESSAGE);
                    client_seen = true;
                    if client_seen && server_seen {
                        break
                    }
                }
                msg = server_chan_rx => {
                    assert_eq!(msg, TEST_MESSAGE);
                    server_seen = true;
                    if client_seen && server_seen {
                        break
                    }
                }
            }
        };
    });
}
