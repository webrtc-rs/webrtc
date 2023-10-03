use bytes::{Bytes, BytesMut};

use super::*;
use crate::error::Result;

#[test]
fn test_message_unmarshal_open_success() {
    let mut bytes = Bytes::from_static(&[
        0x03, // message type
        0x00, // channel type
        0x0f, 0x35, // priority
        0x00, 0xff, 0x0f, 0x35, // reliability parameter
        0x00, 0x05, // label length
        0x00, 0x08, // protocol length
        0x6c, 0x61, 0x62, 0x65, 0x6c, // label
        0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63, 0x6f, 0x6c, // protocol
    ]);

    let actual = Message::unmarshal(&mut bytes).unwrap();

    let expected = Message::DataChannelOpen(DataChannelOpen {
        channel_type: ChannelType::Reliable,
        priority: 3893,
        reliability_parameter: 16715573,
        label: b"label".to_vec(),
        protocol: b"protocol".to_vec(),
    });

    assert_eq!(actual, expected);
}

#[test]
fn test_message_unmarshal_ack_success() -> Result<()> {
    let mut bytes = Bytes::from_static(&[0x02]);

    let actual = Message::unmarshal(&mut bytes)?;
    let expected = Message::DataChannelAck(DataChannelAck {});

    assert_eq!(actual, expected);

    Ok(())
}

#[test]
fn test_message_unmarshal_invalid_message_type() {
    let mut bytes = Bytes::from_static(&[0x01]);
    let expected = Error::InvalidMessageType(0x01);
    let result = Message::unmarshal(&mut bytes);
    let actual = result.expect_err("expected err, but got ok");
    assert_eq!(actual, expected);
}

#[test]
fn test_message_marshal_size() {
    let msg = Message::DataChannelAck(DataChannelAck {});

    let actual = msg.marshal_size();
    let expected = 1;

    assert_eq!(actual, expected);
}

#[test]
fn test_message_marshal() {
    let marshal_size = 12 + 5 + 8;
    let mut buf = BytesMut::with_capacity(marshal_size);
    buf.resize(marshal_size, 0u8);

    let msg = Message::DataChannelOpen(DataChannelOpen {
        channel_type: ChannelType::Reliable,
        priority: 3893,
        reliability_parameter: 16715573,
        label: b"label".to_vec(),
        protocol: b"protocol".to_vec(),
    });

    let actual = msg.marshal_to(&mut buf).unwrap();
    let expected = marshal_size;
    assert_eq!(actual, expected);

    let bytes = buf.freeze();

    let actual = &bytes[..];
    let expected = &[
        0x03, // message type
        0x00, // channel type
        0x0f, 0x35, // priority
        0x00, 0xff, 0x0f, 0x35, // reliability parameter
        0x00, 0x05, // label length
        0x00, 0x08, // protocol length
        0x6c, 0x61, 0x62, 0x65, 0x6c, // label
        0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63, 0x6f, 0x6c, // protocol
    ];

    assert_eq!(actual, expected);
}
