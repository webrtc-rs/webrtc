#[macro_use]
extern crate derive_builder;

pub mod dtls {

    use super::{transport, protocol::Protocol};
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
    pub type TcpHost = &str;

    // TODO check these types
    pub type PskCallback = &'static dyn Fn(Option<PskIdHint>) -> Result<Psk, String>;

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
        pub private_key: CertPrivateKey,
    }

    impl Certificate {
        pub fn new(config: CertConfig) -> Certificate {
            Certificate {
                certificate: vec!(),
                private_key: CertPrivateKey {}
            }
        }
    }

    // TODO
    pub struct CertParts      {}
    pub struct CertPrivateKey {}

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
        _proto: Protocol,
        addr: TcpHost,
        port: TcpPort,
        _config: Config,
    ) -> Result<tokio::net::TcpListener, std::io::Error> {
        println!("mock dtls::listen on {}:{}", addr, port);
        tokio::net::TcpListener::bind(format!("{}:{}", addr, port)).await
    }

    pub async fn dial(
        _proto: Protocol,
        addr: TcpHost,
        port: TcpPort,
        _config: Config,
    ) -> Result<tokio::net::TcpStream, std::io::Error> {
        println!("mock dtls::dial on {}:{}", addr, port);
        tokio::net::TcpStream::connect(format!("{}:{}", addr, port)).await
    }

}

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

pub mod pem {
    use super::dtls::CertParts;
    use std::fs::File;
    pub struct Block {}
    impl Block {
        pub fn new(kind: String, der_bytes: CertParts) -> Self { Block {} }
    }
    pub fn encode(out_file: File, block: Block) -> Result<(), String> { Ok(()) }
}

pub mod x509 {
    use super::dtls::{CertParts, CertPrivateKey};
    pub fn marshal_pkcs8_private_key(pk: CertPrivateKey, )
    -> Result<CertParts, String>
    {
        Ok(CertParts {} )
    }
}

pub mod protocol {
    pub enum Protocol {
        Udp,
    }
}

pub mod openssl {
    use super::{
        pem,
        x509,
        dtls::{Config, CipherSuite, Certificate, TcpPort},
    };
    use tokio::{
        sync::oneshot,
        process::Command,
    };
    use std::{
        env,
        fs::{self, OpenOptions},
    };
    
    /// Create server cert and key files in a temp dir
    /// Returns a channel to delete the temp dir
    pub async fn create_server_openssl_files(config: Config)
    -> Result<Option<oneshot::Sender<()>>, String>
    {
        // Determine server openssl args
        let args = vec!(
            "s_server",
            "-dtls1_2",
            "-quiet",
            "-verify_quiet",
            "-verify_return_error",
        );
        let ciphers = cipher_openssl(*config.cipher_suites);
        if ciphers != "" {
            args.push(format!("-cipher={}", ciphers))
        }
        match config.psk_callback {
            Some(cb) => match cb(None) {
                Ok(psk) => args.push(format!("-psk={}", psk)),
                Err(e) => return Err(e),
            }
            None => {}
        }
        if config.psk_id_hint.len() > 0 {
            args.push(format!("-psk_hint={}", config.psk_id_hint))
        }
        let mut cleanup: Option<oneshot::Sender<()>> = None;
        if config.certificates.len() > 0 {
            let (cert_pem, key_pem, release_certs) = match write_temp_pem(config.certificates[0]) {
                Ok((c,k,f)) => (c,k,f),
                Err(e) => return Err(e.into()),
            };
            cleanup = Some(release_certs);
            args.push(format!("-cert={}", cert_pem));
            args.push(format!("-key={}", key_pem));
        } else {
            args.push(format!("-nocert"));
        }
    
        // Run server openssl command
        let output = match Command::new("openssl").args(&args).output().await {
            Ok(o) => o,
            Err(e) => return Err(e.to_string()),
        };
        println!("{:?}", output);
        return Ok(cleanup);
    }
    
    pub async fn create_client_openssl_files(config: Config, port: TcpPort)
    -> Result<Option<oneshot::Sender<()>>, String>
    {
        // Determine client openssl args
        let args = vec!(
            "s_client",
            "-dtls1_2",
            "-quiet",
            "-verify_quiet",
            "-verify_return_error",
            "-servername=localhost",
            format!("-connect=127.0.0.1:{}", port),
        );
        let cipher_suites = cipher_openssl(*config.cipher_suites);
        if cipher_suites.len() > 0 {
            args.push(format!("-cipher={}", cipher_suites))
        }
        if config.psk_id_hint.len() > 0 {
            args.push(format!("-psk_hint={}", config.psk_id_hint))
        }
        let mut cleanup: Option<oneshot::Sender<()>> = None;
        if config.certificates.len() > 0 {
            // TODO drop the temp file
            let (cert_pem, key_pem, release_certs) = match write_temp_pem(config.certificates[0]) {
                Ok((c,k,f)) => (c,k,f),
                Err(e) => return Err(e.to_string()),
            };
            cleanup = Some(release_certs);
            args.push(format!("-cert={}", cert_pem));
            args.push(format!("-key={}", key_pem));
        } else {
            args.push(format!("-nocert"));
        }
    
        // Run client openssl command
        let output = match Command::new("openssl").args(&args).output().await {
            Ok(o) => o,
            Err(e) => return Err(e.to_string()),
        };
        println!("{:?}", output);
        return Ok(cleanup);
    }
    
    pub fn cipher_openssl(cipher_suites: Vec<CipherSuite>) -> String {
        cipher_suites.iter().map( |cs| {
            (match cs {
                CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CCM        => "ECDHE-ECDSA-AES128-CCM",
                CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_CCM_8      => "ECDHE-ECDSA-AES128-CCM8",
                CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => "ECDHE-ECDSA-AES128-GCM-SHA256",
                CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256   => "ECDHE-RSA-AES128-GCM-SHA256",
                CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA    => "ECDHE-ECDSA-AES256-SHA",
                CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA      => "ECDHE-RSA-AES128-SHA",
                CipherSuite::TLS_PSK_WITH_AES_128_CCM                => "PSK-AES128-CCM",
                CipherSuite::TLS_PSK_WITH_AES_128_CCM_8              => "PSK-AES128-CCM8",
                CipherSuite::TLS_PSK_WITH_AES_128_GCM_SHA256         => "PSK-AES128-GCM-SHA256",
            }).to_string()
        }).fold("".to_string(), |acc, x| format!("{},{}", acc, x))
    }
    
    pub fn write_temp_pem(cert: Certificate)
    -> Result<(String, String, oneshot::Sender<()>), String>
    {
        let mut dir = env::temp_dir();
        dir.push("dtls-webrtc-rs-test");
    
        let der_bytes = cert.certificate[0];
        let cert_path = dir.clone();
        cert_path.push("cert.pem");
        let cert_out = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(cert_path)
            .unwrap();
        match pem::encode(cert_out, pem::Block::new("CERTIFICATE".to_string(), der_bytes)) {
            Ok(_) => {},
            Err(e) => return Err(e.to_string())
        }
        
        let key_path = dir.clone();
        key_path.push("key.pem");
        let key_out = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(key_path)
            .unwrap();
        let priv_key = cert.private_key;
        let priv_bytes = match x509::marshal_pkcs8_private_key(priv_key) {
            Ok(b) => b,
            Err(e) => return Err(e.to_string())
        };
        match pem::encode(key_out, pem::Block::new("PRIVATE KEY".to_string(), priv_bytes)) {
            Ok(_) => {},
            Err(e) => return Err(e.to_string())
        }
        
        let (tx, rx) = oneshot::channel();
        let release_certs = tokio::spawn( async move {
            rx.await;
            fs::remove_dir_all(dir);
        });
        Ok((
            cert_path
                .into_os_string()
                .into_string()
                .unwrap(),
            key_path
                .into_os_string()
                .into_string()
                .unwrap(),
            tx
        ))
    }
}

pub mod test_runner {
    use super::dtls::{Config, TcpHost, TcpPort};
    use tokio::{
        self,
        net::TcpStream,
        io::ErrorKind,
        sync::{oneshot, Mutex},
        time::{Duration, sleep},
    };
    use std::sync::Arc;

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

    pub async fn random_port() -> TcpPort {
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

    async fn run_client(
        config: Config,
        start_rx: tokio::sync::oneshot::Receiver<(TcpHost, TcpPort)>,
    ) -> Result<String, String>
    {
        // Wait for server to tell us where it's listening
        let timeout = Duration::from_secs(1);
        let sleep = sleep(timeout);
        tokio::pin!(sleep);
        let (host, port) = tokio::select! {
            _ = &mut sleep => return Err(format!("Client timed out waiting for server after {:?}", timeout))
            r = start_rx => {
                match r {
                    Ok((host,port)) => (host, port),
                    Err(e) => return Err("failed to receive server start signal".to_string()),
                }
            }
        };

        // Create client openssl files
        let cleanup = match create_client_openssl(config, port).await {
            Ok(c) => c,
            Err(e) => return Err(e.to_string())
        };
        
        // Dial the server
        let dial = dtls::dial(Protocol::Udp, host, port, config);
        let result = match dial.await {
            Ok(stream) => simple_read_write(stream).await,
            Err(e) => return Err(e.to_string()),
        };

        // Cleanup
        match cleanup {
            Some(x) => { x.send(()); },
            None => {},
        }
        return result
    }

    async fn run_server(
        config: Config,
        ready_tx: oneshot::Sender<(TcpHost, TcpPort)>,
    ) -> Result<String, String>
    {
        let host = "127.0.0.1";
        let port = random_port().await;
        // Create openssl server files
        let cleanup = match create_server_openssl(config).await {
            Ok(c) => c,
            Err(e) => return Err(e.to_string())
        };
        // Start lisening
        let listen = dtls::listen(Protocol::Udp, host, port, config);
        let listener = match listen.await {
            Ok(listener) => listener,
            Err(e) => panic!(e),
        };
        // Notify client
        match ready_tx.send((host, port)) {
            Ok(_) => {},
            Err(e) => panic!(e),
        }
        let result = match listener.accept().await {
            Ok((stream, addr)) => {
                // TODO check addr
                simple_read_write(stream).await
            },
            Err(e) => panic!(e),
        };
        match cleanup {
            Some(x) => { x.send(()); },
            None => {},
        }
        return result
    }

    pub fn check_comms(client_config: Config, server_config: Config)
    {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on( async move {
            let port = random_port().await;
            let (server_start_tx, server_start_rx) = oneshot::channel();
            let server = tokio::spawn( async {
                run_server(server_config, server_start_tx).await
            });
            let client = tokio::spawn( async {
                run_client(client_config, server_start_rx).await
            });
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
