/*use super::*;


fn buildSessionSRTPPair() ->(SessionSRTP, *SessionSRTP) {
    aPipe, bPipe := net.Pipe()
    config := &Config{
        Profile: ProtectionProfileAes128CmHmacSha1_80,
        Keys: SessionKeys{
            []byte{0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE, 0x41, 0x39},
            []byte{0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6},
            []byte{0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE, 0x41, 0x39},
            []byte{0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6},
        },
    }

    aSession, err := NewSessionSRTP(aPipe, config)
    if err != nil {
        t.Fatal(err)
    } else if aSession == nil {
        t.Fatal("NewSessionSRTP did not error, but returned nil session")
    }

    bSession, err := NewSessionSRTP(bPipe, config)
    if err != nil {
        t.Fatal(err)
    } else if bSession == nil {
        t.Fatal("NewSessionSRTP did not error, but returned nil session")
    }

    return aSession, bSession
}

#[test]
fn test_session_srtp_bad_init() -> Result<(), Error> {
    if _, err := NewSessionSRTP(nil, nil); err == nil {
        t.Fatal("NewSessionSRTP should error if no config was provided")
    } else if _, err := NewSessionSRTP(nil, &Config{}); err == nil {
        t.Fatal("NewSessionSRTP should error if no net was provided")
    }

    Ok(())
}
 */
