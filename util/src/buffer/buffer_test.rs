use super::*;

use tokio::prelude::*;
use tokio_test::assert_ok;

#[tokio::test]
async fn test_buffer() {
    let mut buffer = assert_ok!(Buffer::new(1, 0));
    let packet: Vec<u8> = vec![0; 4];

    // Write once
    let n = assert_ok!(buffer.write(&packet).await);
    assert_eq!(n, 4, "n must be 4");
}
