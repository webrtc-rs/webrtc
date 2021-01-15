
mod mocks;

use mocks::dtls::{self, Config, Cert, CertConfig, CipherSuite, PSK, PSKIdHint, MTU};
use mocks::transport;
use tokio;
use tokio::time::{sleep, Duration};
use std::sync::{Arc, Mutex};

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

// Spawn and await tasks to read from and write to the given stream
pub async fn simple_read_write(
    stream: &'static Arc<Mutex<tokio::net::TcpStream>>,
    out_buffer: &'static Arc<Mutex<[u8; 8192]>>,
) -> (Result<(), std::io::Error>, Result<(), std::io::Error>) {
    // Read from stream into out buffer
    let read_jh = tokio::spawn( async move {
        let mx_buf = Arc::clone(out_buffer);
        let mut buf = *mx_buf.lock().unwrap();
        loop {
            let mx_stream = Arc::clone(stream);
            let stream = mx_stream.lock().unwrap();
            match stream.readable().await {
                Ok(_) => {}
                Err(e) => {
                    return Err(e)
                }
            }
            match stream.try_read(&mut buf) {
                Ok(n) => {
                    return Ok(());
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e)
                }
            };
        }
    });
    // Write TEST_MESSAGE to socket
    let write_jh = tokio::spawn( async move {
        loop {
            let mx_stream = Arc::clone(stream);
            let stream = mx_stream.lock().unwrap();
            match stream.writable().await {
                Ok(_) => {}
                Err(e) => {
                    return Err(e)
                }
            }
            match stream.try_write(TEST_MESSAGE.as_bytes()) {
                Ok(n) => {
                    return Ok(());
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e)
                }
            }
        }
    });
    let read_result = match read_jh.await {
        Ok(r) => r,
        Err(e) => Err(e),
    };
    let write_result = match write_jh.await {
        Ok(r) => r,
        Err(e) => Err(e),
    };
    (read_result, write_result)
}

pub async fn run_client(
    client_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    start_rx: tokio::sync::oneshot::Receiver<()>,
) -> (Result<(), std::io::Error>, Result<(), std::io::Error>) {
    let mut sleep = sleep(Duration::from_secs(1));
    tokio::select! {
        _ = start_rx => {}  // Do nothing
        _ = &mut sleep => { return Err(std::io::Error::new(std::io::Error::Other, "timed out")) }
    }
    let stream = match dtls::dial("udp", "127.0.0.1", server_port, client_config).await {
        Ok(stream) => stream,
        Err(e) => panic!(e),
    };
    match simple_read_write(stream, out_buffer).await {
        Ok(v) => return v,
        Err(e) => panic!(e),
    }
}

pub async fn run_server(
    server_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    start_rx: tokio::sync::oneshot::Receiver<()>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) {
    let listener = match dtls::listen("udp", "127.0.0.1", server_port, server_config).await {
        Ok(listener) => listener,
        Err(e) => panic!(e),
    };
    let (stream, addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => panic!(e),
    };
    ready_tx.send(());
    match simple_read_write(stream, out_buffer).await {
        Ok(v) => return v,
        Err(e) => panic!(e),
    }
}

// TODO
fn create_psk() -> (PSK, PSKIdHint) { ((), ()) }

fn check_comms(config: Config) {
    println!("Checking client server communication:\n{:?}\n", config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let mut sleep = sleep(TEST_TIME_LIMIT);
        let mut event_count: u8 = 0;  // break after two events have been emitted
        let mut client_seen = false;
        let mut server_seen = false;
        let conn = tokio::sync::RwLock::new(transport::Connection::new());
        let server_port = random_port().await;
        let mut server_out_buffer = [0_u8; 8192];
        let (server_start_tx, server_start_rx) = tokio::sync::oneshot::channel();
        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
        let mut client_out_buffer = [0_u8; 8192];
        let (client_start_tx, client_start_rx) = tokio::sync::oneshot::channel();
        let (client_ready_tx, client_ready_rx) = tokio::sync::oneshot::channel();
        let client_jh = tokio::spawn(run_client(
            config,
            server_port,
            &mut client_out_buffer,
            server_ready_rx,
        ));
        let server_jh = tokio::spawn(run_server(
            config,
            server_port,
            &mut server_out_buffer,
            server_start_rx,
            server_ready_tx,
        ));
        tokio::pin!(sleep);
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
