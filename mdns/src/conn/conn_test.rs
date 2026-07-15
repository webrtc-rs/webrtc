#[cfg(test)]
mod test {
    use std::sync::Arc;

    use tokio::time::timeout;

    use crate::config::Config;
    use crate::conn::*;

    #[tokio::test]
    async fn test_multiple_close() -> Result<()> {
        let server_a = DnsConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 5353),
            Config::default(),
        )?;

        server_a.close().await?;

        if let Err(err) = server_a.close().await {
            assert_eq!(err, Error::ErrConnectionClosed);
        } else {
            panic!("expected error, but got ok");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_close_releases_socket_while_conn_is_retained() -> Result<()> {
        let server = DnsConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
            Config::default(),
        )?;

        let socket = server.socket_weak_for_test().await;

        server.close().await?;

        assert!(
            socket.upgrade().is_none(),
            "close returned while the UDP socket was still retained"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_close_stops_pending_queries() -> Result<()> {
        let server = Arc::new(DnsConn::server(
            SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0),
            Config::default(),
        )?);
        let query_task = tokio::spawn({
            let server = Arc::clone(&server);
            async move {
                let (_close_query_tx, close_query_rx) = mpsc::channel(1);
                server.query("unresolvable.local", close_query_rx).await
            }
        });

        tokio::task::yield_now().await;
        server.close().await?;

        let query_result = timeout(Duration::from_millis(100), query_task)
            .await
            .expect("pending mDNS query did not stop when the connection closed")
            .expect("pending mDNS query task panicked");
        assert_eq!(query_result, Err(Error::ErrConnectionClosed));

        Ok(())
    }

    #[tokio::test]
    async fn test_query_respect_timeout() -> Result<()> {
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
