
mod mocks;

use mocks::dtls::{self, Config, Cert, CertConfig, CipherSuite, MTU, PSK};
use tokio::{self, net::TcpStream, task::JoinHandle};
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
) -> Result<usize, std::io::Error>
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
            Ok(n) => {
                return Ok(n);
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e)
            }
        };
    }
}

async fn write_to(
    stream: Arc<Mutex<TcpStream>>,
) -> Result<usize, std::io::Error>
{
    loop {
        let s = stream.lock().await;
        match s.writable().await {
            Ok(_) => {}
            Err(e) => {
                return Err(e)
            }
        }
        match s.try_write(TEST_MESSAGE.as_bytes()) {
            Ok(n) => {
                return Ok(n);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e)
            }
        }
    }
}

// Spawn and await tasks to read from and write to the given stream
async fn simple_read_write(
    stream: tokio::net::TcpStream,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
) -> Result<(
        JoinHandle<Result<usize, std::io::Error>>, JoinHandle<Result<usize, std::io::Error>>
    ), std::io::Error>
{
    let ref stream = Arc::new(Mutex::new(stream));
    let rs = Arc::clone(stream);
    let ws = Arc::clone(stream);
    let reader_join_handle = tokio::spawn(read_from(rs, out_buffer));
    let writer_join_handle = tokio::spawn(write_to(ws));
    Ok((reader_join_handle, writer_join_handle))
}

async fn run_client(
    client_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    start_rx: tokio::sync::oneshot::Receiver<()>,
) -> (JoinHandle<Result<usize, std::io::Error>>, JoinHandle<Result<usize, std::io::Error>>)
{
    let timeout = tokio::time::Duration::from_secs(1);
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    tokio::select! {
        _ = &mut sleep => { panic!("Client timed out wait for server after {:?}", timeout) }
        _ = start_rx => {}  // Do nothing
    }
    let stream = match dtls::dial("udp", "127.0.0.1", server_port, client_config).await {
        Ok(stream) => stream,
        Err(e) => panic!(e),
    };
    match simple_read_write(stream, out_buffer).await {
        Ok((r,w)) => (r,w),
        Err(e) => panic!(e),
    }
}

async fn run_server(
    server_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> (JoinHandle<Result<usize, std::io::Error>>, JoinHandle<Result<usize, std::io::Error>>)
{
    let listener = match dtls::listen("udp", "127.0.0.1", server_port, server_config).await {
        Ok(listener) => listener,
        Err(e) => panic!(e),
    };
    let (stream, _addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => panic!(e),
    };
    match ready_tx.send(()) {
        Ok(_) => {},
        Err(e) => panic!(e),
    }
    match simple_read_write(stream, out_buffer).await {
        Ok((r,w)) => (r,w),
        Err(e) => panic!(e),
    }
}

fn check_comms(config: Config) {
    println!("Checking client server communication:\n{:?}\n", config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let mut server_writer_seen = false;
        let mut server_reader_seen = false;
        let mut client_writer_seen = false;
        let mut client_reader_seen = false;
        let server_port = random_port().await;
        let server_out_buffer = Arc::new(Mutex::new([0_u8; 8192]));
        let client_out_buffer = Arc::new(Mutex::new([0_u8; 8192]));
        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
        let client = run_client(
            config,
            server_port,
            Arc::clone(&client_out_buffer),
            server_ready_rx,
        );
        let (client_reader_jh, client_writer_jh) = client.await;
        let server = run_server(
            config,
            server_port,
            Arc::clone(&server_out_buffer),
            server_ready_tx,
        );
        let (server_reader_jh, server_writer_jh) = server.await;

        let timeout = tokio::time::Duration::from_secs(5);
        let sleep = tokio::time::sleep(timeout);
        tokio::pin!(sleep);
        tokio::pin!(client_out_buffer);
        tokio::pin!(server_out_buffer);
        tokio::pin!(client_reader_jh);
        tokio::pin!(client_writer_jh);
        tokio::pin!(server_writer_jh);
        tokio::pin!(server_reader_jh);
        loop {
            tokio::select! {
                _ = &mut sleep => { panic!("Test timed out after {:?}", timeout) }
                result = &mut client_writer_jh => {
                    match result {
                        Ok(_) => client_writer_seen = true,
                        Err(e) => assert!(false, "client writer failed: {}", e)
                    }
                }
                result = &mut client_reader_jh => {
                    match result {
                        Ok(_) => {
                            client_reader_seen = true;
                            let buf = *client_out_buffer.lock().await;
                            let msg = std::str::from_utf8(&buf).unwrap();
                            assert!(msg == TEST_MESSAGE);
                        }
                        Err(e) => assert!(false, "client reader failed: {}", e)
                    }
                }
                result = &mut server_writer_jh => {
                    match result {
                        Ok(_) => server_writer_seen = true,
                        Err(e) => assert!(false, "server writer failed: {}", e)
                    }
                }
                result = &mut server_reader_jh => {
                    match result {
                        Ok(_) => {
                            server_reader_seen = true;
                            let buf = *server_out_buffer.lock().await;
                            let msg = std::str::from_utf8(&buf).unwrap();
                            assert!(msg == TEST_MESSAGE);
                        }
                        Err(e) => assert!(false, "server reader failed: {}", e)
                    }
                }
            }
            if server_reader_seen && client_reader_seen
               && server_writer_seen && client_writer_seen {
                break
            }
        }
    });
}
