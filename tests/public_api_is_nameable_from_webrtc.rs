//! Every type reachable in a public signature must be nameable from `webrtc` alone.
//!
//! `rtc` is a private dependency: it is a git submodule of this repository, so an
//! application that adds its own `rtc` dependency resolves a *different* source. The two
//! copies are distinct types, and passing one where the other is expected does not compile
//! — the failure surfaces at the call site with two identically-spelled types, which is a
//! confusing way to learn that a re-export is missing.
//!
//! This file therefore names each argument type through `webrtc::` **and never imports
//! `rtc`**. Adding a builder method that takes an `rtc` type without re-exporting it makes
//! this test fail to compile, which is the point.

use webrtc::peer_connection::{
    CertificateParams, MulticastDnsMode, NetworkType, RTCCertificate, RTCDtlsRole,
    SctpMaxMessageSize, SettingEngineBuilder, crypto,
};

/// Builds a `SettingEngine` naming every argument type through `webrtc::`.
#[test]
fn setting_engine_arguments_are_nameable() {
    let _ = SettingEngineBuilder::new()
        .with_multicast_dns_mode(MulticastDnsMode::Disabled)
        .with_network_types(vec![NetworkType::Udp4, NetworkType::Udp6])
        .with_answering_dtls_role(RTCDtlsRole::Server)
        .with_sctp_max_message_size(SctpMaxMessageSize::Bounded(16 * 1024))
        .build();
}

/// Generates a certificate naming every argument type through `webrtc::`.
///
/// `SignatureScheme` and the provider traits arrive via the `crypto` module, which is
/// already re-exported; `CertificateParams` is the one this asserts.
#[test]
fn certificate_arguments_are_nameable() -> Result<(), Box<dyn std::error::Error>> {
    let provider = crypto::default_provider()?;
    let params = CertificateParams::new(vec!["localhost".to_owned()])?;

    RTCCertificate::generate(
        provider.crypto(),
        crypto::SignatureScheme::EcdsaP256Sha256,
        params,
    )?;

    Ok(())
}
