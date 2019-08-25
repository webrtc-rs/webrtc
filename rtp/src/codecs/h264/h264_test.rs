use super::*;

use std::io::BufReader;

use rand::Rng;

use utils::Error;

#[test]
fn test_h264_payload() -> Result<(), Error> {
    let pck = H264Payloader;

    let empty = vec![];
    let small_payload = vec![0x90, 0x90, 0x90];
    let multiple_payload = vec![0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x01, 0x90];
    let small_payload2 = vec![0x09, 0x00, 0x00];

    // Positive MTU, empty payload
    let mut reader = BufReader::new(empty.as_slice());
    let result = pck.payload(1, &mut reader)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // Negative MTU, small payload
    let mut reader = BufReader::new(small_payload.as_slice());
    let result = pck.payload(-1, &mut reader)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // 0 MTU, small payload
    let mut reader = BufReader::new(small_payload.as_slice());
    let result = pck.payload(0, &mut reader)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let mut reader = BufReader::new(small_payload.as_slice());
    let result = pck.payload(1, &mut reader)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let mut reader = BufReader::new(small_payload.as_slice());
    let result = pck.payload(5, &mut reader)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");
    assert_eq!(
        result[0].len(),
        small_payload.len(),
        "Generated payload should be the same size as original payload size"
    );

    // Multiple NALU in a single payload
    let mut reader = BufReader::new(multiple_payload.as_slice());
    let result = pck.payload(5, &mut reader)?;
    assert_eq!(result.len(), 2, "2 nal units should be broken out");
    for i in 0..2 {
        assert_eq!(
            result[i].len(),
            1,
            "Payload {} of 2 is packed incorrectly",
            i + 1,
        );
    }

    // Nalu type 9 or 12
    let mut reader = BufReader::new(small_payload2.as_slice());
    let result = pck.payload(5, &mut reader)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    Ok(())
}
