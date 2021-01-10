
// mock types
#[allow(dead_code)]
mod dtls {

    use std::sync::{Arc, Mutex};
    use tokio::time::{sleep, Duration};

    pub type CipherSuite = u16;
    pub const TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256: CipherSuite = 0;
    pub const TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA: CipherSuite    = 0;
    pub const TLS_PSK_WITH_AES_128_CCM: CipherSuite                = 0;
    pub const TLS_PSK_WITH_AES_128_CCM_8: CipherSuite              = 0;
    pub const TLS_PSK_WITH_AES_128_GCM_SHA256: CipherSuite         = 0;

    pub type MTU = u16;
    // TODO
    pub type PSK = ();
    pub type PSKIdHint = ();

    const BACKOFF: Duration = Duration::from_millis(500);

    #[derive(Debug)]
    pub struct Client {
        config: Config,
        num_events_emitted: Arc<Mutex<u8>>,
    }

    impl Client {
        pub fn new(config: Config) -> Self {
            Client {
                config,
                num_events_emitted: Arc::new(Mutex::new(0)),
            }
        }
        pub async fn start(&self) {
            println!("client started")
        }
        pub async fn next(&self) -> Event {
            println!("client polled");
            let data = Arc::clone(&self.num_events_emitted);
            let mut n = data.lock().unwrap();
            if *n > 0 {
                println!("client already polled {} times, waiting {:?}", *n, BACKOFF);
                sleep(BACKOFF).await;
            }
            *n += 1;
            Event::Message { content: () }
        }
    }

    #[derive(Debug)]
    pub struct Server {
        config: Config,
        num_events_emitted: Arc<Mutex<u8>>,
    }

    impl Server {
        pub fn new(config: Config) -> Self {
            Server {
                config,
                num_events_emitted: Arc::new(Mutex::new(0)),
            }
        }
        pub async fn start(&self) {
            println!("server started")
        }
        pub async fn next(&self) -> Event {
            println!("server polled");
            let data = Arc::clone(&self.num_events_emitted);
            let mut n = data.lock().unwrap();
            if *n > 0 {
                println!("server already polled {} times, waiting {:?}", *n, BACKOFF);
                sleep(BACKOFF).await;
            }
            *n += 1;
            Event::Message { content: () }
        }
    }

    #[derive(Debug)]
    pub enum Event {
        Message { content: () },
        Error { reason: () },
    }

    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct Cert { host: &'static str }

    impl Cert {
        pub fn new(host: &'static str) -> Self { Cert { host } }
    }

    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct Config {
        cipher_suite: Option<CipherSuite>,
        cert: Option<Cert>,
        insecure_skip_verify: bool,
        psk: Option<PSK>,
        psk_id_hint: Option<PSKIdHint>,
        mtu: Option<MTU>,
    }

    // TODO: there is almost definitely an existing macro for this...
    impl Config {
        pub fn new() -> Self {
            Config {
                cipher_suite: None,
                cert: None,
                insecure_skip_verify: false,
                psk: None,
                psk_id_hint: None,
                mtu: None,
            }
        }

        pub fn cert(&self, cert: Cert) -> Self {
             Config {
                cipher_suite: self.cipher_suite,
                cert: Some(cert),
                insecure_skip_verify: self.insecure_skip_verify,
                psk: self.psk,
                psk_id_hint: self.psk_id_hint,
                mtu: self.mtu,
            }
        }

        pub fn cipher_suite(&self, cipher_suite: CipherSuite) -> Self {
            Config {
                cipher_suite: Some(cipher_suite),
                cert: self.cert,
                insecure_skip_verify: self.insecure_skip_verify,
                psk: self.psk,
                psk_id_hint: self.psk_id_hint,
                mtu: self.mtu,
            }
        }

        pub fn insecure_skip_verify(&self) -> Self {
            Config {
                cipher_suite: self.cipher_suite,
                cert: self.cert,
                insecure_skip_verify: true,
                psk: self.psk,
                psk_id_hint: self.psk_id_hint,
                mtu: self.mtu,
            }
        }

        pub fn psk(&self, psk: PSK, psk_id_hint: PSKIdHint) -> Self {
            Config {
                cipher_suite: self.cipher_suite,
                cert: self.cert,
                insecure_skip_verify: self.insecure_skip_verify,
                psk: Some(psk),
                psk_id_hint: Some(psk_id_hint),
                mtu: self.mtu,
            }
        }

        pub fn mtu(&self, mtu: MTU) -> Self {
            Config {
                cipher_suite: self.cipher_suite,
                cert: self.cert,
                insecure_skip_verify: self.insecure_skip_verify,
                psk: self.psk,
                psk_id_hint: self.psk_id_hint,
                mtu: Some(mtu),
            }
        }
    }

}

#[cfg(test)]
mod tests {

    use crate::dtls;
    use crate::dtls::{Client, Server, Event, Cert, Config, CipherSuite, PSK, PSKIdHint, MTU};
    use tokio;
    use tokio::time::{sleep, Duration};

    const TEST_MESSAGE: () = ();
    
    #[test]
    pub fn e2e_basic() {
        let cipher_suites: [u16; 2] = [
            dtls::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
        ];
        for cipher in cipher_suites.iter() {
            let cert = create_self_signed_cert("");
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
            let cert = create_self_signed_cert("localhost");
            let conf: Config = Config::new()
                .cert(cert)
                .cipher_suite(dtls::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256)
                .mtu(*mtu)
                .insecure_skip_verify();
            check_comms(conf);
        }
    }

    fn create_self_signed_cert(host: &'static str) -> Cert {
        Cert::new(host)
    }

    fn create_psk() -> (PSK, PSKIdHint) { ((), ()) }

    fn check_comms(config: Config) {
        println!("Checking client server comunnication:\n{:?}\n", config);
        let timeout_duration =  Duration::from_millis(500);
        let client = Client::new(config);
        let server = Server::new(config);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on( async move {
            let sleep = sleep(timeout_duration);
            tokio::pin!(sleep);
            let mut event_count: u8 = 0;  // break after two events have been emitted
            let mut client_seen = false;
            let mut server_seen = false;
            loop {
                tokio::select! {
                    _ = &mut sleep => {
                        assert!(false, "test timed out after {:?}", timeout_duration);
                        break
                    }
                    event = client.next() => {
                        println!("client event:\n{:?}\n", event);
                        match event {
                            Event::Message { content } => {
                                assert_eq!(content, TEST_MESSAGE);
                                client_seen = true;
                            }
                            _ => { assert!(false, "client retured error") }
                        }
                        event_count = event_count + 1;
                    }
                    event = server.next() => {
                        println!("server event:\n{:?}\n", event);
                        match event {
                            Event::Message { content }  => {
                                assert_eq!(content, TEST_MESSAGE);
                                server_seen = true;
                            }
                            _ => { assert!(false, "server returned error") }
                        }
                        event_count = event_count + 1;
                    }
                }
                if event_count >= 2 {
                    break
                }
            };
            assert!(client_seen);
            assert!(server_seen);
        });
    }

}
