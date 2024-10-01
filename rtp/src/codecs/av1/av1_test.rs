use crate::codecs::av1::leb128::read_leb128;
use crate::codecs::av1::obu::{
    OBU_HAS_EXTENSION_BIT, OBU_TYPE_FRAME, OBU_TYPE_FRAME_HEADER, OBU_TYPE_METADATA,
    OBU_TYPE_SEQUENCE_HEADER, OBU_TYPE_TEMPORAL_DELIMITER, OBU_TYPE_TILE_GROUP, OBU_TYPE_TILE_LIST,
};
use crate::error::Result;

use super::*;

const OBU_EXTENSION_S1T1: u8 = 0b0010_1000;
const NEW_CODED_VIDEO_SEQUENCE_BIT: u8 = 0b0000_1000;

struct Av1Obu {
    header: u8,
    extension: u8,
    payload: Vec<u8>,
}

impl Av1Obu {
    pub fn new(obu_type: u8) -> Self {
        Self {
            header: obu_type << 3 | OBU_HAS_SIZE_BIT,
            extension: 0,
            payload: vec![],
        }
    }

    pub fn with_extension(mut self, extension: u8) -> Self {
        self.extension = extension;
        self.header |= OBU_HAS_EXTENSION_BIT;
        self
    }

    pub fn without_size(mut self) -> Self {
        self.header &= !OBU_HAS_SIZE_BIT;
        self
    }

    pub fn with_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = payload;
        self
    }
}

fn build_av1_frame(obus: &Vec<Av1Obu>) -> Bytes {
    let mut raw = vec![];
    for obu in obus {
        raw.push(obu.header);
        if obu.header & OBU_HAS_EXTENSION_BIT != 0 {
            raw.push(obu.extension);
        }
        if obu.header & OBU_HAS_SIZE_BIT != 0 {
            // write size in leb128 format.
            let mut payload_size = obu.payload.len();
            while payload_size >= 0b1000_0000 {
                raw.push(0b1000_0000 | (payload_size & 0b0111_1111) as u8);
                payload_size >>= 7;
            }
            raw.push(payload_size as u8);
        }
        raw.extend_from_slice(&obu.payload);
    }
    Bytes::from(raw)
}

#[test]
fn test_packetize_one_obu_without_size_and_extension() -> Result<()> {
    let frame = build_av1_frame(&vec![Av1Obu::new(OBU_TYPE_FRAME)
        .without_size()
        .with_payload(vec![1, 2, 3, 4, 5, 6, 7])]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0001_0000,         // aggregation header
            OBU_TYPE_FRAME << 3, // header
            1,
            2,
            3,
            4,
            5,
            6,
            7
        ]]
    );
    Ok(())
}

#[test]
fn test_packetize_one_obu_without_size_with_extension() -> Result<()> {
    let frame = build_av1_frame(&vec![Av1Obu::new(OBU_TYPE_FRAME)
        .without_size()
        .with_extension(OBU_EXTENSION_S1T1)
        .with_payload(vec![2, 3, 4, 5, 6, 7])]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0001_0000,                                 // aggregation header
            OBU_TYPE_FRAME << 3 | OBU_HAS_EXTENSION_BIT, // header
            OBU_EXTENSION_S1T1,                          // extension header
            2,
            3,
            4,
            5,
            6,
            7
        ]]
    );
    Ok(())
}

#[test]
fn removes_obu_size_field_without_extension() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![11, 12, 13, 14, 15, 16, 17])
    ]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0001_0000,         // aggregation header
            OBU_TYPE_FRAME << 3, // header
            11,
            12,
            13,
            14,
            15,
            16,
            17
        ]]
    );
    Ok(())
}

#[test]
fn removes_obu_size_field_with_extension() -> Result<()> {
    let frame = build_av1_frame(&vec![Av1Obu::new(OBU_TYPE_FRAME)
        .with_extension(OBU_EXTENSION_S1T1)
        .with_payload(vec![1, 2, 3, 4, 5, 6, 7])]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0001_0000,                                 // aggregation header
            OBU_TYPE_FRAME << 3 | OBU_HAS_EXTENSION_BIT, // header
            OBU_EXTENSION_S1T1,                          // extension header
            1,
            2,
            3,
            4,
            5,
            6,
            7
        ]]
    );
    Ok(())
}

#[test]
fn test_omits_size_for_last_obu_when_three_obus_fits_into_the_packet() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_SEQUENCE_HEADER).with_payload(vec![1, 2, 3, 4, 5, 6]),
        Av1Obu::new(OBU_TYPE_METADATA).with_payload(vec![11, 12, 13, 14]),
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![21, 22, 23, 24, 25, 26]),
    ]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0011_1000,                   // aggregation header
            7,                             // size of the first OBU
            OBU_TYPE_SEQUENCE_HEADER << 3, // header of the first OBU
            1,
            2,
            3,
            4,
            5,
            6,
            5,                      // size of the second OBU
            OBU_TYPE_METADATA << 3, // header of the second OBU
            11,
            12,
            13,
            14,
            OBU_TYPE_FRAME << 3, // header of the third OBU
            21,
            22,
            23,
            24,
            25,
            26,
        ]]
    );
    Ok(())
}

#[test]
fn test_use_size_for_all_obus_when_four_obus_fits_into_the_packet() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_SEQUENCE_HEADER).with_payload(vec![1, 2, 3, 4, 5, 6]),
        Av1Obu::new(OBU_TYPE_METADATA).with_payload(vec![11, 12, 13, 14]),
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![21, 22, 23]),
        Av1Obu::new(OBU_TYPE_TILE_GROUP).with_payload(vec![31, 32, 33, 34, 35, 36]),
    ]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0000_1000,                   // aggregation header
            7,                             // size of the first OBU
            OBU_TYPE_SEQUENCE_HEADER << 3, // header of the first OBU
            1,
            2,
            3,
            4,
            5,
            6,
            5,                      // size of the second OBU
            OBU_TYPE_METADATA << 3, // header of the second OBU
            11,
            12,
            13,
            14,
            4,                   // size of the third OBU
            OBU_TYPE_FRAME << 3, // header of the third OBU
            21,
            22,
            23,
            7,                        // size of the fourth OBU
            OBU_TYPE_TILE_GROUP << 3, // header of the fourth OBU
            31,
            32,
            33,
            34,
            35,
            36
        ]]
    );
    Ok(())
}

#[test]
fn test_discards_temporal_delimiter_and_tile_list_obu() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_TEMPORAL_DELIMITER),
        Av1Obu::new(OBU_TYPE_METADATA),
        Av1Obu::new(OBU_TYPE_TILE_LIST).with_payload(vec![1, 2, 3, 4, 5, 6]),
        Av1Obu::new(OBU_TYPE_FRAME_HEADER).with_payload(vec![21, 22, 23]),
        Av1Obu::new(OBU_TYPE_TILE_GROUP).with_payload(vec![31, 32, 33, 34, 35, 36]),
    ]);
    let mut payloader = Av1Payloader {};
    assert_eq!(
        payloader.payload(1200, &frame)?,
        vec![vec![
            0b0011_0000,                // aggregation header
            1,                          // size of the first OBU
            OBU_TYPE_METADATA << 3,     // header of the first OBU
            4,                          // size of the second OBU
            OBU_TYPE_FRAME_HEADER << 3, // header of the second OBU
            21,
            22,
            23,
            OBU_TYPE_TILE_GROUP << 3, // header of the fourth OBU
            31,
            32,
            33,
            34,
            35,
            36
        ]]
    );
    Ok(())
}

#[test]
fn test_split_two_obus_into_two_packet_force_split_obu_header() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_FRAME_HEADER)
            .with_extension(OBU_EXTENSION_S1T1)
            .with_payload(vec![21]),
        Av1Obu::new(OBU_TYPE_TILE_GROUP)
            .with_extension(OBU_EXTENSION_S1T1)
            .with_payload(vec![11, 12, 13, 14]),
    ]);
    let mut payloader = Av1Payloader {};

    // Craft expected payloads so that there is only one way to split original
    // frame into two packets.
    assert_eq!(
        payloader.payload(6, &frame)?,
        vec![
            vec![
                0b0110_0000,                                        // aggregation header
                3,                                                  // size of the first OBU
                OBU_TYPE_FRAME_HEADER << 3 | OBU_HAS_EXTENSION_BIT, // header of the first OBU
                OBU_EXTENSION_S1T1,                                 // extension header
                21,
                OBU_TYPE_TILE_GROUP << 3 | OBU_HAS_EXTENSION_BIT, // header of the second OBU
            ],
            vec![
                0b1001_0000, // aggregation header
                OBU_EXTENSION_S1T1,
                11,
                12,
                13,
                14
            ]
        ]
    );
    Ok(())
}

#[test]
fn test_sets_n_bit_at_the_first_packet_of_a_key_frame_with_sequence_header() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_SEQUENCE_HEADER).with_payload(vec![1, 2, 3, 4, 5, 6, 7])
    ]);
    let mut payloader = Av1Payloader {};
    let result = payloader.payload(6, &frame)?;
    assert_eq!(result.len(), 2);
    assert_eq!(
        result[0][0] & NEW_CODED_VIDEO_SEQUENCE_BIT,
        NEW_CODED_VIDEO_SEQUENCE_BIT
    );
    assert_eq!(result[1][0] & NEW_CODED_VIDEO_SEQUENCE_BIT, 0);
    Ok(())
}

#[test]
fn test_doesnt_set_n_bit_at_the_packets_of_a_key_frame_without_sequence_header() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![1, 2, 3, 4, 5, 6, 7])
    ]);
    let mut payloader = Av1Payloader {};
    let result = payloader.payload(6, &frame)?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[0][0] & NEW_CODED_VIDEO_SEQUENCE_BIT, 0);
    assert_eq!(result[1][0] & NEW_CODED_VIDEO_SEQUENCE_BIT, 0);
    Ok(())
}

#[test]
fn test_doesnt_set_n_bit_at_the_packets_of_a_delta_frame() -> Result<()> {
    // TODO: implement delta frame detection.
    Ok(())
}

#[test]
fn test_split_single_obu_into_two_packets() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![11, 12, 13, 14, 15, 16, 17, 18, 19])
    ]);
    let mut payloader = Av1Payloader {};
    // let result = payloader.payload(8, &frame)?;
    // println!("{:?}", result[0].to_vec());
    // println!("{:?}", result[1].to_vec());
    assert_eq!(
        payloader.payload(8, &frame)?,
        vec![
            vec![
                0b0101_0000,         // aggregation header
                OBU_TYPE_FRAME << 3, // header
                11,
                12,
                13,
                14,
                15,
                16
            ],
            vec![
                0b1001_0000, // aggregation header
                17,
                18,
                19
            ],
        ]
    );

    Ok(())
}

#[test]
fn test_split_single_obu_into_many_packets() -> Result<()> {
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![27; 1200])
    ]);
    let mut payloader = Av1Payloader {};
    let result = payloader.payload(100, &frame)?;
    assert_eq!(result.len(), 13);
    assert_eq!(result[0], {
        let mut ret = vec![
            0b0101_0000,         // aggregation header
            OBU_TYPE_FRAME << 3, // header
        ];
        ret.extend(vec![27; 98]);
        ret
    });
    for packet in result.iter().take(12).skip(1) {
        assert_eq!(packet.to_vec(), {
            let mut ret = vec![
                0b1101_0000, // aggregation header
            ];
            ret.extend(vec![27; 99]);
            ret
        });
    }
    assert_eq!(result[12], {
        let mut ret = vec![
            0b1001_0000, // aggregation header
        ];
        ret.extend(vec![27; 13]);
        ret
    });

    Ok(())
}

#[test]
fn test_split_two_obus_into_two_packets() -> Result<()> {
    // 2nd OBU is too large to fit into one packet, so its head would be in the
    // same packet as the 1st OBU.
    let frame = build_av1_frame(&vec![
        Av1Obu::new(OBU_TYPE_SEQUENCE_HEADER).with_payload(vec![11, 12]),
        Av1Obu::new(OBU_TYPE_FRAME).with_payload(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]),
    ]);
    let mut payloader = Av1Payloader {};
    let result = payloader.payload(8, &frame)?;
    assert_eq!(
        result,
        vec![
            vec![
                0b0110_1000,                   // aggregation header
                3,                             // size of the first OBU
                OBU_TYPE_SEQUENCE_HEADER << 3, // header
                11,
                12,
                OBU_TYPE_FRAME << 3, // header of the second OBU
                1,
                2
            ],
            vec![
                0b1001_0000, // aggregation header
                3,
                4,
                5,
                6,
                7,
                8,
                9
            ]
        ]
    );
    Ok(())
}

#[test]
fn read_leb128_0() {
    let bytes = vec![0u8];
    let (payload_size, leb128_size) = read_leb128(&(bytes.into()));
    assert_eq!(payload_size, 0);
    assert_eq!(leb128_size, 1);
}

#[test]
fn read_leb128_5_byte() {
    let bytes = vec![0xC3, 0x80, 0x81, 0x80, 0x00];
    let (payload_size, leb128_size) = read_leb128(&(bytes.into()));
    assert_eq!(leb128_size, 5);
    assert_eq!(payload_size, 16451);
}
