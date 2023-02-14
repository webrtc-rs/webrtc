use super::*;

#[test]
fn test_channel_data_encode() -> Result<()> {
    let mut d = ChannelData {
        data: vec![1, 2, 3, 4],
        number: ChannelNumber(MIN_CHANNEL_NUMBER + 1),
        ..Default::default()
    };
    d.encode();

    let mut b = ChannelData::default();
    b.raw.extend_from_slice(&d.raw);
    b.decode()?;

    assert_eq!(b, d, "not equal");

    assert!(
        ChannelData::is_channel_data(&b.raw) && ChannelData::is_channel_data(&d.raw),
        "unexpected IsChannelData"
    );

    Ok(())
}

#[test]
fn test_channel_data_equal() -> Result<()> {
    let tests = vec![
        (
            "equal",
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 3],
                ..Default::default()
            },
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 3],
                ..Default::default()
            },
            true,
        ),
        (
            "number",
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER + 1),
                data: vec![1, 2, 3],
                ..Default::default()
            },
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 3],
                ..Default::default()
            },
            false,
        ),
        (
            "length",
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 3, 4],
                ..Default::default()
            },
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 3],
                ..Default::default()
            },
            false,
        ),
        (
            "data",
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 2],
                ..Default::default()
            },
            ChannelData {
                number: ChannelNumber(MIN_CHANNEL_NUMBER),
                data: vec![1, 2, 3],
                ..Default::default()
            },
            false,
        ),
    ];

    for (name, a, b, r) in tests {
        let v = a == b;
        assert_eq!(v, r, "unexpected: ({name}) {r} != {r}");
    }

    Ok(())
}

#[test]
fn test_channel_data_decode() -> Result<()> {
    let tests = vec![
        ("small", vec![1, 2, 3], Error::ErrUnexpectedEof),
        (
            "zeroes",
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            Error::ErrInvalidChannelNumber,
        ),
        (
            "bad chan number",
            vec![63, 255, 0, 0, 0, 4, 0, 0, 1, 2, 3, 4],
            Error::ErrInvalidChannelNumber,
        ),
        (
            "bad length",
            vec![0x40, 0x40, 0x02, 0x23, 0x16, 0, 0, 0, 0, 0, 0, 0],
            Error::ErrBadChannelDataLength,
        ),
    ];

    for (name, buf, want_err) in tests {
        let mut m = ChannelData {
            raw: buf,
            ..Default::default()
        };
        if let Err(err) = m.decode() {
            assert_eq!(want_err, err, "unexpected: ({name}) {want_err} != {err}");
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_channel_data_reset() -> Result<()> {
    let mut d = ChannelData {
        data: vec![1, 2, 3, 4],
        number: ChannelNumber(MIN_CHANNEL_NUMBER + 1),
        ..Default::default()
    };
    d.encode();
    let mut buf = vec![0; d.raw.len()];
    buf.copy_from_slice(&d.raw);
    d.reset();
    d.raw = buf;
    d.decode()?;

    Ok(())
}

#[test]
fn test_is_channel_data() -> Result<()> {
    let tests = vec![
        ("small", vec![1, 2, 3, 4], false),
        ("zeroes", vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], false),
    ];

    for (name, buf, r) in tests {
        let v = ChannelData::is_channel_data(&buf);
        assert_eq!(v, r, "unexpected: ({name}) {r} != {v}");
    }

    Ok(())
}

const CHANDATA_TEST_HEX: [&str; 2] = [
    "40000064000100502112a442453731722f2b322b6e4e7a5800060009443758343a33776c59000000c0570004000003e7802a00081d5136dab65b169300250000002400046e001eff0008001465d11a330e104a9f5f598af4abc6a805f26003cf802800046b334442",
    "4000022316fefd0000000000000011012c0b000120000100000000012000011d00011a308201163081bda003020102020900afe52871340bd13e300a06082a8648ce3d0403023011310f300d06035504030c06576562525443301e170d3138303831313033353230305a170d3138303931313033353230305a3011310f300d06035504030c065765625254433059301306072a8648ce3d020106082a8648ce3d030107034200048080e348bd41469cfb7a7df316676fd72a06211765a50a0f0b07526c872dcf80093ed5caa3f5a40a725dd74b41b79bdd19ee630c5313c8601d6983286c8722c1300a06082a8648ce3d0403020348003045022100d13a0a131bc2a9f27abd3d4c547f7ef172996a0c0755c707b6a3e048d8762ded0220055fc8182818a644a3d3b5b157304cc3f1421fadb06263bfb451cd28be4bc9ee16fefd0000000000000012002d10000021000200000000002120f7e23c97df45a96e13cb3e76b37eff5e73e2aee0b6415d29443d0bd24f578b7e16fefd000000000000001300580f00004c000300000000004c040300483046022100fdbb74eab1aca1532e6ac0ab267d5b83a24bb4d5d7d504936e2785e6e388b2bd022100f6a457b9edd9ead52a9d0e9a19240b3a68b95699546c044f863cf8349bc8046214fefd000000000000001400010116fefd0001000000000004003000010000000000040aae2421e7d549632a7def8ed06898c3c5b53f5b812a963a39ab6cdd303b79bdb237f3314c1da21b",
];

#[test]
fn test_chrome_channel_data() -> Result<()> {
    let mut data = vec![];
    let mut messages = vec![];

    // Decoding hex data into binary.
    for h in &CHANDATA_TEST_HEX {
        let b = match hex::decode(h) {
            Ok(b) => b,
            Err(_) => return Err(Error::Other("hex decode error".to_owned())),
        };
        data.push(b);
    }

    // All hex streams decoded to raw binary format and stored in data slice.
    // Decoding packets to messages.
    for packet in data {
        let mut m = ChannelData {
            raw: packet,
            ..Default::default()
        };

        m.decode()?;
        let mut encoded = ChannelData {
            data: m.data.clone(),
            number: m.number,
            ..Default::default()
        };
        encoded.encode();

        let mut decoded = ChannelData {
            raw: encoded.raw.clone(),
            ..Default::default()
        };

        decoded.decode()?;
        assert_eq!(decoded, m, "should be equal");

        messages.push(m);
    }
    assert_eq!(messages.len(), 2, "unexpected message slice list");

    Ok(())
}
