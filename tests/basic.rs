
mod mocks;

use mocks::dtls::{self, Config, Cert, CertConfig, CipherSuite, MTU, PSK};
use tokio::{self, net::TcpStream, time::{Duration, sleep}};
use std::{sync::Arc, io::{Error, ErrorKind}};
use tokio::sync::Mutex;

const TEST_MESSAGE: &str = "Hello world";

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

fn create_psk() -> (PSK,PSK) { ((),()) }

async fn random_port() -> u16 {
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

async fn read_from(stream: Arc<Mutex<TcpStream>>) -> Result<String, Error> {
    loop {
        let s = stream.lock().await;
        match s.readable().await {
            Ok(_) => {}
            Err(e) => {
                return Err(e)
            }
        }
        let mut buf = [0_u8; 8192];
        match s.try_read(&mut buf) {
            Ok(n) => {
                match std::str::from_utf8(&buf[0..n]) {
                    Ok(s) => return Ok(s.to_string()),
                    Err(e) => return Err(Error::new(ErrorKind::Other, format!("Failed to parse utf8: {:?}", e))),
                }
            },
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e)
            }
        };
    }
}

async fn write_to(stream: Arc<Mutex<TcpStream>>) -> Result<(), Error> {
    println!("writing to stream...");
    loop {
        let s = stream.lock().await;
        match s.writable().await {
            Ok(_) => {}
            Err(e) => {
                return Err(e)
            }
        }
        match s.try_write(TEST_MESSAGE.as_bytes()) {
            Ok(_) => {
                break;
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e)
            }
        }
    }
    println!("finished writing to stream");
    Ok(())
}

/// Read and write to the stream
/// Returns the UTF8 String received from stream
async fn simple_read_write(stream: TcpStream) -> Result<String, std::io::Error> {
    let ref stream = Arc::new(Mutex::new(stream));
    let reader_join_handle = tokio::spawn(read_from(Arc::clone(stream)));
    let writer_join_handle = tokio::spawn(write_to(Arc::clone(stream)));
    let msg = match reader_join_handle.await {
        Ok(r) => match r {
            Ok(s) => s,
            Err(e) => return Err(e.into())
        }
        Err(e) => return Err(e.into())
    };
    match writer_join_handle.await {
        Ok(_) => {},
        Err(e) => return Err(e.into())
    }
    Ok(msg)
}

async fn run_client(
    client_config: Config,
    server_port: u16,
    start_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<String, std::io::Error>
{
    let timeout = Duration::from_secs(1);
    let sleep = sleep(timeout);
    tokio::pin!(sleep);
    tokio::select! {
        _ = &mut sleep => { panic!("Client timed out waiting for server after {:?}", timeout) }
        _ = start_rx => {}  // Do nothing
    }
    let stream = match dtls::dial("udp".to_string(), "127.0.0.1".to_string(), server_port, client_config).await {
        Ok(stream) => stream,
        Err(e) => return Err(e),
    };
    simple_read_write(stream).await
}

async fn run_server(
    server_config: Config,
    server_port: u16,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<String, Error>
{
    let listener = match dtls::listen("udp".to_string(), "127.0.0.1".to_string(), server_port, server_config).await {
        Ok(listener) => listener,
        Err(e) => panic!(e),
    };
    match ready_tx.send(()) {
        Ok(_) => {},
        Err(e) => panic!(e),
    }
    let (stream, _addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => panic!(e),
    };
    simple_read_write(stream).await
}

fn check_comms(config: Config) {
    println!("Checking client server communication:\n{:?}\n", config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let mut client_complete = false;
        let mut server_complete = false;
        let server_port = random_port().await;
        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
        let client = tokio::spawn(run_client(
            config,
            server_port,
            server_ready_rx,
        ));
        let server = tokio::spawn(run_server(
            config,
            server_port,
            server_ready_tx,
        ));
        let timeout = Duration::from_secs(5);
        let sleep = sleep(timeout);
        tokio::pin!(sleep);
        tokio::pin!(client);
        tokio::pin!(server);
        while !(server_complete && client_complete) {
            tokio::select! {
                _ = &mut sleep => {
                    panic!("Test timed out after {:?}", timeout)
                }
                join_result = &mut client => {
                    match join_result {
                        Ok(msg) => match msg {
                            Ok(s) => {
                                client_complete = true;
                                println!("client got: {}", s);
                                println!("  expected: {}", TEST_MESSAGE);
                                assert!(s == TEST_MESSAGE);
                            }
                            Err(e) => assert!(false, "client failed: {}", e)
                        }
                        Err(e) => assert!(false, "failed to join with client: {}", e)
                    }
                }
                join_result = &mut server => {
                    match join_result {
                        Ok(msg) => match msg {
                            Ok(s) => {
                                server_complete = true;
                                println!("server got: {}", s);
                                println!("  expected: {}", TEST_MESSAGE);
                                assert!(s == TEST_MESSAGE);
                            }
                            Err(e) => assert!(false, "server failed: {}", e)
                        }
                        Err(e) => assert!(false, "failed to join with server: {}", e)
                    }
                }
            }
        }
    });
}
