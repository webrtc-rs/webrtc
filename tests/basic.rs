
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
    let listener = match dtls::listen("udp", "127.0.0.1", server_port, server_config).await {
        Ok(listener) => listener,
        Err(e) => {
            err_chan.send(e);
            return;
        }
    };
    server_ready.send(());
    let (stream, addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => {
            err_chan.send(e);
            return;
        }
    };
    // TODO make sure addr is the expected client
    let mut buf = vec![0_u8; 8192];
    loop {
        match stream.readable().await {
            Ok(_) => {}
            Err(e) => {
                err_chan.send(e);
                return;
            }
        }
        match stream.try_read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                break;
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                err_chan.send(e);
                return;
            }
        };
    }
    let s = match std::str::from_utf8(&buf) {
        Ok(v) => v,
        Err(e) => {
            err_chan.send(
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "failed to parse utf8"
                )
            );
            return;
        }
    };
    out_chan.send(s);
    loop {
        match stream.writable().await {
            Ok(_) => { }
            Err(e) => {
                err_chan.send(e);
                return;
            }
        }
        match stream.try_write(TEST_MESSAGE.as_bytes()) {
            Ok(n) => {
                break;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                err_chan.send(e);
                break;
            }
        }
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
