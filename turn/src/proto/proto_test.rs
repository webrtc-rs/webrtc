use super::*;
use crate::error::*;

const CHROME_ALLOC_REQ_TEST_HEX: [&str; 4] = [
    "000300242112a442626b4a6849664c3630526863802f0016687474703a2f2f6c6f63616c686f73743a333030302f00000019000411000000",
    "011300582112a442626b4a6849664c36305268630009001000000401556e617574686f72697a656400150010356130323039623563623830363130360014000b61312e63796465762e7275758022001a436f7475726e2d342e352e302e33202764616e204569646572272300",
    "0003006c2112a442324e50695a437a4634535034802f0016687474703a2f2f6c6f63616c686f73743a333030302f000000190004110000000006000665726e61646f00000014000b61312e63796465762e7275000015001035613032303962356362383036313036000800145c8743f3b64bec0880cdd8d476d37b801a6c3d33",
    "010300582112a442324e50695a437a4634535034001600080001fb922b1ab211002000080001adb2f49f38ae000d0004000002588022001a436f7475726e2d342e352e302e33202764616e204569646572277475000800145d7e85b767a519ffce91dbf0a96775e370db92e3",
];

#[test]
fn test_chrome_alloc_request() -> Result<()> {
    let mut data = vec![];
    let mut messages = vec![];

    // Decoding hex data into binary.
    for h in &CHROME_ALLOC_REQ_TEST_HEX {
        let b = match hex::decode(h) {
            Ok(b) => b,
            Err(_) => return Err(Error::Other("hex decode error".to_owned())),
        };
        data.push(b);
    }

    // All hex streams decoded to raw binary format and stored in data slice.
    // Decoding packets to messages.
    for packet in data {
        let mut m = Message::new();
        m.write(&packet)?;
        messages.push(m);
    }
    assert_eq!(messages.len(), 4, "unexpected message slice list");

    Ok(())
}
