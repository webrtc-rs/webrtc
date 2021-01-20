
mod test_mods;

use test_mods::{
    dtls::{ConfigBuilder, Certificate, CertConfigBuilder},
    test_runner::{e2e_simple, e2e_simple_psk, e2e_mtu},
};

fn create_self_signed_ssl_config() -> ConfigBuilder {
    let cert = Certificate::new(
        CertConfigBuilder::default()
            .self_signed(true)
            .build()
            .unwrap()
    );
    return *ConfigBuilder::default()
        .certificates(&vec!(cert));
}

#[test]
pub fn openssl_e2e_simple() {
    let config = ConfigBuilder::default();
    let ssl_config = create_self_signed_ssl_config();
    e2e_simple(config, ssl_config);
    e2e_simple(ssl_config, config);
}

#[test]
pub fn openssl_e2e_simple_psk() {
    let config = ConfigBuilder::default();
    let ssl_config = create_self_signed_ssl_config();
    e2e_simple_psk(config, ssl_config);
    e2e_simple_psk(ssl_config, config);
}

#[test]
pub fn openssl_e2e_mtus() {
    let config = ConfigBuilder::default();
    let ssl_config = create_self_signed_ssl_config();
    e2e_mtu(config, ssl_config);
    e2e_mtu(ssl_config, config);
}
