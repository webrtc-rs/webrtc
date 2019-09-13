#![cfg(feature = "default")]

use util::Buffer;

use util::Error;

use tokio::prelude::*;
use tokio_test::assert_ok;

#[tokio::test]
async fn test_buffer() {
    let mut buffer = Buffer::new(0, 0);
    let mut packet: Vec<u8> = vec![0; 4];

    // Write once
    let n = assert_ok!(buffer.write(&vec![0, 1]).await);
    assert_eq!(n, 2, "n must be 2");
}
