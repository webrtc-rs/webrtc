use super::*;
use std::str::FromStr;

#[derive(Default)]
struct DummyObserver {
    read_ch_tx: Option<Arc<mpsc::Sender<Box<dyn Chunk + Send + Sync>>>>,
}

#[async_trait]
impl ConnObserver for DummyObserver {
    async fn write(&self, c: Box<dyn Chunk + Send + Sync>) -> Result<(), Error> {
        if let Some(read_ch_tx) = &self.read_ch_tx {
            read_ch_tx.send(c).await?;
        }
        Ok(())
    }

    fn determine_source_ip(&self, loc_ip: IpAddr, _dst_ip: IpAddr) -> Option<IpAddr> {
        Some(loc_ip)
    }
}

#[tokio::test]
async fn test_udp_conn_send_to_recv_from() -> Result<(), Error> {
    let data = b"Hello".to_vec();
    let src_addr = SocketAddr::from_str("127.0.0.1:1234")?;
    let dst_addr = SocketAddr::from_str("127.0.0.1:5678")?;

    let dummy_obs = Arc::new(Mutex::new(DummyObserver::default()));
    let dummy_obs2 = Arc::clone(&dummy_obs);
    let obs = dummy_obs2 as Arc<Mutex<dyn ConnObserver + Send + Sync>>;

    let conn = Arc::new(UdpConn::new(src_addr, None, obs));
    {
        let mut dummy = dummy_obs.lock().await;
        dummy.read_ch_tx = Some(conn.get_inbound_ch());
    }

    let conn_rx = Arc::clone(&conn);
    let data_rx = data.clone();

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];

        let (n, addr) = match conn_rx.recv_from(&mut buf).await {
            Ok((n, addr)) => (n, addr),
            Err(err) => {
                log::debug!("conn closed. exiting the read loop");
                assert!(false, "unexpected conn close: {}", err);
                return;
            }
        };

        log::debug!("read data");
        assert_eq!(data_rx.len(), n, "should match");
        assert_eq!(&data_rx, &buf[..n], "should match");
        assert_eq!(dst_addr.to_string(), addr.to_string(), "should match");
    });

    let n = match conn.send_to(&data, dst_addr).await {
        Ok(n) => n,
        Err(err) => {
            assert!(false, "should success, but got {}", err);
            return Ok(());
        }
    };

    assert_eq!(n, data.len(), "should match");

    Ok(())
}

#[tokio::test]
async fn test_udp_conn_send_recv() -> Result<(), Error> {
    let data = b"Hello".to_vec();
    let src_addr = SocketAddr::from_str("127.0.0.1:1234")?;
    let dst_addr = SocketAddr::from_str("127.0.0.1:5678")?;

    let dummy_obs = Arc::new(Mutex::new(DummyObserver::default()));
    let dummy_obs2 = Arc::clone(&dummy_obs);
    let obs = dummy_obs2 as Arc<Mutex<dyn ConnObserver + Send + Sync>>;

    let conn = Arc::new(UdpConn::new(src_addr, None, obs));
    {
        let mut dummy = dummy_obs.lock().await;
        dummy.read_ch_tx = Some(conn.get_inbound_ch());
    }

    conn.connect(dst_addr).await?;

    let conn_rx = Arc::clone(&conn);
    let data_rx = data.clone();

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];

        let n = match conn_rx.recv(&mut buf).await {
            Ok(n) => n,
            Err(err) => {
                log::debug!("conn closed. exiting the read loop");
                assert!(false, "unexpected conn close: {}", err);
                return;
            }
        };

        log::debug!("read data");
        assert_eq!(data_rx.len(), n, "should match");
        assert_eq!(&data_rx, &buf[..n], "should match");
    });

    let n = match conn.send(&data).await {
        Ok(n) => n,
        Err(err) => {
            assert!(false, "should success, but got {}", err);
            return Ok(());
        }
    };

    assert_eq!(n, data.len(), "should match");

    Ok(())
}
