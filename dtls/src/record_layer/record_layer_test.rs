use std::io::{BufReader, BufWriter};

use super::record_layer_header::*;
use super::*;
use crate::change_cipher_spec::ChangeCipherSpec;

#[test]
fn test_udp_decode() -> Result<()> {
    let tests = vec![
        (
            "Change Cipher Spec, single packet",
            vec![
                0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01, 0x01,
            ],
            vec![vec![
                0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01, 0x01,
            ]],
            None,
        ),
        (
            "Change Cipher Spec, multi packet",
            vec![
                0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01, 0x01,
                0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x00, 0x01, 0x01,
            ],
            vec![
                vec![
                    0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01,
                    0x01,
                ],
                vec![
                    0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x00, 0x01,
                    0x01,
                ],
            ],
            None,
        ),
        (
            "Invalid packet length",
            vec![0x14, 0xfe],
            vec![],
            Some(Error::ErrInvalidPacketLength),
        ),
        (
            "Packet declared invalid length",
            vec![
                0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0xFF, 0x01,
            ],
            vec![],
            Some(Error::ErrInvalidPacketLength),
        ),
    ];

    for (name, data, wanted, wanted_err) in tests {
        let dtls_pkts = unpack_datagram(&data);
        if let Some(err) = wanted_err {
            if let Err(dtls) = dtls_pkts {
                assert_eq!(err.to_string(), dtls.to_string());
            } else {
                panic!("something wrong for {name} when wanted_err is Some");
            }
        } else if let Ok(pkts) = dtls_pkts {
            assert_eq!(
                wanted, pkts,
                "{name} UDP decode: got {pkts:?}, want {wanted:?}",
            );
        } else {
            panic!("something wrong for {name} when wanted_err is None");
        }
    }

    Ok(())
}

#[test]
fn test_record_layer_round_trip() -> Result<()> {
    let tests = vec![(
        "Change Cipher Spec, single packet",
        vec![
            0x14, 0xfe, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01, 0x01,
        ],
        RecordLayer {
            record_layer_header: RecordLayerHeader {
                content_type: ContentType::ChangeCipherSpec,
                protocol_version: ProtocolVersion {
                    major: 0xfe,
                    minor: 0xff,
                },
                epoch: 0,
                sequence_number: 18,
                content_len: 1,
            },
            content: Content::ChangeCipherSpec(ChangeCipherSpec {}),
        },
    )];

    for (name, data, want) in tests {
        let mut reader = BufReader::new(data.as_slice());
        let r = RecordLayer::unmarshal(&mut reader)?;

        assert_eq!(
            want, r,
            "{name} recordLayer.unmarshal: got {r:?}, want {want:?}"
        );

        let mut data2 = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(data2.as_mut());
            r.marshal(&mut writer)?;
        }
        assert_eq!(
            data, data2,
            "{name} recordLayer.marshal: got {data2:?}, want {data:?}"
        );
    }

    Ok(())
}
