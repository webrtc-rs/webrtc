
#[macro_use]
extern crate derive_builder;

mod test_mods;

use test_mods::{
    transport,
    dtls::{
        self,
        Client,
        Server,
        ConfigBuilder,
        Certificate,
        CertConfigBuilder,
        CipherSuite,
        MTU,
    },
};
use tokio::time::{sleep, Duration};
use rand::prelude::*;

#[derive(Builder, Clone, Copy)]
struct TestCase {
    pub loss_chance: u8,
    pub do_client_auth: bool,
    pub cipher_suite: Option<CipherSuite>,
    pub mtu: MTU,
}

const LOSSY_TEST_TIMEOUT: Duration = Duration::from_secs(30);

#[test]
pub fn e2e_lossy() {
    let server_cert = Certificate::new(CertConfigBuilder::default().self_signed(true).build().unwrap());
    let client_cert = Certificate::new(CertConfigBuilder::default().self_signed(true).build().unwrap());
    let cases: Vec<&mut TestCaseBuilder> = vec!(
        TestCaseBuilder::default().loss_chance(0),
        TestCaseBuilder::default().loss_chance(10),
        TestCaseBuilder::default().loss_chance(20),
        TestCaseBuilder::default().loss_chance(50),
        TestCaseBuilder::default().loss_chance(0) .do_client_auth(true),
        TestCaseBuilder::default().loss_chance(10).do_client_auth(true),
        TestCaseBuilder::default().loss_chance(20).do_client_auth(true),
        TestCaseBuilder::default().loss_chance(50).do_client_auth(true),
        TestCaseBuilder::default().loss_chance(0) .cipher_suite( Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA)),
        TestCaseBuilder::default().loss_chance(10).cipher_suite(Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA)),
        TestCaseBuilder::default().loss_chance(20).cipher_suite(Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA)),
        TestCaseBuilder::default().loss_chance(50).cipher_suite(Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA)),
        TestCaseBuilder::default().loss_chance(0) .do_client_auth(true).mtu(100),
        TestCaseBuilder::default().loss_chance(10).do_client_auth(true).mtu(100),
        TestCaseBuilder::default().loss_chance(20).do_client_auth(true).mtu(100),
        TestCaseBuilder::default().loss_chance(50).do_client_auth(true).mtu(100),
    );
    for c in cases {
        let case = c.build().unwrap();
        let mut name = format!("Loss{}_MTU{}", case.loss_chance, case.mtu);
        if case.do_client_auth {
            name = format!("{}_WithCliAuth", name);
        }
        match case.cipher_suite {
            Some(cs) => name = format!("{}_With{:?}", name, cs),
            None => {},
        }
        println!("Test: {}", name);
        let flight_interval = Duration::from_millis(100);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on( async move {
            let chosen_loss = rand::thread_rng().gen_range(0..9) + case.loss_chance;
            let bridge = transport::Bridge::new();
            bridge.set_loss_chance(chosen_loss);

            let client = tokio::spawn( async move {
                let mut config = ConfigBuilder::default()
                    .flight_interval(flight_interval)
                    .insecure_skip_verify(true)
                    .mtu(case.mtu);
                match case.cipher_suite {
                    Some(cipher_suite) => {
                        config.cipher_suites(&vec!(cipher_suite));
                    }
                    _ => {}  // do nothing
                }
                if case.do_client_auth {
                    config = config.certificates(&vec!(client_cert));
                }
                return Client::new(bridge.get_connection(), config.build().unwrap());
            });

            let server = tokio::spawn( async move {
                let mut config = ConfigBuilder::default()
                    .certificates(&vec!(server_cert))
                    .flight_interval(flight_interval)
                    .mtu(case.mtu);
                if case.do_client_auth {
                    config = config.client_auth_type(dtls::ClientAuthType::RequireAnyClientCert);
                }
                return Server::new(bridge.get_connection(), config.build().unwrap());
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
                                        fail("server error".to_string(),
                                            client_done, server_done, chosen_loss,
                                            e.to_string());
                                    }
                                    Err(e) => {
                                        fail("server error".to_string(),
                                            client_done, server_done, chosen_loss,
                                            e.to_string());
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
                                        fail("client error".to_string(),
                                            client_done, server_done, chosen_loss,
                                            e.to_string());
                                    }
                                    Err(e) => {
                                        fail("client error".to_string(),
                                            client_done, server_done, chosen_loss,
                                            e.to_string());
                                    }
                                }
                            }
                            _ = &mut test_timeout => {
                                fail("timed out".to_string(),
                                    client_done, server_done, chosen_loss,
                                    format!("test timed out after {:?}", LOSSY_TEST_TIMEOUT));
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

fn fail(preamble: String, client_done: bool, server_done: bool, chosen_loss: u8, msg: String) {
    assert!(
        false,
        "{} ... clientComplete({}) serverComplete({}) LossChance({}) error({})",
        preamble, client_done, server_done, chosen_loss, msg,
    );
}