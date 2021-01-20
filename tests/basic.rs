
mod test_mods;

use test_mods::{
    dtls::ConfigBuilder,
    test_runner::{e2e_simple, e2e_simple_psk, e2e_mtu},
};

#[test]
pub fn simple_e2e_simple() {
    e2e_simple(ConfigBuilder::default(), ConfigBuilder::default());
}

#[test]
pub fn simple_e2e_simple_psk() {
    e2e_simple_psk(ConfigBuilder::default(), ConfigBuilder::default());
}

#[test]
pub fn simple_e2e_mtu() {
    e2e_mtu(ConfigBuilder::default(), ConfigBuilder::default());
}
