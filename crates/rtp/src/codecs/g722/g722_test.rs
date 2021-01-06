// use super::*;

// use std::io::BufReader;

#[test]
fn test_g722_payload() -> Result<(), Error> {
    let pck = G722Payloader;

//     const TEST_LEN: usize = 10000;
//     const TEST_MTU: isize = 1500;

//     //generate random 8-bit g722 samples
//     let samples: Vec<u8> = (0..TEST_LEN).map(|_| rand::random::<u8>()).collect();

//     //make a copy, for payloader input
//     let mut samples_in = vec![0; TEST_LEN];
//     samples_in.clone_from_slice(&samples);

//     //split our samples into payloads
//     let mut reader = BufReader::new(samples_in.as_slice());
//     let payloads = pck.payload(TEST_MTU, &mut reader)?;

//     let outcnt = ((TEST_LEN as f64) / (TEST_MTU as f64)).ceil() as usize;
//     assert_eq!(
//         outcnt,
//         payloads.len(),
//         "Generated {} payloads instead of {}",
//         payloads.len(),
//         outcnt
//     );
//     assert_eq!(&samples, &samples_in, "Modified input samples");

//     let samples_out = payloads.concat();
//     assert_eq!(&samples_out, &samples_in, "Output samples don't match");

//     let empty = vec![];
//     let payload = vec![0x90, 0x90, 0x90];

//     // Positive MTU, empty payload
//     let mut reader = BufReader::new(empty.as_slice());
//     let result = pck.payload(1, &mut reader)?;
//     assert!(result.is_empty(), "Generated payload should be empty");

//     // 0 MTU, small payload
//     let mut reader = BufReader::new(payload.as_slice());
//     let result = pck.payload(0, &mut reader)?;
//     assert_eq!(result.len(), 0, "Generated payload should be empty");

//     // Negative MTU, small payload
//     let mut reader = BufReader::new(payload.as_slice());
//     let result = pck.payload(-1, &mut reader)?;
//     assert_eq!(result.len(), 0, "Generated payload should be empty");

//     // Positive MTU, small payload
//     let mut reader = BufReader::new(payload.as_slice());
//     let result = pck.payload(10, &mut reader)?;
//     assert_eq!(result.len(), 1, "Generated payload should be the 1");

//     Ok(())
// }
