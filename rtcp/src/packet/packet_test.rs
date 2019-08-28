use super::*;

//use std::io::BufReader;

use util::Error;

#[test]
fn test_packet_unmarshal_empty() -> Result<(), Error> {
    let data = vec![];
    let result: Result<Vec<Box<dyn Packet<&mut Vec<u8>>>>, Error> = unmarshal(data.as_slice());
    if let Err(got) = result {
        let want = ErrInvalidHeader.clone();
        assert_eq!(got, want, "Unmarshal(nil) err = {}, want {}", got, want);
    } else {
        assert!(false, "want error");
    }

    Ok(())
}

#[test]
fn test_packet_invalid_header_length() -> Result<(), Error> {
    /*let data = vec![
        // Receiver Report (offset=0)
        // v=2, p=0, count=1, RR, len=100
        0x81, 0xc9, 0x0, 0x64,
    ];*/
    let data = vec![
        // Goodbye (offset=84)
        // v=2, p=0, count=1, BYE, len=100
        0x81, 0xcb, 0x0, 0x64,
    ];
    let result: Result<Vec<Box<dyn Packet<&mut Vec<u8>>>>, Error> = unmarshal(data.as_slice());
    if let Err(got) = result {
        let want = ErrPacketTooShort.clone();
        assert_eq!(
            got, want,
            "Unmarshal(invalid_header_length) err = {}, want {}",
            got, want
        );
    } else {
        assert!(false, "want error");
    }

    Ok(())
}
