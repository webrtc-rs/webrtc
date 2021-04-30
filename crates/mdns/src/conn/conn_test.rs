#[cfg(test)]
mod test {
    use crate::{config::Config, conn::*};
    use tokio::time::timeout;
    use util::Error;

    #[tokio::test]
    async fn test_multiple_close() -> Result<(), Error> {
        let server_a = DnsConn::server(
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
        let server_a = DnsConn::server(
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
