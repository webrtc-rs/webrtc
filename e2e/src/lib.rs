
mod dtls {

    use tokio_stream::{self as stream};

    pub type CipherSuite = u16;
    pub const TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256: CipherSuite = 0;
    pub const TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA: CipherSuite    = 0;
    pub const TLS_PSK_WITH_AES_128_CCM: CipherSuite                = 0;
    pub const TLS_PSK_WITH_AES_128_CCM_8: CipherSuite              = 0;
    pub const TLS_PSK_WITH_AES_128_GCM_SHA256: CipherSuite         = 0;

    pub type MTU = u16;
    // TODO
    pub type PSK = ();
    pub type PSK_Id_Hint = ();

    #[derive(Debug)]
    pub struct Client { config: Config }

    impl Client {
        pub fn new(config: Config) -> Self { Client { config }}
        pub async fn start(&self) -> Option<()> { Some(()) }
        pub async fn next(&self) -> Event { Event::Message { content: () } }
    }
    
    #[derive(Debug)]
    pub struct Server { config: Config }

    impl Server {
        pub fn new(config: Config) -> Self { Server { config }}
        pub async fn start(&self) -> Option<()> { Some(()) }
        pub async fn next(&self) -> Event { Event::Message { content: () } }
    }

    #[derive(Debug)]
    pub enum Event {
        Message { content: () },
        Error { reason: () },
    }

    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct Cert {}

    impl Cert {
        pub fn new() -> Self { Cert {} }
    }

    #[derive(Debug)]
    #[derive(Clone)]
    #[derive(Copy)]
    pub struct Config {
        cipher_suite: Option<CipherSuite>,
        cert: Option<Cert>,
        insecure_skip_verify: bool,
        psk: Option<PSK>,
        psk_id_hint: Option<PSK_Id_Hint>,
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

        pub fn psk(&self, psk: PSK, psk_id_hint: PSK_Id_Hint) -> Self {
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
    use crate::dtls::{Client, Server, Event, Cert, Config, CipherSuite, PSK, PSK_Id_Hint, MTU};
    use tokio;
    use tokio::runtime::Runtime;
    use tokio::time::{sleep, Duration};

    const test_message: () = ();
    
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
            Runtime::new().unwrap().block_on( async {
                check_comms(conf).await;
            })
        }
    }

    fn create_self_signed_cert(host: &'static str) -> Cert { Cert::new() }

    fn create_psk() -> (PSK, PSK_Id_Hint) { ((), ()) }

    async fn check_comms(config: Config) {
        let timeout_duration =  Duration::from_secs(1);
        let mut client_seen = false;
        let mut server_seen = false;
        let client = Client::new(config);
        let server = Server::new(config);
        let sleep = sleep(timeout_duration);
        tokio::pin!(sleep);
        loop {
            let mut event_count: u8 = 0;
            tokio::select! {
                _ = &mut sleep => {
                    assert!(false, "test timed out after {:?}", timeout_duration);
                    break
                }
                event = client.next() => {
                    match event {
                        Event::Message { content } => {
                            assert_eq!(content, test_message);
                            client_seen = true;
                        }
                        _ => { assert!(false, "client retured error") }
                    }
                    event_count = event_count + 1;
                }
                event = server.next() => {
                    match event {
                        Event::Message { content }  => {
                            assert_eq!(content, test_message);
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
    }

}
