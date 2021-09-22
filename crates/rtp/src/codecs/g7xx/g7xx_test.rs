use super::*;

#[test]
fn test_g7xx_payload() -> Result<()> {
    let mut pck = G711Payloader::default();

    const TEST_LEN: usize = 10000;
    const TEST_MTU: usize = 1500;

    //generate random 8-bit g722 samples
    let samples: Vec<u8> = (0..TEST_LEN).map(|_| rand::random::<u8>()).collect();

    //make a copy, for payloader input
    let mut samples_in = vec![0u8; TEST_LEN];
    samples_in.clone_from_slice(&samples);
    let samples_in = Bytes::copy_from_slice(&samples_in);

    //split our samples into payloads
    let payloads = pck.payload(TEST_MTU, &samples_in)?;

    let outcnt = ((TEST_LEN as f64) / (TEST_MTU as f64)).ceil() as usize;
    assert_eq!(
        outcnt,
        payloads.len(),
        "Generated {} payloads instead of {}",
        payloads.len(),
        outcnt
    );
    assert_eq!(&samples, &samples_in, "Modified input samples");

    let samples_out = payloads.concat();
    assert_eq!(&samples_out, &samples_in, "Output samples don't match");

    let empty = Bytes::from_static(&[]);
    let payload = Bytes::from_static(&[0x90, 0x90, 0x90]);

    // Positive MTU, empty payload
    let result = pck.payload(1, &empty)?;
    assert!(result.is_empty(), "Generated payload should be empty");

    // 0 MTU, small payload
    let result = pck.payload(0, &payload)?;
    assert_eq!(result.len(), 0, "Generated payload should be empty");

    // Positive MTU, small payload
    let result = pck.payload(10, &payload)?;
    assert_eq!(result.len(), 1, "Generated payload should be the 1");

    Ok(())
}
