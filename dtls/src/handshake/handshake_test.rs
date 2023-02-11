use std::io::{BufReader, BufWriter};
use std::time::{Duration, SystemTime};

use super::*;
use crate::compression_methods::*;
use crate::handshake::handshake_message_client_hello::*;
use crate::handshake::handshake_random::HandshakeRandom;
use crate::record_layer::record_layer_header::ProtocolVersion;

#[test]
fn test_handshake_message() -> Result<()> {
    let raw_handshake_message = vec![
        0x01, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0xfe, 0xfd, 0xb6,
        0x2f, 0xce, 0x5c, 0x42, 0x54, 0xff, 0x86, 0xe1, 0x24, 0x41, 0x91, 0x42, 0x62, 0x15, 0xad,
        0x16, 0xc9, 0x15, 0x8d, 0x95, 0x71, 0x8a, 0xbb, 0x22, 0xd7, 0x47, 0xec, 0xd8, 0x3d, 0xdc,
        0x4b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let parsed_handshake = Handshake {
        handshake_header: HandshakeHeader {
            handshake_type: HandshakeType::ClientHello,
            length: 0x29,
            message_sequence: 0,
            fragment_offset: 0,
            fragment_length: 0x29,
        },
        handshake_message: HandshakeMessage::ClientHello(HandshakeMessageClientHello {
            version: ProtocolVersion {
                major: 0xFE,
                minor: 0xFD,
            },
            random: HandshakeRandom {
                gmt_unix_time: if let Some(unix_time) =
                    SystemTime::UNIX_EPOCH.checked_add(Duration::new(3056586332u64, 0))
                {
                    unix_time
                } else {
                    SystemTime::UNIX_EPOCH
                },
                random_bytes: [
                    0x42, 0x54, 0xff, 0x86, 0xe1, 0x24, 0x41, 0x91, 0x42, 0x62, 0x15, 0xad, 0x16,
                    0xc9, 0x15, 0x8d, 0x95, 0x71, 0x8a, 0xbb, 0x22, 0xd7, 0x47, 0xec, 0xd8, 0x3d,
                    0xdc, 0x4b,
                ],
            },
            cookie: vec![],
            cipher_suites: vec![],
            compression_methods: CompressionMethods { ids: vec![] },
            extensions: vec![],
        }),
    };

    let mut reader = BufReader::new(raw_handshake_message.as_slice());
    let h = Handshake::unmarshal(&mut reader)?;
    assert_eq!(
        h, parsed_handshake,
        "handshakeMessageClientHello unmarshal: got {h:?}, want {parsed_handshake:?}"
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        h.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_handshake_message,
        "handshakeMessageClientHello marshal: got {raw:?}, want {raw_handshake_message:?}"
    );

    Ok(())
}
