
mod mocks;

use mocks::dtls::{self, Client, Server, Config, Cert, CertConfig, CipherSuite, MTU};
use mocks::transport;
use tokio::time::{sleep, Duration};
use rand::prelude::*;

#[derive(Copy)]
#[derive(Clone)]
struct TestCase {
    loss_chance: u8,
    do_client_auth: bool,
    cipher_suite: Option<CipherSuite>,
    mtu: MTU,
}

// TODO find macro for this
impl TestCase {
    pub fn new() -> Self {
        TestCase {
            loss_chance: 0,
            do_client_auth: false,
            cipher_suite: None,
            mtu: 0,
        }
    }
    pub fn loss_chance(&self, loss_chance: u8) -> Self {
        TestCase {
            loss_chance,
            do_client_auth: self.do_client_auth,
            cipher_suite: self.cipher_suite,
            mtu: self.mtu,
        }
    }
    pub fn do_client_auth(&self) -> Self {
        TestCase {
            loss_chance: self.loss_chance,
            do_client_auth: true,
            cipher_suite: self.cipher_suite,
            mtu: self.mtu,
        }
    }
    pub fn mtu(&self, mtu: MTU) -> Self {
        TestCase {
            loss_chance: self.loss_chance,
            do_client_auth: self.do_client_auth,
            cipher_suite: self.cipher_suite,
            mtu: mtu,
        }
    }
    pub fn cipher_suite(&self, cipher_suite: CipherSuite) -> Self {
        TestCase {
            loss_chance: self.loss_chance,
            do_client_auth: self.do_client_auth,
            cipher_suite: Some(cipher_suite),
            mtu: self.mtu,
        }
    }
}

const LOSSY_TEST_TIMEOUT: Duration = Duration::from_secs(30);

#[test]
pub fn e2e_lossy() {
    let server_cert = Cert::new(CertConfig::new().self_signed());
    let client_cert = Cert::new(CertConfig::new().self_signed());
    let cases: Vec<TestCase> = vec!(
        TestCase::new().loss_chance(0),
        TestCase::new().loss_chance(10),
        TestCase::new().loss_chance(20),
        TestCase::new().loss_chance(50),
        TestCase::new().loss_chance(0).do_client_auth(),
        TestCase::new().loss_chance(10).do_client_auth(),
        TestCase::new().loss_chance(20).do_client_auth(),
        TestCase::new().loss_chance(50).do_client_auth(),
        TestCase::new().loss_chance(0).cipher_suite(dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA),
        TestCase::new().loss_chance(10).cipher_suite(dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA),
        TestCase::new().loss_chance(20).cipher_suite(dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA),
        TestCase::new().loss_chance(50).cipher_suite(dtls::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA),
        TestCase::new().loss_chance(0).do_client_auth().mtu(100),
        TestCase::new().loss_chance(10).do_client_auth().mtu(100),
        TestCase::new().loss_chance(20).do_client_auth().mtu(100),
        TestCase::new().loss_chance(50).do_client_auth().mtu(100),
    );
    for case in cases {
        let mut name = format!("Loss{}_MTU{}", case.loss_chance, case.mtu);
        if case.do_client_auth {
            name = format!("{}_WithCliAuth", name);
        }
        match case.cipher_suite {
            Some(cipher_suite) => {
                name = format!("{}_With{}", name, cipher_suite);
            }
            _ => {
                // do nothing
            }
        }
        println!("Test: {}", name);
        let flight_interval = Duration::from_millis(100);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on( async move {
            let chosen_loss = rand::thread_rng().gen_range(0..9) + case.loss_chance;
            let bridge = transport::Bridge::new();
            bridge.set_loss_chance(chosen_loss);

            let client = tokio::spawn( async move {
                let mut config = Config::new()
                    .flight_interval(flight_interval)
                    .insecure_skip_verify()
                    .mtu(case.mtu);
                match case.cipher_suite {
                    Some(cipher_suite) => {
                        config = config.cipher_suite(cipher_suite);
                    }
                    _ => {}  // do nothing
                }
                if case.do_client_auth {
                    config = config.cert(client_cert);
                }
                return Client::new(bridge.get_connection(), config);
            });

            let server = tokio::spawn( async move {
                let mut config = Config::new()
                    .cert(server_cert)
                    .flight_interval(flight_interval)
                    .mtu(case.mtu);
                if case.do_client_auth {
                    config = config.client_auth_type(dtls::ClientAuthType::RequireAnyClientCert);
                }
                return Server::new(bridge.get_connection(), config);
            });

            let test_timeout = sleep(LOSSY_TEST_TIMEOUT);
            let server_conn = None;
            let client_conn = None;
            let server_done = false;
            let client_done = false;
            tokio::pin!(test_timeout);
            tokio::pin!(client_conn);
            tokio::pin!(server_conn);
            tokio::pin!(client);
            tokio::pin!(server);
            loop {
                let iter_timeout = tokio::time::sleep(Duration::from_secs(10));
                match (*server_conn, *client_conn) {
                    (Some(_srv_conn), Some(_cli_conn)) => {
                        // TODO check for expected props
                        break;
                    }
                    (_, _) => {
                        tokio::select! {
                            result = &mut server => {
                                match result {
                                    Ok(Ok(server)) => {
                                        let conn = server.get_connection();
                                        *server_conn = Some(*conn);
                                    }
                                    Ok(Err(e)) => {
                                        fail("server".to_string(),
                                        client_done, server_done, chosen_loss, e.to_string());
                                    }
                                    Err(e) => {
                                        fail("server".to_string(),
                                            client_done, server_done, chosen_loss, e.to_string());
                                    }
                                }
                            }
                            result = &mut client => {
                                match result {
                                    Ok(Ok(client)) => {
                                        let conn = client.get_connection();
                                        *client_conn = Some(*conn);
                                    }
                                    Ok(Err(e)) => {
                                        fail("client".to_string(),
                                        client_done, server_done, chosen_loss, e.to_string());
                                    }
                                    Err(e) => {
                                        fail("client".to_string(),
                                            client_done, server_done, chosen_loss, e.to_string());
                                    }
                                }
                            }
                            _ = &mut test_timeout => {
                                assert!(
                                    false,
                                    "Test expired: clientComplete({}) serverComplete({}) LossChance({})",
                                    client_done, server_done, chosen_loss
                                );
                            }
                            _ = iter_timeout => {
                                // Do nothing
                            }
                        }
                    }
                }
            }
        });
    }
}

fn fail(name: String, client_done: bool, server_done: bool, chosen_loss: u8, msg: String) {
    assert!(
        false,
        "{} error: clientComplete({}) serverComplete({}) LossChance({}) error({})",
        name, client_done, server_done, chosen_loss, msg,
    );
}