
#[cfg(test)]
mod tests {

    use crate::dtls;
    use std::time::Duration;
    use rand;

    struct RunResult {
        dtls_conn: dtls::Connection,
        error: Err,
    }

    struct TestCase {
        loss_chance: u8,
        do_client_auth: bool,
        cipher_suite: dtls::CipherSuite,
        mtu: dtls::MTU,
    }

    const LOSSY_TEST_TIMEOUT: Duration = Duration::from_secs(30);

    #[test]
    pub fn e2e_lossy() {
        server_cert = Cert::new().self_signed().build()?;
        client_cert = Cert::new().self_signed().build()?;
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
            if case.cipher_suite {
                name = format!("{}_With{}", name, case.cipher_suite);
            }
            let server_config = dtls::Config::new()
                .flight_interval(flight_interval)
                .cert(server_cert)
                .mtu(case.mtu);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on( async move {
                let chosen_loss = rand::distributions::Uniform::from(0..9) + case.loss_chance;
                let (mut client_tx, mut client_rx) = tokio::sync::watch::channel("client_done");
                let (mut server_tx, mut server_rx) = tokio::sync::watch::channel("server_done");
                let bridge = transport_test::Bridge::new();
                bridge.set_loss_chance(chosen_loss);

                tokio::spawn( async move {
                    let mut config = dtls::Config::new()
                        .flight_interval(flight_interval)
                        .cipher_suite(case.cipher_suite)
                        .insecure_skip_verify()
                        .mtu(case.mtu);
                    if case.do_client_auth {
                        config = config.cert(client_cert);
                    }
                    let maybe_client = dtls::Client::new(bridge.get_connection(), config);
                    client_tx.send(maybe_client);
                });

                tokio::spawn( async move {
                    let mut config = dtls::Config::new()
                        .cert(server_cert)
                        .flight_interval(flight_interval)
                        .mtu(case.mtu);
                    if case.do_client_auth {
                        config = config.cert(dtls::REQUIRE_ANY_CLIENT_CERT);
                    }
                    let maybe_server = dtls::Server::new(bridge.get_connection(), config);
                    server_tx.send(maybe_server);
                });

                let test_timeout = sleep(timeout_duration);
                tokio::pin!(test_timeout);
                loop {
                    let iter_timeout = sleep(Duration::from_secs(10));
                    tokio::select! {
                        server_result = server_rx => {
                            if let err = server_result.err {
                                assert!(false, "Fail, server error clientComplete({}) serverComplete({}) LossChance({}) error({})")
                                break
                            }
                        }
                        client_result = client_rx => {
                            if let err = client_result.err {
                                assert!(false, "Fail, client error clientComplete({}) serverComplete({}) LossChance({}) error({})")
                                break
                            }
                        }
                        _ = test_timeout => {
                            assert!(false, "Test expired: clientComplete({}) serverComplete({}) LossChance({})");
                        }
                        _ = iter_timeout => {
                            // Do nothing
                        }
                    }
                }
            });
        }
    }
}
