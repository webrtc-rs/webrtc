use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_handshake_message_finished() -> Result<()> {
    let raw_finished = vec![
        0x01, 0x01, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    ];
    let parsed_finished = HandshakeMessageFinished {
        verify_data: raw_finished.clone(),
    };

    let mut reader = BufReader::new(raw_finished.as_slice());
    let c = HandshakeMessageFinished::unmarshal(&mut reader)?;
    assert_eq!(
        c, parsed_finished,
        "handshakeMessageFinished unmarshal: got {c:?}, want {parsed_finished:?}"
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_finished,
        "handshakeMessageFinished marshal: got {raw:?}, want {raw_finished:?}"
    );

    Ok(())
}
