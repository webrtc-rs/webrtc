use super::*;
use crate::key_derivation::*;

use bytes::Bytes;
use lazy_static::lazy_static;

const CIPHER_CONTEXT_ALGO: ProtectionProfile = ProtectionProfile::Aes128CmHmacSha1_80;
const DEFAULT_SSRC: u32 = 0;

#[test]
fn test_context_roc() -> Result<()> {
    let key_len = CIPHER_CONTEXT_ALGO.key_len();
    let salt_len = CIPHER_CONTEXT_ALGO.salt_len();

    let mut c = Context::new(
        &vec![0; key_len],
        &vec![0; salt_len],
        CIPHER_CONTEXT_ALGO,
        None,
        None,
    )?;

    let roc = c.get_roc(123);
    assert!(roc.is_none(), "ROC must return None for unused SSRC");

    c.set_roc(123, 100);
    let roc = c.get_roc(123);
    if let Some(r) = roc {
        assert_eq!(r, 100, "ROC is set to 100, but returned {r}")
    } else {
        panic!("ROC must return value for used SSRC");
    }

    Ok(())
}

#[test]
fn test_context_index() -> Result<()> {
    let key_len = CIPHER_CONTEXT_ALGO.key_len();
    let salt_len = CIPHER_CONTEXT_ALGO.salt_len();

    let mut c = Context::new(
        &vec![0; key_len],
        &vec![0; salt_len],
        CIPHER_CONTEXT_ALGO,
        None,
        None,
    )?;

    let index = c.get_index(123);
    assert!(index.is_none(), "Index must return None for unused SSRC");

    c.set_index(123, 100);
    let index = c.get_index(123);
    if let Some(i) = index {
        assert_eq!(i, 100, "Index is set to 100, but returned {i}");
    } else {
        panic!("Index must return true for used SSRC")
    }

    Ok(())
}

#[test]
fn test_key_len() -> Result<()> {
    let key_len = CIPHER_CONTEXT_ALGO.key_len();
    let salt_len = CIPHER_CONTEXT_ALGO.salt_len();

    let result = Context::new(&[], &vec![0; salt_len], CIPHER_CONTEXT_ALGO, None, None);
    assert!(result.is_err(), "CreateContext accepted a 0 length key");

    let result = Context::new(&vec![0; key_len], &[], CIPHER_CONTEXT_ALGO, None, None);
    assert!(result.is_err(), "CreateContext accepted a 0 length salt");

    let result = Context::new(
        &vec![0; key_len],
        &vec![0; salt_len],
        CIPHER_CONTEXT_ALGO,
        None,
        None,
    );
    assert!(
        result.is_ok(),
        "CreateContext failed with a valid length key and salt"
    );

    Ok(())
}

#[test]
fn test_valid_packet_counter() -> Result<()> {
    let master_key = vec![
        0x0d, 0xcd, 0x21, 0x3e, 0x4c, 0xbc, 0xf2, 0x8f, 0x01, 0x7f, 0x69, 0x94, 0x40, 0x1e, 0x28,
        0x89,
    ];
    let master_salt = vec![
        0x62, 0x77, 0x60, 0x38, 0xc0, 0x6d, 0xc9, 0x41, 0x9f, 0x6d, 0xd9, 0x43, 0x3e, 0x7c,
    ];

    let srtp_session_salt = aes_cm_key_derivation(
        LABEL_SRTP_SALT,
        &master_key,
        &master_salt,
        0,
        master_salt.len(),
    )?;

    let s = SrtpSsrcState {
        ssrc: 4160032510,
        ..Default::default()
    };
    let expected_counter = vec![
        0xcf, 0x90, 0x1e, 0xa5, 0xda, 0xd3, 0x2c, 0x15, 0x00, 0xa2, 0x24, 0xae, 0xae, 0xaf, 0x00,
        0x00,
    ];
    let counter = generate_counter(32846, s.rollover_counter, s.ssrc, &srtp_session_salt)?;
    assert_eq!(
        counter, expected_counter,
        "Session Key {counter:?} does not match expected {expected_counter:?}",
    );

    Ok(())
}

#[test]
fn test_rollover_count() -> Result<()> {
    let mut s = SrtpSsrcState {
        ssrc: DEFAULT_SSRC,
        ..Default::default()
    };

    // Set initial seqnum
    let roc = s.next_rollover_count(65530);
    assert_eq!(roc, 0, "Initial rolloverCounter must be 0");
    s.update_rollover_count(65530);

    // Invalid packets never update ROC
    s.next_rollover_count(0);
    s.next_rollover_count(0x4000);
    s.next_rollover_count(0x8000);
    s.next_rollover_count(0xFFFF);
    s.next_rollover_count(0);

    // We rolled over to 0
    let roc = s.next_rollover_count(0);
    assert_eq!(roc, 1, "rolloverCounter was not updated after it crossed 0");
    s.update_rollover_count(0);

    let roc = s.next_rollover_count(65530);
    assert_eq!(
        roc, 0,
        "rolloverCounter was not updated when it rolled back, failed to handle out of order"
    );
    s.update_rollover_count(65530);

    let roc = s.next_rollover_count(5);
    assert_eq!(
        roc, 1,
        "rolloverCounter was not updated when it rolled over initial, to handle out of order"
    );
    s.update_rollover_count(5);

    s.next_rollover_count(6);
    s.update_rollover_count(6);

    s.next_rollover_count(7);
    s.update_rollover_count(7);

    let roc = s.next_rollover_count(8);
    assert_eq!(
        roc, 1,
        "rolloverCounter was improperly updated for non-significant packets"
    );
    s.update_rollover_count(8);

    // valid packets never update ROC
    let roc = s.next_rollover_count(0x4000);
    assert_eq!(
        roc, 1,
        "rolloverCounter was improperly updated for non-significant packets"
    );
    s.update_rollover_count(0x4000);

    let roc = s.next_rollover_count(0x8000);
    assert_eq!(
        roc, 1,
        "rolloverCounter was improperly updated for non-significant packets"
    );
    s.update_rollover_count(0x8000);

    let roc = s.next_rollover_count(0xFFFF);
    assert_eq!(
        roc, 1,
        "rolloverCounter was improperly updated for non-significant packets"
    );
    s.update_rollover_count(0xFFFF);

    let roc = s.next_rollover_count(0);
    assert_eq!(
        roc, 2,
        "rolloverCounter must be incremented after wrapping, got {roc}"
    );

    Ok(())
}

lazy_static! {
    static ref MASTER_KEY: Bytes = Bytes::from_static(&[
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ]);
    static ref MASTER_SALT: Bytes = Bytes::from_static(&[
        0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab,
    ]);
    static ref DECRYPTED_RTP_PACKET: Bytes = Bytes::from_static(&[
        0x80, 0x0f, 0x12, 0x34, 0xde, 0xca, 0xfb, 0xad, 0xca, 0xfe, 0xba, 0xbe, 0xab, 0xab, 0xab,
        0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
    ]);
    static ref ENCRYPTED_RTP_PACKET: Bytes = Bytes::from_static(&[
        0x80, 0x0f, 0x12, 0x34, 0xde, 0xca, 0xfb, 0xad, 0xca, 0xfe, 0xba, 0xbe, 0xc5, 0x00, 0x2e,
        0xde, 0x04, 0xcf, 0xdd, 0x2e, 0xb9, 0x11, 0x59, 0xe0, 0x88, 0x0a, 0xa0, 0x6e, 0xd2, 0x97,
        0x68, 0x26, 0xf7, 0x96, 0xb2, 0x01, 0xdf, 0x31, 0x31, 0xa1, 0x27, 0xe8, 0xa3, 0x92,
    ]);
    static ref DECRYPTED_RTCP_PACKET: Bytes = Bytes::from_static(&[
        0x81, 0xc8, 0x00, 0x0b, 0xca, 0xfe, 0xba, 0xbe, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
        0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
    ]);
    static ref ENCRYPTED_RTCP_PACKET: Bytes = Bytes::from_static(&[
        0x81, 0xc8, 0x00, 0x0b, 0xca, 0xfe, 0xba, 0xbe, 0xc9, 0x8b, 0x8b, 0x5d, 0xf0, 0x39, 0x2a,
        0x55, 0x85, 0x2b, 0x6c, 0x21, 0xac, 0x8e, 0x70, 0x25, 0xc5, 0x2c, 0x6f, 0xbe, 0xa2, 0xb3,
        0xb4, 0x46, 0xea, 0x31, 0x12, 0x3b, 0xa8, 0x8c, 0xe6, 0x1e, 0x80, 0x00, 0x00, 0x01,
    ]);
}

#[test]
fn test_encrypt_rtp() {
    let mut ctx = Context::new(
        &MASTER_KEY,
        &MASTER_SALT,
        ProtectionProfile::AeadAes128Gcm,
        None,
        None,
    )
    .expect("Error creating srtp context");

    let gotten_encrypted_rtp_packet = ctx
        .encrypt_rtp(&DECRYPTED_RTP_PACKET)
        .expect("Error encrypting rtp payload");

    assert_eq!(gotten_encrypted_rtp_packet, *ENCRYPTED_RTP_PACKET)
}

#[test]
fn test_decrypt_rtp() {
    let mut ctx = Context::new(
        &MASTER_KEY,
        &MASTER_SALT,
        ProtectionProfile::AeadAes128Gcm,
        None,
        None,
    )
    .expect("Error creating srtp context");

    let gotten_decrypted_rtp_packet = ctx
        .decrypt_rtp(&ENCRYPTED_RTP_PACKET)
        .expect("Error decrypting rtp payload");

    assert_eq!(gotten_decrypted_rtp_packet, *DECRYPTED_RTP_PACKET)
}

#[test]
fn test_encrypt_rtcp() {
    let mut ctx = Context::new(
        &MASTER_KEY,
        &MASTER_SALT,
        ProtectionProfile::AeadAes128Gcm,
        None,
        None,
    )
    .expect("Error creating srtp context");

    let gotten_encrypted_rtcp_packet = ctx
        .encrypt_rtcp(&DECRYPTED_RTCP_PACKET)
        .expect("Error encrypting rtcp payload");

    assert_eq!(gotten_encrypted_rtcp_packet, *ENCRYPTED_RTCP_PACKET)
}

#[test]
fn test_decrypt_rtcp() {
    let mut ctx = Context::new(
        &MASTER_KEY,
        &MASTER_SALT,
        ProtectionProfile::AeadAes128Gcm,
        None,
        None,
    )
    .expect("Error creating srtp context");

    let gotten_decrypted_rtcp_packet = ctx
        .decrypt_rtcp(&ENCRYPTED_RTCP_PACKET)
        .expect("Error decrypting rtcp payload");

    assert_eq!(gotten_decrypted_rtcp_packet, *DECRYPTED_RTCP_PACKET)
}
