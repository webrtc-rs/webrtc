#[cfg(test)]
mod test {
    use crate::{config::Config, conn::*};
    use tokio::time::timeout;
    use util::Error;

    // #[tokio::test]
    // async fn test_valid_communication() {
    //     env_logger::builder().is_test(true).try_init().unwrap();

    //     log::trace!("server a created");

    //     let server_a = DNSConn::server(
    //         SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
    //         Config {
    //             local_names: vec![
    //                 "webrtc-rs-mdns-1.local".to_owned(),
    //                 "webrtc-rs-mdns-2.local".to_owned(),
    //             ],
    //             ..Default::default()
    //         },
    //     )
    //     .unwrap();

    //     let server_b = DNSConn::server(
    //         SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
    //         Config {
    //             ..Default::default()
    //         },
    //     )
    //     .unwrap();

    //     let (a, b) = mpsc::channel(1);

    //     tokio::spawn(async move {
    //         tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    //         a.send(()).await
    //     });
    //     println!("\n\n\n\nSending query\n\n\n\n\n\n",);

    //     let result = server_b.query("webrtc-rs-mdns-1.local", b).await;

    //     println!("\n\n\n\nresult is {:?}\n\n\n\n\n\n", result);
    //     assert!(
    //         result.is_ok(),
    //         "first server_b.query timeout!: {:?}",
    //         result.err()
    //     );

    //     let (a, b) = mpsc::channel(1);

    //     tokio::spawn(async move {
    //         tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
    //         a.send(()).await
    //     });

    //     println!("\n\n\n\nSending query\n\n\n\n\n\n",);

    //     let result = server_b.query("webrtc-rs-mdns-2.local", b).await;
    //     assert!(
    //         result.is_ok(),
    //         "second server_b.query timeout!: {:?}",
    //         result.err()
    //     );
    //     println!("closing servers");

    //     server_a.close().await.unwrap();
    //     server_b.close().await.unwrap();
    // }

    #[tokio::test]
    async fn test_multiple_close() -> Result<(), Error> {
        let server_a = DNSConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
            Config::default(),
        )?;

        server_a.close().await?;

        if let Err(err) = server_a.close().await {
            assert_eq!(err, *ERR_CONNECTION_CLOSED);
        } else {
            assert!(false, "expected error, but got ok");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_query_respect_timeout() -> Result<(), Error> {
        let server_a = DNSConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
            Config::default(),
        )?;

        let (a, b) = mpsc::channel(1);

        timeout(Duration::from_millis(100), a.send(()))
            .await
            .unwrap()
            .unwrap();

        let res = server_a.query("invalid-host", b).await;
        assert!(res.is_err(), "server_a.query expects timeout!");

        server_a.close().await?;

        Ok(())
    }
}
