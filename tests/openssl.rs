
mod test_mods;

use test_mods::{
    dtls::ConfigBuilder,
    test_runner::check_comms,
};

#[test]
pub fn openssl_e2e_simple() {
    let config = ConfigBuilder::default()
        .build()
        .unwrap();
    let ssl_config = ConfigBuilder::default()
        .build()
        .unwrap();
    println!("non-ssl client, ssl server:");
    check_comms(config, ssl_config);
    println!("ssl client, non-ssl server:");
    check_comms(ssl_config, config);
}

#[test]
pub fn openssl_e2e_simple_psk() {
}

#[test]
pub fn openssl_e2e_mtus() {
}
