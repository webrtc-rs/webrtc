#[macro_use]
extern crate derive_builder;

#[allow(dead_code)]
pub mod dtls {

    use super::transport;
    use tokio::time::Duration;

    #[allow(non_camel_case_types)]
    #[derive(Debug, Clone, Copy)]
    pub enum CipherSuite {
        TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
        TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA,
        TLS_PSK_WITH_AES_128_CCM,
        TLS_PSK_WITH_AES_128_CCM_8,
        TLS_PSK_WITH_AES_128_GCM_SHA256,
        TLS_ECDHE_ECDSA_WITH_AES_128_CCM,
        TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8,
        TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
        TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA,
    }

    // TODO
    pub const REQUIRE_ANY_CLIENT_CERT: () = ();

    type FlightInterval = Duration;
    pub type MTU = u16;
    pub type TcpPort = u16;
    pub type PskCallback = &'static dyn Fn(Option<PskIdHint>) -> Result<Psk, String>;
    // TODO

    #[derive(Clone, Copy)]
    pub struct Psk { }

    impl std::fmt::Display for Psk {
        fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("unimplemented")
        }
    }

    #[derive(Clone, Copy)]
    pub struct PskIdHint { }

    impl std::fmt::Display for PskIdHint {
        fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("unimplemented")
        }
    }

    impl PskIdHint {
        pub fn len(&self) -> usize {
            panic!("unimplemented")
        }
    }

    const BACKOFF: Duration = Duration::from_millis(500);

    pub struct Client {
        conn: transport::Connection,
        config: Config,
    }

    impl Client {
        pub fn new(conn: transport::Connection, config: Config) -> Result<Self, String> {
            Ok( Client {
                conn,
                config,
            })
        }
        pub async fn start(&self) {
            println!("client started")
        }
        pub async fn next(&self) -> Event {
            println!("client polled");
            Event::Message { content: () }
        }
        pub fn get_connection(&self) -> &transport::Connection {
            &self.conn
        }
    }

    pub struct Server {
        conn: transport::Connection,
        config: Config,
    }

    impl Server {
        pub fn new(conn: transport::Connection, config: Config) -> Result<Self, String> {
            Ok( Server {
                conn,
                config,
            })
        }
        pub async fn start(&self) {
            println!("server started")
        }
        pub async fn next(&self) -> Event {
            println!("server polled");
            Event::Message { content: () }
        }
        pub fn get_connection(&self) -> &transport::Connection {
            &self.conn
        }
    }

    pub enum Event {
        Message { content: () },
        Error { reason: () },
    }

    pub struct Certificate {
        pub certificate: Vec<CertParts>,
        pub private_key: (),
    }

    // TODO
    struct CertParts { }

    #[derive(Builder, Clone)]
    pub struct CertConfig {
        host: String,
        self_signed: bool,
    }

    #[derive(Clone, Copy)]
    pub enum ClientAuthType {
        NoClientCert,
        RequireAnyClientCert,
    }

    #[derive(Builder, Clone, Copy)]
    pub struct Config {
        pub certificates: &'static Vec<Certificate>,
        pub cipher_suites: &'static Vec<CipherSuite>,
        pub insecure_skip_verify: bool,
        // sets the PSK used by the DTLS connection
        pub psk_callback: Option<PskCallback>,
        pub psk_id_hint: PskIdHint,
        // maximum tranmission unit in bytes
        pub mtu: MTU,
        // how often we send outbound handshake messages
        pub flight_interval: FlightInterval,
        pub client_auth_type: ClientAuthType,
    }

    pub async fn listen(
        _proto: String,
        addr: String,
        port: u16,
        _config: Config,
    ) -> Result<tokio::net::TcpListener, std::io::Error> {
        println!("mock dtls::listen on {}:{}", addr, port);
        tokio::net::TcpListener::bind(format!("{}:{}", addr, port)).await
    }

    pub async fn dial(
        _proto: String,
        addr: String,
        port: u16,
        _config: Config,
    ) -> Result<tokio::net::TcpStream, std::io::Error> {
        println!("mock dtls::dial on {}:{}", addr, port);
        tokio::net::TcpStream::connect(format!("{}:{}", addr, port)).await
    }

}

#[allow(dead_code)]
#[allow(unused_variables)]
pub mod transport {

    #[derive(Copy)]
    #[derive(Clone)]
    pub struct Connection { }

    impl Connection {
        pub fn new() -> Self { Connection { } }
        pub fn send(&self, message: &str) -> Result<u16, &str> { Ok(0) }
        pub fn recv(&self, buffer: &mut [u8; 8192]) -> Result<usize, &str> { Ok(0) }
    }

    #[derive(Copy)]
    #[derive(Clone)]
    pub struct Bridge { }
    
    impl Bridge {
        pub fn new() -> Self { Bridge { } }
        pub fn set_loss_chance(&self, loss_chance: u8) { }
        pub fn get_connection(&self) -> Connection { Connection { } }
    }
}
pub mod test_runner {

    use super::dtls::{Config, TcpPort};
    use tokio::{
        self,
        net::TcpStream,
        io::ErrorKind,
        sync::{oneshot, Mutex},
        time::{Duration, sleep},
    };
    use std::sync::{Arc};

    const TEST_MESSAGE: &str = "Hello world";

    /// Read and write to the stream
    /// Returns the UTF8 String received from stream
    pub async fn simple_read_write(stream: TcpStream) -> Result<String, String> {
        let ref stream = Arc::new(Mutex::new(stream));
        let reader_join_handle = tokio::spawn(read_from(Arc::clone(stream)));
        let writer_join_handle = tokio::spawn(write_to(Arc::clone(stream)));
        let msg = match reader_join_handle.await {
            Ok(r) => match r {
                Ok(s) => s,
                Err(e) => return Err(e.to_string())
            }
            Err(e) => return Err(e.to_string())
        };
        match writer_join_handle.await {
            Ok(_) => {},
            Err(e) => return Err(e.to_string())
        }
        Ok(msg)
    }

    async fn read_from(stream: Arc<Mutex<TcpStream>>) -> Result<String, String> {
        loop {
            let s = stream.lock().await;
            match s.readable().await {
                Ok(_) => {}
                Err(e) => {
                    return Err(e.to_string())
                }
            }
            let mut buf = [0_u8; 8192];
            match s.try_read(&mut buf) {
                Ok(n) => {
                    match std::str::from_utf8(&buf[0..n]) {
                        Ok(s) => return Ok(s.to_string()),
                        Err(e) => return Err(format!("Failed to parse utf8: {:?}", e)),
                    }
                },
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.to_string())
                }
            };
        }
    }
    
    async fn write_to(stream: Arc<Mutex<TcpStream>>) -> Result<(), String> {
        println!("writing to stream...");
        loop {
            let s = stream.lock().await;
            match s.writable().await {
                Ok(_) => {}
                Err(e) => return Err(e.to_string())
            }
            match s.try_write(TEST_MESSAGE.as_bytes()) {
                Ok(_) => {
                    break;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => return Err(e.to_string())
            }
        }
        println!("finished writing to stream");
        Ok(())
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
}
