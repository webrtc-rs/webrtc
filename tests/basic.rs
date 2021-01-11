
mod mocks;

use mocks::dtls::{self, Client, Server, Config, Event, Cert, CertConfig, CipherSuite, PSK, PSKIdHint, MTU};
use mocks::transport;
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

fn create_psk() -> (PSK, PSKIdHint) { ((), ()) }

fn check_comms(config: Config) {
    println!("Checking client server comunnication:\n{:?}\n", config);
    let timeout_duration =  Duration::from_millis(500);
    let conn = transport::Connection::new();
    let client = Client::new(&conn, config).unwrap();
    let server = Server::new(&conn, config).unwrap();
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
