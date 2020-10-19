use super::*;

//use std::io::{BufReader, BufWriter};
use std::time::{Duration, SystemTime};

use util::Error;

#[test]
fn test_handshake_message_client_hello() -> Result<(), Error> {
    let _raw_client_hello = vec![
        0xfe, 0xfd, 0xb6, 0x2f, 0xce, 0x5c, 0x42, 0x54, 0xff, 0x86, 0xe1, 0x24, 0x41, 0x91, 0x42,
        0x62, 0x15, 0xad, 0x16, 0xc9, 0x15, 0x8d, 0x95, 0x71, 0x8a, 0xbb, 0x22, 0xd7, 0x47, 0xec,
        0xd8, 0x3d, 0xdc, 0x4b, 0x00, 0x14, 0xe6, 0x14, 0x3a, 0x1b, 0x04, 0xea, 0x9e, 0x7a, 0x14,
        0xd6, 0x6c, 0x57, 0xd0, 0x0e, 0x32, 0x85, 0x76, 0x18, 0xde, 0xd8, 0x00, 0x04, 0xc0, 0x2b,
        0xc0, 0x0a, 0x01, 0x00, 0x00, 0x08, 0x00, 0x0a, 0x00, 0x04, 0x00, 0x02, 0x00, 0x1d,
    ];

    let gmt_unix_time = if let Some(unix_time) =
        SystemTime::UNIX_EPOCH.checked_add(Duration::new(3056586332u64, 0))
    {
        unix_time
    } else {
        SystemTime::UNIX_EPOCH
    };
    let _parsed_client_hello = HandshakeMessageClientHello {
        version: ProtocolVersion {
            major: 0xFE,
            minor: 0xFD,
        },
        random: HandshakeRandom {
            gmt_unix_time,
            random_bytes: [
                0x42, 0x54, 0xff, 0x86, 0xe1, 0x24, 0x41, 0x91, 0x42, 0x62, 0x15, 0xad, 0x16, 0xc9,
                0x15, 0x8d, 0x95, 0x71, 0x8a, 0xbb, 0x22, 0xd7, 0x47, 0xec, 0xd8, 0x3d, 0xdc, 0x4b,
            ],
        },
        cookie: vec![
            0xe6, 0x14, 0x3a, 0x1b, 0x04, 0xea, 0x9e, 0x7a, 0x14, 0xd6, 0x6c, 0x57, 0xd0, 0x0e,
            0x32, 0x85, 0x76, 0x18, 0xde, 0xd8,
        ],
        cipher_suites: vec![],
        //&cipherSuiteTLSEcdheEcdsaWithAes128GcmSha256{},
        //&cipherSuiteTLSEcdheEcdsaWithAes256CbcSha{},
        //],
        compression_methods: CompressionMethods {
            ids: vec![CompressionMethodId::Null],
        },
        extensions: vec![],
        //extension{
        //&extensionSupportedEllipticCurves{ellipticCurves: []namedCurve{namedCurveX25519}},
        //}],
    };
    /*
    c := &handshakeMessageClientHello{}
    if err := c.Unmarshal(rawClientHello); err != nil {
        t.Error(err)
    } else if !reflect.DeepEqual(c, parsedClientHello) {
        t.Errorf("handshakeMessageClientHello unmarshal: got %#v, want %#v", c, parsedClientHello)
    }

    raw, err := c.Marshal()
    if err != nil {
        t.Error(err)
    } else if !reflect.DeepEqual(raw, rawClientHello) {
        t.Errorf("handshakeMessageClientHello marshal: got %#v, want %#v", raw, rawClientHello)
    }*/

    Ok(())
}
