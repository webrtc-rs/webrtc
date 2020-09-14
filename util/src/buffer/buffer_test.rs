use super::*;

use tokio::time::delay_for;
use tokio_test::assert_ok;

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
        let mut packet: Vec<u8> = vec![0; 4];

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
    delay_for(Duration::from_micros(1)).await;

    // Write once
    let n = assert_ok!(buffer.write(&[0, 1]).await);
    assert_eq!(n, 2, "n must be 2");

    // Wait for the reader to start reading again.
    delay_for(Duration::from_micros(1)).await;

    // Close will unblock the reader.
    buffer.close().await;

    done_rx.recv().await;
}

#[tokio::test]
async fn test_buffer_limit_count() {
    let mut buffer = Buffer::new(2, 0);

    assert_eq!(0, buffer.count().await);

    // Write twice
    let n = assert_ok!(buffer.write(&[0, 1]).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(1, buffer.count().await);

    let n = assert_ok!(buffer.write(&[2, 3]).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(2, buffer.count().await);

    // Over capacity
    let result = buffer.write(&[4, 5]).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(err, ERR_BUFFER_FULL.clone());
    }
    assert_eq!(2, buffer.count().await);

    // Read once
    let mut packet: Vec<u8> = vec![0; 4];
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[0, 1], &packet[..n]);
    assert_eq!(1, buffer.count().await);

    // Write once
    let n = assert_ok!(buffer.write(&[6, 7]).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(2, buffer.count().await);

    // Over capacity
    let result = buffer.write(&[8, 9]).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(err, ERR_BUFFER_FULL.clone());
    }
    assert_eq!(2, buffer.count().await);

    // Read twice
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[2, 3], &packet[..n]);
    assert_eq!(1, buffer.count().await);

    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[6, 7], &packet[..n]);
    assert_eq!(0, buffer.count().await);

    // Nothing left.
    buffer.close().await;
}

#[tokio::test]
async fn test_buffer_limit_size() {
    let mut buffer = Buffer::new(0, 5);

    assert_eq!(0, buffer.size().await);

    // Write twice
    let n = assert_ok!(buffer.write(&[0, 1]).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(2, buffer.size().await);

    let n = assert_ok!(buffer.write(&[2, 3]).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(4, buffer.size().await);

    // Over capacity
    let result = buffer.write(&[4, 5]).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(err, ERR_BUFFER_FULL.clone());
    }
    assert_eq!(4, buffer.size().await);

    // Cheeky write at exact size.
    let n = assert_ok!(buffer.write(&[6]).await);
    assert_eq!(n, 1, "n must be 1");
    assert_eq!(5, buffer.size().await);

    // Read once
    let mut packet: Vec<u8> = vec![0; 4];
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[0, 1], &packet[..n]);
    assert_eq!(3, buffer.size().await);

    // Write once
    let n = assert_ok!(buffer.write(&[7, 8]).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(5, buffer.size().await);

    // Over capacity
    let result = buffer.write(&[9, 10]).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(err, ERR_BUFFER_FULL.clone());
    }
    assert_eq!(5, buffer.size().await);

    // Read everything
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[2, 3], &packet[..n]);
    assert_eq!(3, buffer.size().await);

    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 1, "n must be 1");
    assert_eq!(&[6], &packet[..n]);
    assert_eq!(2, buffer.size().await);

    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 2, "n must be 2");
    assert_eq!(&[7, 8], &packet[..n]);
    assert_eq!(0, buffer.size().await);

    // Nothing left.
    buffer.close().await;
}

#[tokio::test]
async fn test_buffer_misc() {
    let mut buffer = Buffer::new(0, 0);

    // Write once
    let n = assert_ok!(buffer.write(&[0, 1, 2, 3]).await);
    assert_eq!(n, 4, "n must be 4");
    assert_eq!(4, buffer.size().await);

    // Try to read with a short buffer
    let mut packet: Vec<u8> = vec![0; 3];
    let result = buffer.read(&mut packet).await;
    assert!(result.is_err());
    if let Err(err) = result {
        assert_eq!(err, ERR_BUFFER_SHORT.clone());
    }

    // Try again with the right size
    let mut packet: Vec<u8> = vec![0; 4];
    let n = assert_ok!(buffer.read(&mut packet).await);
    assert_eq!(n, 4, "n must be 4");
    assert_eq!(&[0, 1, 2, 3], &packet[..n]);
    assert_eq!(0, buffer.size().await);

    // Close
    buffer.close().await;

    // check is_close
    assert!(buffer.is_closed().await);

    // Make sure you can Close twice
    buffer.close().await;
}
