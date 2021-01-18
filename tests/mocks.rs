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

    #[derive(Builder, Clone)]
    pub struct Certificate {
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
