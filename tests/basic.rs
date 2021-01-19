
mod test_mods;

use test_mods::{
    dtls::{
        ConfigBuilder,
        CipherSuite,
        Certificate,
        CertConfigBuilder,
    },
    test_runner::check_comms,
};

#[test]
pub fn e2e_basic() {
    let cipher_suites = [
        CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
        CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA
    ];
    for cs in cipher_suites.iter() {
        let cert = Certificate::new(
            CertConfigBuilder::default()
                .self_signed(true)
                .build()
                .unwrap()
        );
        let config = ConfigBuilder::default()
            .cipher_suites(vec!(*cs))
            .certificates(vec!(cert))
            .insecure_skip_verify(true)
            .build()
            .unwrap();
        check_comms(config, config);
    }
}

#[test]
pub fn e2e_simple_psk() {
    let cipher_suites = [
        CipherSuite::TLS_PSK_WITH_AES_128_CCM,
        CipherSuite::TLS_PSK_WITH_AES_128_CCM_8,
        CipherSuite::TLS_PSK_WITH_AES_128_GCM_SHA256,
    ];
    for cs in cipher_suites.iter() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let config = ConfigBuilder::default()
            .psk_callback(Some(&|_| { vec!(0xAB, 0xC1, 0x23,) }))
            .psk_id_hint(vec!(0x01, 0x02, 0x03, 0x04, 0x05))
            .cipher_suites(vec!(*cs))
            .build()
            .unwrap();
        check_comms(config, config);
    }      
}

#[test]
pub fn e2e_mtu() {
    let mtus = [
        10_000,
        1000,
        100
    ];
    for mtu in mtus.iter() {
        let cert = Certificate::new(
            CertConfigBuilder::default()
                .self_signed(true)
                .host("localhost".to_string())
                .build()
                .unwrap()
        );
        let config = ConfigBuilder::default()
            .certificates(vec!(cert))
            .cipher_suites(vec!(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256))
            .insecure_skip_verify(true)
            .mtu(*mtu)
            .build()
            .unwrap();
        check_comms(config, config);
    }
}
