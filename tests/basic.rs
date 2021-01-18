
mod mocks;

use mocks::dtls::{
    self,
    Config,
    ConfigBuilder,
    CertificateBuilder,
    CipherSuite,
    MTU,
    TcpPort
};
use tokio::{self, net::TcpStream, time::{Duration, sleep}};
use tokio::{sync::{oneshot, Mutex}, io::{Error, ErrorKind}};
use std::sync::Arc;

const TEST_MESSAGE: &str = "Hello world";

#[test]
pub fn e2e_basic() {
    let cipher_suites: [CipherSuite; 2] = [
        CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
        CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
    ];
    for cs in cipher_suites.iter() {
        let cert = CertificateBuilder::default()
            .self_signed(true)
            .build()
            .unwrap();
        let conf = ConfigBuilder::default()
            .cipher_suites(vec!(*cs))
            .certificates(vec!(cert))
            .insecure_skip_verify(true)
            .build()
            .unwrap();
        check_comms(conf);
    }
}

#[test]
pub fn e2e_simple_psk() {
    let cipher_suites: [CipherSuite; 3] = [
        CipherSuite::TLS_PSK_WITH_AES_128_CCM,
        CipherSuite::TLS_PSK_WITH_AES_128_CCM_8,
        CipherSuite::TLS_PSK_WITH_AES_128_GCM_SHA256,
    ];
    for cs in cipher_suites.iter() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let conf = ConfigBuilder::default()
            .psk_callback(&|_| { vec!(0xAB, 0xC1, 0x23,) })
            .psk_id_hint(vec!(0x01, 0x02, 0x03, 0x04, 0x05))
            .cipher_suites(vec!(*cs))
            .build()
            .unwrap();
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
        let cert = CertificateBuilder::default()
            .self_signed(true)
            .host("localhost".to_string())
            .build()
            .unwrap();
        let conf = ConfigBuilder::default()
            .certificates(vec!(cert))
            .cipher_suites(vec!(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256))
            .insecure_skip_verify(true)
            .mtu(*mtu)
            .build()
            .unwrap();
        check_comms(conf);
    }
}

async fn random_port() -> TcpPort {
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
    config: &Config,
    server_port: TcpPort,
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
    let dial = dtls::dial(
        "udp".to_string(),
        "127.0.0.1".to_string(),
        server_port,
        *config
    );
    let stream = match dial.await {
        Ok(stream) => stream,
        Err(e) => return Err(e),
    };
    simple_read_write(stream).await
}

async fn run_server(
    config: &Config,
    server_port: TcpPort,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<String, Error>
{
    let listen = dtls::listen(
        "udp".to_string(),
        "127.0.0.1".to_string(),
        server_port,
        *config
    );
    let listener = match listen.await {
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

pub fn check_comms(conf: Config) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let port = random_port().await;
        let (server_start_tx, server_start_rx) = oneshot::channel();
        let server = tokio::spawn(run_server(&conf, port, server_start_tx));
        let client = tokio::spawn(run_client(&conf, port, server_start_rx));
        let mut client_complete = false;
        let mut server_complete = false;
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
