#[cfg(test)]
mod test {
    use crate::{config::Config, conn::*};
    use tokio::time::timeout;
    use util::Error;

    #[tokio::test]
    async fn test_valid_communication() -> Result<(), Error> {
        println!("a");
        let server_a = DNSConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
            Config {
                local_names: vec![
                    "webrtc-rs-mdns-1.local".to_owned(),
                    "webrtc-rs-mdns-2.local".to_owned(),
                ],
                ..Default::default()
            },
        )?;

        let mut server_b = DNSConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
            Config {
                ..Default::default()
            },
        )?;

        println!("a");

        let (_a, b) = mpsc::channel(1);

        assert!(
            !server_b.query("webrtc-rs-mdns-1.local", b).await.is_err(),
            "first server_b.query timeout!"
        );

        println!("b");

        let (_a, b) = mpsc::channel(1);

        assert!(
            !server_b.query("webrtc-rs-mdns-2.local", b).await.is_err(),
            "second server_b.query timeout!"
        );

        println!("c");

        server_a.close().await?;

        println!("d");
        server_b.close().await?;

        Ok(())
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
        let mut server_a = DNSConn::server(
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
