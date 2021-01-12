#[cfg(test)]
mod test {

    use crate::{config::Config, conn::*};
    use tokio::time::timeout;
    use util::Error;

    #[tokio::test]
    async fn test_valid_communication() {
        let (close_tx, mut close_rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let server_a = DNSConn::server(
                SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
                Config {
                    local_names: vec![
                        "webrtc-rs-mdns-1.local".to_owned(),
                        "webrtc-rs-mdns-2.local".to_owned(),
                    ],
                    ..Default::default()
                },
            )
            .unwrap();

            close_rx.recv().await.unwrap();
            server_a.close().await.unwrap()
        });

        let server_b = DNSConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
            Config {
                ..Default::default()
            },
        )
        .unwrap();

        let (a, b) = mpsc::channel(1);

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            a.send(()).await
        });

        let result = server_b.query("webrtc-rs-mdns-2.local", b).await;

        assert!(
            result.is_ok(),
            "first server_b.query timeout!: {:?}",
            result.err()
        );

        println!("done");

        let (a, b) = mpsc::channel(1);

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            a.send(()).await
        });

        let result = server_b.query("webrtc-rs-mdns-2.local", b).await;
        assert!(
            result.is_ok(),
            "second server_b.query timeout!: {:?}",
            result.err()
        );
        println!("closing servers");

        close_tx.send(()).await.unwrap();
        server_b.close().await.unwrap();
    }

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
