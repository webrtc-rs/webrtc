use std::io::{BufReader, BufWriter};

use super::*;

#[test]
fn test_handshake_message_server_hello_done() -> Result<()> {
    let raw_server_hello_done = vec![];
    let parsed_server_hello_done = HandshakeMessageServerHelloDone {};

    let mut reader = BufReader::new(raw_server_hello_done.as_slice());
    let c = HandshakeMessageServerHelloDone::unmarshal(&mut reader)?;
    assert_eq!(
        c, parsed_server_hello_done,
        "handshakeMessageServerHelloDone unmarshal: got {c:?}, want {parsed_server_hello_done:?}"
    );

    let mut raw = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
        c.marshal(&mut writer)?;
    }
    assert_eq!(
        raw, raw_server_hello_done,
        "handshakeMessageServerHelloDone marshal: got {raw:?}, want {raw_server_hello_done:?}"
    );

    Ok(())
}
