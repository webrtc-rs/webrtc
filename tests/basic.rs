
mod mocks;
mod test_runner;

use mocks::{
    dtls::{
        self,
        Config,
        ConfigBuilder,
        CertConfigBuilder,
        CipherSuite,
        MTU,
        TcpPort
    },
    test_runner::{simple_read_write, check_comms},
};
use tokio::{
    self,
    sync::oneshot,
    time::{Duration, sleep},
};

#[test]
pub fn e2e_basic() {
    let cipher_suites: [CipherSuite; 2] = [
        CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
        CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
    ];
    for cs in cipher_suites.iter() {
        let cert = CertConfigBuilder::default()
            .self_signed(true)
            .build()
            .unwrap();
        let conf = ConfigBuilder::default()
            .cipher_suites(vec!(*cs))
            .certificates(vec!(cert))
            .insecure_skip_verify(true)
            .build()
            .unwrap();
        check_comms(conf);
    }
}

#[test]
pub fn e2e_simple_psk() {
    let cipher_suites: [CipherSuite; 3] = [
        CipherSuite::TLS_PSK_WITH_AES_128_CCM,
        CipherSuite::TLS_PSK_WITH_AES_128_CCM_8,
        CipherSuite::TLS_PSK_WITH_AES_128_GCM_SHA256,
    ];
    for cs in cipher_suites.iter() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let conf = ConfigBuilder::default()
            .psk_callback(Some(&|_| { vec!(0xAB, 0xC1, 0x23,) }))
            .psk_id_hint(vec!(0x01, 0x02, 0x03, 0x04, 0x05))
            .cipher_suites(vec!(*cs))
            .build()
            .unwrap();
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
        let cert = CertConfigBuilder::default()
            .self_signed(true)
            .host("localhost".to_string())
            .build()
            .unwrap();
        let conf = ConfigBuilder::default()
            .certificates(vec!(cert))
            .cipher_suites(vec!(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256))
            .insecure_skip_verify(true)
            .mtu(*mtu)
            .build()
            .unwrap();
        check_comms(conf);
    }
}

async fn run_client(
    config: Config,
    server_port: TcpPort,
    start_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<String, String>
{
    let timeout = Duration::from_secs(1);
    let sleep = sleep(timeout);
    tokio::pin!(sleep);
    tokio::select! {
        _ = &mut sleep => { panic!("Client timed out waiting for server after {:?}", timeout) }
        _ = start_rx => {}  // Do nothing
    }
    let dial = dtls::dial(
        "udp".to_string(),
        "127.0.0.1".to_string(),
        server_port,
        config
    );
    let stream = match dial.await {
        Ok(stream) => stream,
        Err(e) => return Err(e.to_string()),
    };
    simple_read_write(stream).await
}

async fn run_server(
    config: Config,
    server_port: TcpPort,
    ready_tx: oneshot::Sender<()>,
) -> Result<String, String>
{
    // Listen for new connections
    let listen = dtls::listen(
        "udp".to_string(),
        "127.0.0.1".to_string(),
        server_port,
        config
    );
    let listener = match listen.await {
        Ok(listener) => listener,
        Err(e) => panic!(e),
    };
    match ready_tx.send(()) {
        Ok(_) => {},
        Err(e) => panic!(e),
    }
    let (stream, _addr) = match listener.accept().await {
        Ok((stream, addr)) => (stream, addr),
        Err(e) => panic!(e),
    };
    simple_read_write(stream).await
}
