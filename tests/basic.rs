
mod mocks;

use mocks::dtls::{self, Config, Cert, CertConfig, CipherSuite, MTU, PSK};
use tokio::{self, net::TcpStream};
use std::sync::Arc;
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

async fn read_from(
    stream: Arc<Mutex<TcpStream>>,
    out_buffer: Arc<Mutex<[u8; 8192]>>,   
) -> Result<(), std::io::Error>
{
    let mut buf = *out_buffer.lock().await;
    loop {
        let s = stream.lock().await;
        match s.readable().await {
            Ok(_) => {}
            Err(e) => {
                return Err(e)
            }
        }
        match s.try_read(&mut buf) {
            Ok(_) => {
                break;
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e)
            }
        };
    }
    Ok(())
}

async fn write_to(
    stream: Arc<Mutex<TcpStream>>,
) -> Result<(), std::io::Error>
{
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
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
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

async fn simple_read_write(
    stream: TcpStream,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
) -> Result<(), std::io::Error>
{
    let ref stream = Arc::new(Mutex::new(stream));
    let reader_join_handle = tokio::spawn(read_from(Arc::clone(stream), out_buffer));
    let writer_join_handle = tokio::spawn(write_to(Arc::clone(stream)));
    match reader_join_handle.await {
        Ok(_) => {},
        Err(e) => return Err(e.into()),
    }
    match writer_join_handle.await {
        Ok(_) => {},
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

async fn run_client(
    client_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    start_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), std::io::Error>
{
    let timeout = tokio::time::Duration::from_secs(1);
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    tokio::select! {
        _ = &mut sleep => { panic!("Client timed out waiting for server after {:?}", timeout) }
        _ = start_rx => {}  // Do nothing
    }
    let stream = match dtls::dial("udp".to_string(), "127.0.0.1".to_string(), server_port, client_config).await {
        Ok(stream) => stream,
        Err(e) => return Err(e),
    };
    simple_read_write(stream, out_buffer).await
}

async fn run_server(
    server_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), std::io::Error>
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
    simple_read_write(stream, out_buffer).await
}

fn check_comms(config: Config) {
    println!("Checking client server communication:\n{:?}\n", config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let mut client_complete = false;
        let mut server_complete = false;
        let server_port = random_port().await;
        let server_out_buffer = Arc::new(Mutex::new([0_u8; 8192]));
        let client_out_buffer = Arc::new(Mutex::new([0_u8; 8192]));
        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
        let client = tokio::spawn(run_client(
            config,
            server_port,
            Arc::clone(&client_out_buffer),
            server_ready_rx,
        ));
        let server = tokio::spawn(run_server(
            config,
            server_port,
            Arc::clone(&server_out_buffer),
            server_ready_tx,
        ));
        let timeout = tokio::time::Duration::from_secs(5);
        let sleep = tokio::time::sleep(timeout);
        tokio::pin!(sleep);
        tokio::pin!(client_out_buffer);
        tokio::pin!(server_out_buffer);
        tokio::pin!(client);
        tokio::pin!(server);
        loop {
            tokio::select! {
                _ = &mut sleep => { panic!("Test timed out after {:?}", timeout) }
                result = &mut client => {
                    match result {
                        Ok(_) => {
                            client_complete = true;
                            let buf = *client_out_buffer.lock().await;
                            let msg = std::str::from_utf8(&buf).unwrap();
                            println!("     got: {}", msg);
                            println!("expected: {}", TEST_MESSAGE);
                            assert!(msg == TEST_MESSAGE);
                        }
                        Err(e) => assert!(false, "client failed: {}", e)
                    }
                }
                result = &mut server => {
                    match result {
                        Ok(_) => {
                            server_complete = true;
                            let buf = *server_out_buffer.lock().await;
                            let msg = std::str::from_utf8(&buf).unwrap().to_string();
                            println!("     got: {}", msg);
                            println!("expected: {}", TEST_MESSAGE);
                            assert!(msg == TEST_MESSAGE);
                        }
                        Err(e) => assert!(false, "server failed: {}", e)
                    }
                }
            }
            if server_complete && client_complete {
                break
            }
        }
    });
}
