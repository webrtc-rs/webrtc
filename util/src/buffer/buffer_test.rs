use super::*;

use tokio::prelude::*;
use tokio_test::assert_ok;
use tokio::timer::delay;

use std::time::Duration;

#[tokio::test]
async fn test_buffer() {
    let mut buffer = Buffer::new(0, 0);
    let mut packet: Vec<u8> = vec![0; 4];

    // Write once
    let n = assert_ok!(buffer.write(&[0, 1]).await);
    assert_eq!(n, 2, "n must be 2");

    // Read once
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[0, 1], &packet[..n]);

    // Write twice
    let n = assert_ok!(buffer.write(&[2, 3, 4]).await);
    assert_eq!(n, 3, "n must be 3");

    let n = assert_ok!(buffer.write(&[5, 6, 7]).await);
    assert_eq!(n, 3, "n must be 3");

    // Read twice
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 3, "n must be 3");
    assert_eq!(&[2, 3, 4], &packet[..n]);

    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 3, "n must be 3");
    assert_eq!(&[5, 6, 7], &packet[..n]);

    // Write once prior to close.
    let n = assert_ok!(buffer.write(&[3]).await);
    assert_eq!(n, 1, "n must be 1");

    // Close
    buffer.close().await;

    // Future writes will error
    let result = buffer.write(&[4]).await;
    assert!(result.is_err());

    // But we can read the remaining data.
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 1, "n must be 1");
    assert_eq!(&[3], &packet[..n]);

    // Until EOF
    let result = buffer.read(&mut packet).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(err, ERR_BUFFER_CLOSED.clone());
    }
}

#[tokio::test]
async fn test_buffer_async() {
    let mut buffer = Buffer::new(0, 0);

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);

    let mut buffer2 = buffer.clone();
    tokio::spawn(async move {
        let mut packet:Vec<u8> = vec![0; 4];

        let n = assert_ok!(buffer2.read(&mut packet).await);
        assert_eq!(n, 2, "n must be 2");
        assert_eq!(&[0, 1], &packet[..n]);

        let result = buffer2.read(&mut packet).await;
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err, ERR_BUFFER_CLOSED.clone());
        }

        drop(done_tx);
    });

    // Wait for the reader to start reading.
    let when = tokio::clock::now() + Duration::from_millis(1);
    delay(when).await;

    // Write once
    let n = assert_ok!(buffer.write(&[0, 1]).await);
    assert_eq!(n, 2, "n must be 2");

    // Wait for the reader to start reading again.
    let when = tokio::clock::now() + Duration::from_millis(1);
    delay(when).await;

    // Close will unblock the reader.
    buffer.close().await;

    done_rx.recv().await;
}