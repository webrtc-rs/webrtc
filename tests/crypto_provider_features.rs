//! Feature semantics for the crypto provider, asserted through the async crate's public API.
//!
//! `webrtc`'s `ring` and `aws-lc-rs` features forward to `rtc` and are additive: enabling both
//! compiles both built-ins and leaves `ring` as the resolved default. These tests pin that so a
//! change to the forwarding in `Cargo.toml` cannot silently alter which provider a user gets.
//!
//! The matching build matrix lives in `.github/workflows/cargo.yml`.

use webrtc::peer_connection::crypto;

/// With at least one built-in enabled, `default_provider()` resolves.
#[cfg(any(feature = "crypto-ring", feature = "crypto-aws-lc-rs"))]
#[test]
fn a_built_in_provider_resolves() {
    let provider = crypto::default_provider().expect("a built-in provider is enabled");
    assert!(
        !provider.name().is_empty(),
        "a provider must report a name for logs and getStats"
    );
}

/// `ring` is the default whenever it is enabled, including alongside `aws-lc-rs`. Adding the
/// second feature — which Cargo will do on its own if any dependency asks for it — must not
/// change what an application ends up running.
#[cfg(feature = "crypto-ring")]
#[test]
fn ring_is_the_default_even_when_aws_lc_rs_is_also_enabled() {
    let provider = crypto::default_provider().expect("ring is enabled");
    assert_eq!(
        provider.name(),
        "ring",
        "enabling aws-lc-rs alongside ring must not change the resolved default"
    );
}

/// With only `aws-lc-rs`, that is what resolves.
#[cfg(all(feature = "crypto-aws-lc-rs", not(feature = "crypto-ring")))]
#[test]
fn aws_lc_rs_resolves_when_it_is_the_only_built_in() {
    let provider = crypto::default_provider().expect("aws-lc-rs is enabled");
    assert_eq!(provider.name(), "aws-lc-rs");
}

/// Both providers are constructible when both features are on, so a process can drive one peer
/// connection on each.
#[cfg(all(feature = "crypto-ring", feature = "crypto-aws-lc-rs"))]
#[test]
fn both_built_ins_are_available_together() {
    use std::sync::Arc;

    let ring: Arc<dyn crypto::RTCCryptoProvider> = Arc::new(crypto::providers::RingProvider::new());
    let aws: Arc<dyn crypto::RTCCryptoProvider> =
        Arc::new(crypto::providers::AwsLcRsProvider::new());

    assert_eq!(ring.name(), "ring");
    assert_eq!(aws.name(), "aws-lc-rs");
}

/// With no built-in compiled, resolution fails with a diagnosable error rather than panicking.
/// An application in this configuration supplies its own provider through
/// `SettingEngine::set_crypto_provider`.
#[cfg(not(any(feature = "crypto-ring", feature = "crypto-aws-lc-rs")))]
#[test]
fn no_built_in_reports_an_actionable_error() {
    // `expect_err` needs `Debug` on the Ok type, which `RTCCryptoProvider` deliberately does not
    // implement — provider state can hold key material.
    let message = match crypto::default_provider() {
        Ok(_) => panic!("no built-in provider should be compiled in this configuration"),
        Err(error) => error.to_string(),
    };
    assert!(
        !message.is_empty(),
        "the error must say something a user can act on: {message}"
    );
}
