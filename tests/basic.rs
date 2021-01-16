
mod mocks;

use mocks::dtls::{self, Config};
use mocks::transport;
use tokio;
use tokio::time::{sleep, Duration};
use std::sync::Arc;
use tokio::sync::Mutex;

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
pub async fn simple_read_write<'a>(
    stream: tokio::net::TcpStream,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    writer_join_handle: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
    reader_join_handle: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
) -> Result<(), std::io::Error>
{
    let ref arc_mx_stream = Arc::new(Mutex::new(stream));
    let read = Arc::clone(arc_mx_stream);
    let write = Arc::clone(arc_mx_stream);
    // Read from stream into out buffer
    *reader_join_handle = tokio::spawn( async move {
        let mut buf = *out_buffer.lock().await;
        loop {
            let s = read.lock().await;
            match s.readable().await {
                Ok(_) => {}
                Err(e) => {
                    return Err(e)
                }
            }
            match s.try_read(&mut buf) {
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
    *writer_join_handle = tokio::spawn( async move {
        loop {
            let s = write.lock().await;
            match s.writable().await {
                Ok(_) => {}
                Err(e) => {
                    return Err(e)
                }
            }
            match s.try_write(TEST_MESSAGE.as_bytes()) {
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
    Ok(())
}

pub async fn run_client(
    client_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    start_rx: tokio::sync::oneshot::Receiver<()>,
    writer_join_handle: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
    reader_join_handle: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
) -> Result<(), std::io::Error> {
    let timeout = Duration::from_secs(1);
    let mut sleep = sleep(timeout);
    tokio::select! {
        _ = start_rx => {}  // Do nothing
        _ = &mut sleep => { panic!("client timed out waiting for server after {:?}", timeout) }
    }
    let stream = match dtls::dial("udp", "127.0.0.1", server_port, client_config).await {
        Ok(stream) => stream,
        Err(e) => panic!(e),
    };
    let ref mut read_result: std::io::Error;
    let ref mut write_result: std::io::Error;
    match simple_read_write(stream, out_buffer, writer_join_handle, reader_join_handle).await {
        Ok(_) => return Ok(()),
        Err(e) => return Err(e),
    }
}

pub async fn run_server(
    server_config: Config,
    server_port: u16,
    out_buffer: Arc<Mutex<[u8; 8192]>>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
    writer_join_handle: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
    reader_join_handle: &mut tokio::task::JoinHandle<Result<(), std::io::Error>>,
) -> Result<(), std::io::Error> {
    let listener = match dtls::listen("udp", "127.0.0.1", server_port, server_config).await {
        Ok(listener) => listener,
        Err(e) => panic!(e),
    };
    let (stream, addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => panic!(e),
    };
    ready_tx.send(());
    match simple_read_write(stream, out_buffer, writer_join_handle, reader_join_handle).await {
        Ok(_) => return Ok(()),
        Err(e) => return Err(e),
    }
}

fn check_comms(config: Config) {
    println!("Checking client server communication:\n{:?}\n", config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on( async move {
        let mut event_count: u8 = 0;  // break after two events have been emitted
        let mut server_writer_seen = false;
        let mut server_reader_seen = false;
        let mut client_writer_seen = false;
        let mut client_reader_seen = false;
        let conn = tokio::sync::RwLock::new(transport::Connection::new());
        let server_port = random_port().await;
        let server_out_buffer = Arc::new(Mutex::new([0_u8; 8192]));
        let client_out_buffer = Arc::new(Mutex::new([0_u8; 8192]));
        let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
        let ref mut client_writer_jh: tokio::task::JoinHandle<Result<(), std::io::Error>>;
        let ref mut client_reader_jh: tokio::task::JoinHandle<Result<(), std::io::Error>>;
        let ref mut server_writer_jh: tokio::task::JoinHandle<Result<(), std::io::Error>>;
        let ref mut server_reader_jh: tokio::task::JoinHandle<Result<(), std::io::Error>>;
        let client_jh = tokio::spawn(run_client(
            config,
            server_port,
            client_out_buffer,
            server_ready_rx,
            client_writer_jh,
            client_reader_jh,
        ));
        let server_jh = tokio::spawn(run_server(
            config,
            server_port,
            server_out_buffer,
            server_ready_tx,
            server_writer_jh,
            server_reader_jh,
        ));

        let mut sleep = sleep(TEST_TIME_LIMIT);
        loop {
            tokio::select! {
                _ = &mut sleep => {
                    assert!(false, "test timed out after {:?}", TEST_TIME_LIMIT);
                    break
                }
                result = client_writer_jh => {
                    match result {
                        Ok(_) => client_writer_seen = true,
                        Err(e) => assert!(false, "client writer failed: {}", e)
                    }
                }
                result = client_reader_jh => {
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
                result = server_writer_jh => {
                    match result {
                        Ok(_) => server_writer_seen = true,
                        Err(e) => assert!(false, "server writer failed: {}", e)
                    }
                }
                result = server_reader_jh => {
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
        };
        assert!(server_reader_seen);
        assert!(server_writer_seen);
        assert!(client_writer_seen);
        assert!(client_reader_seen);
    });
}
