use super::*;

use util::Error;

const CIPHER_CONTEXT_ALGO: ProtectionProfile = PROTECTION_PROFILE_AES128CM_HMAC_SHA1_80;
const DEFAULT_SSRC: u32 = 0;

#[test]
fn test_key_len() -> Result<(), Error> {
    let result = Context::new(vec![], vec![0; SALT_LEN], CIPHER_CONTEXT_ALGO);
    assert!(result.is_err(), "CreateContext accepted a 0 length key");

    let result = Context::new(vec![0; KEY_LEN], vec![], CIPHER_CONTEXT_ALGO);
    assert!(result.is_err(), "CreateContext accepted a 0 length salt");

    let result = Context::new(vec![0; KEY_LEN], vec![0; SALT_LEN], CIPHER_CONTEXT_ALGO);
    assert!(
        result.is_ok(),
        "CreateContext failed with a valid length key and salt"
    );

    Ok(())
}

#[test]
fn test_valid_packet_counter() -> Result<(), Error> {
    let master_key = vec![
        0x0d, 0xcd, 0x21, 0x3e, 0x4c, 0xbc, 0xf2, 0x8f, 0x01, 0x7f, 0x69, 0x94, 0x40, 0x1e, 0x28,
        0x89,
    ];
    let master_salt = vec![
        0x62, 0x77, 0x60, 0x38, 0xc0, 0x6d, 0xc9, 0x41, 0x9f, 0x6d, 0xd9, 0x43, 0x3e, 0x7c,
    ];

    let c = Context::new(master_key, master_salt, CIPHER_CONTEXT_ALGO)?;

    let s = SSRCState {
        ssrc: 4160032510,
        ..Default::default()
    };
    let expected_counter = vec![
        0xcf, 0x90, 0x1e, 0xa5, 0xda, 0xd3, 0x2c, 0x15, 0x00, 0xa2, 0x24, 0xae, 0xae, 0xaf, 0x00,
        0x00,
    ];
    let counter =
        Context::generate_counter(32846, s.rollover_counter, s.ssrc, &c.srtp_session_salt)?;
    assert_eq!(
        counter, expected_counter,
        "Session Key {:?} does not match expected {:?}",
        counter, expected_counter,
    );

    Ok(())
}

#[test]
fn test_rollover_count() -> Result<(), Error> {
    let mut s = SSRCState {
        ssrc: DEFAULT_SSRC,
        ..Default::default()
    };

    // Set initial seqnum
    s.update_rollover_count(65530);

    // We rolled over to 0
    s.update_rollover_count(0);
    assert_eq!(
        s.rollover_counter, 1,
        "rolloverCounter was not updated after it crossed 0"
    );

    s.update_rollover_count(65530);
    assert_eq!(
        s.rollover_counter, 0,
        "rolloverCounter was not updated when it rolled back, failed to handle out of order"
    );

    s.update_rollover_count(5);
    assert_eq!(
        s.rollover_counter, 1,
        "rolloverCounter was not updated when it rolled over initial, to handle out of order"
    );

    s.update_rollover_count(6);
    s.update_rollover_count(7);
    s.update_rollover_count(8);
    assert_eq!(
        s.rollover_counter, 1,
        "rolloverCounter was improperly updated for non-significant packets"
    );

    Ok(())
}
