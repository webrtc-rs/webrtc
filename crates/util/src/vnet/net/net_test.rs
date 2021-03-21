use super::*;

const DEMO_IP: &str = "1.2.3.4";

#[derive(Default)]
struct DummyObserver;

#[async_trait]
impl ConnObserver for DummyObserver {
    async fn write(&self, _c: Box<dyn Chunk + Send + Sync>) -> Result<(), Error> {
        Ok(())
    }

    fn determine_source_ip(&self, loc_ip: IpAddr, _dst_ip: IpAddr) -> Option<IpAddr> {
        Some(loc_ip)
    }
}

#[test]
fn test_net_native_interfaces() -> Result<(), Error> {
    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");

    let interfaces = nw.get_interfaces();
    log::debug!("interfaces: {:?}", interfaces);
    for ifc in interfaces {
        let addrs = ifc.addrs();
        for addr in addrs {
            log::debug!("{}", addr)
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_net_native_resolve_addr() -> Result<(), Error> {
    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");

    let udp_addr = nw.resolve_addr(true, "localhost:1234").await?;
    assert_eq!("127.0.0.1", udp_addr.ip().to_string(), "should match");
    assert_eq!(1234, udp_addr.port(), "should match");

    Ok(())
}

#[tokio::test]
async fn test_net_native_bind() -> Result<(), Error> {
    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");

    let conn = nw.bind(SocketAddr::from_str("0.0.0.0:0")?).await?;
    let laddr = conn.local_addr()?;
    log::debug!("laddr: {}", laddr);

    Ok(())
}

#[tokio::test]
async fn test_net_native_connect() -> Result<(), Error> {
    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");

    let conn = nw.bind(SocketAddr::from_str("0.0.0.0:0")?).await?;
    let laddr = conn.local_addr()?;

    let result = conn.connect(SocketAddr::from_str("0.0.0.0:1234")?).await;
    log::debug!("laddr: {}, result: {:?}", laddr, result);

    Ok(())
}

#[tokio::test]
async fn test_net_native_loopback() -> Result<(), Error> {
    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");

    let conn = nw.bind(SocketAddr::from_str("127.0.0.1:0")?).await?;
    let laddr = conn.local_addr()?;

    let msg = "PING!";
    let n = conn.send_to(msg.as_bytes(), laddr).await?;
    assert_eq!(n, msg.len(), "should match msg size {}", msg.len());

    let mut buf = vec![0u8; 1000];
    let (n, raddr) = conn.recv_from(&mut buf).await?;
    assert_eq!(n, msg.len(), "should match msg size {}", msg.len());
    assert_eq!(
        msg.as_bytes(),
        &buf[..n],
        "should match msg content {}",
        msg
    );
    assert_eq!(laddr, raddr, "should match addr {}", laddr);

    Ok(())
}

#[tokio::test]
async fn test_net_native_unexpected_operations() -> Result<(), Error> {
    let mut lo_name = String::new();
    let ifcs = ifaces::ifaces()?;
    for ifc in &ifcs {
        if let Some(addr) = ifc.addr {
            if addr.ip().is_loopback() {
                lo_name = ifc.name.clone();
                break;
            }
        }
    }

    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");
    if !lo_name.is_empty() {
        if let Some(ifc) = nw.get_interface(&lo_name) {
            assert_eq!(lo_name, ifc.name, "should match ifc name");
        } else {
            assert!(false, "should succeed");
        }
    }

    let result = nw.get_interface("foo0");
    assert!(result.is_none(), "should be none");

    let ips = nw.get_static_ips();
    assert!(ips.is_empty(), "should empty");

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_interfaces() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let interfaces = nw.get_interfaces();
    assert_eq!(2, interfaces.len(), "should be one interface");

    for ifc in interfaces {
        match ifc.name.as_str() {
            LO0_STR => {
                let addrs = ifc.addrs();
                assert_eq!(1, addrs.len(), "should be one address");
            }
            "eth0" => {
                let addrs = ifc.addrs();
                assert!(addrs.is_empty(), "should empty");
            }
            _ => {
                assert!(false, "unknown interface: {}", ifc.name);
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_interface_by_name() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let interfaces = nw.get_interfaces();
    assert_eq!(2, interfaces.len(), "should be one interface");

    if let Some(ifc) = nw.get_interface(LO0_STR) {
        assert_eq!(LO0_STR, ifc.name.as_str(), "should match");
        let addrs = ifc.addrs();
        assert_eq!(1, addrs.len(), "should be one address");
    } else {
        assert!(false, "should got ifc");
    }

    if let Some(ifc) = nw.get_interface("eth0") {
        assert_eq!("eth0", ifc.name.as_str(), "should match");
        let addrs = ifc.addrs();
        assert!(addrs.is_empty(), "should empty");
    } else {
        assert!(false, "should got ifc");
    }

    let result = nw.get_interface("foo0");
    assert!(result.is_none(), "should fail");

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_has_ipaddr() -> Result<(), Error> {
    let mut nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let interfaces = nw.get_interfaces();
    assert_eq!(2, interfaces.len(), "should be one interface");

    let ipnet = IpNet::from_str("10.1.2.3/24")?;
    nw.add_addrs_to_interface("eth0", &[ipnet]).await?;

    if let Some(ifc) = nw.get_interface("eth0") {
        let addrs = ifc.addrs();
        assert!(!addrs.is_empty(), "should not empty");
    }

    if let Net::VNet(vnet) = &nw {
        let ip = Ipv4Addr::from_str("127.0.0.1")?.into();
        assert!(vnet.has_ipaddr(ip), "the IP addr {} should exist", ip);

        let ip = Ipv4Addr::from_str("10.1.2.3")?.into();
        assert!(vnet.has_ipaddr(ip), "the IP addr {} should exist", ip);

        let ip = Ipv4Addr::from_str("192.168.1.1")?.into();
        assert!(!vnet.has_ipaddr(ip), "the IP addr {} should exist", ip);
    }

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_get_all_ipaddrs() -> Result<(), Error> {
    let mut nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let interfaces = nw.get_interfaces();
    assert_eq!(2, interfaces.len(), "should be one interface");

    let ipnet = IpNet::from_str("10.1.2.3/24")?;
    nw.add_addrs_to_interface("eth0", &[ipnet]).await?;

    if let Some(ifc) = nw.get_interface("eth0") {
        let addrs = ifc.addrs();
        assert!(!addrs.is_empty(), "should not empty");
    }

    if let Net::VNet(vnet) = &nw {
        let ips = vnet.get_all_ipaddrs(false);
        assert_eq!(2, ips.len(), "ips should match size {} == 2", ips.len())
    }

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_assign_port() -> Result<(), Error> {
    let mut nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let addr = DEMO_IP;
    let start = 1000u16;
    let end = 1002u16;
    let space = end + 1 - start;

    let interfaces = nw.get_interfaces();
    assert_eq!(2, interfaces.len(), "should be one interface");

    let ipnet = IpNet::from_str(&format!("{}/24", addr))?;
    nw.add_addrs_to_interface("eth0", &[ipnet]).await?;

    if let Net::VNet(vnet) = &mut nw {
        // attempt to assign port with start > end should fail
        let ip = IpAddr::from_str(addr)?;
        let result = vnet.assign_port(ip, 3000, 2999).await;
        assert!(result.is_err(), "assign_port should fail");

        for i in 0..space {
            let port = vnet.assign_port(ip, start, end).await?;
            log::debug!("{} got port: {}", i, port);

            let obs: Arc<Mutex<dyn ConnObserver + Send + Sync>> =
                Arc::new(Mutex::new(DummyObserver::default()));

            let conn = Arc::new(UDPConn::new(SocketAddr::new(ip, port), None, obs));

            let vi = vnet.vi.lock().await;
            let _ = vi.udp_conns.insert(conn).await;
        }

        {
            let vi = vnet.vi.lock().await;
            assert_eq!(
                space as usize,
                vi.udp_conns.len().await,
                "udp_conns should match"
            );
        }

        // attempt to assign again should fail
        let result = vnet.assign_port(ip, start, end).await;
        assert!(result.is_err(), "assign_port should fail");
    }

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_determine_source_ip() -> Result<(), Error> {
    let mut nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let interfaces = nw.get_interfaces();
    assert_eq!(2, interfaces.len(), "should be one interface");

    let ipnet = IpNet::from_str(&format!("{}/24", DEMO_IP))?;
    nw.add_addrs_to_interface("eth0", &[ipnet]).await?;

    // Any IP turned into non-loopback IP
    let any_ip = IpAddr::from_str("0.0.0.0")?;
    let dst_ip = IpAddr::from_str("27.1.7.135")?;
    if let Net::VNet(vnet) = &mut nw {
        let vi = vnet.vi.lock().await;
        let src_ip = vi.determine_source_ip(any_ip, dst_ip);
        log::debug!("any_ip: {} => {:?}", any_ip, src_ip);
        assert!(src_ip.is_some(), "shouldn't be none");
        if let Some(src_ip) = src_ip {
            assert_eq!(src_ip.to_string().as_str(), DEMO_IP, "use non-loopback IP");
        }
    }

    // Any IP turned into loopback IP
    let any_ip = IpAddr::from_str("0.0.0.0")?;
    let dst_ip = IpAddr::from_str("127.0.0.2")?;
    if let Net::VNet(vnet) = &mut nw {
        let vi = vnet.vi.lock().await;
        let src_ip = vi.determine_source_ip(any_ip, dst_ip);
        log::debug!("any_ip: {} => {:?}", any_ip, src_ip);
        assert!(src_ip.is_some(), "shouldn't be none");
        if let Some(src_ip) = src_ip {
            assert_eq!(src_ip.to_string().as_str(), "127.0.0.1", "use loopback IP");
        }
    }

    // Non any IP won't change
    let any_ip = IpAddr::from_str(DEMO_IP)?;
    let dst_ip = IpAddr::from_str("127.0.0.2")?;
    if let Net::VNet(vnet) = &mut nw {
        let vi = vnet.vi.lock().await;
        let src_ip = vi.determine_source_ip(any_ip, dst_ip);
        log::debug!("any_ip: {} => {:?}", any_ip, src_ip);
        assert!(src_ip.is_some(), "shouldn't be none");
        if let Some(src_ip) = src_ip {
            assert_eq!(src_ip, any_ip, "IP change");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_resolve_addr() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let udp_addr = nw.resolve_addr(true, "localhost:1234").await?;
    assert_eq!(
        "127.0.0.1",
        udp_addr.ip().to_string().as_str(),
        "udp addr {} should match 127.0.0.1",
        udp_addr.ip(),
    );
    assert_eq!(
        1234,
        udp_addr.port(),
        "udp addr {} should match 1234",
        udp_addr.port()
    );

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_loopback() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let conn = nw.bind(SocketAddr::from_str("127.0.0.1:0")?).await?;
    let laddr = conn.local_addr()?;

    let msg = "PING!";
    let n = conn.send_to(msg.as_bytes(), laddr).await?;
    assert_eq!(n, msg.len(), "should match msg size {}", msg.len());

    let mut buf = vec![0u8; 1000];
    let (n, raddr) = conn.recv_from(&mut buf).await?;
    assert_eq!(n, msg.len(), "should match msg size {}", msg.len());
    assert_eq!(
        msg.as_bytes(),
        &buf[..n],
        "should match msg content {}",
        msg
    );
    assert_eq!(laddr, raddr, "should match addr {}", laddr);

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_bind_specific_port() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let conn = nw.bind(SocketAddr::from_str("127.0.0.1:50916")?).await?;
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string().as_str(),
        "127.0.0.1",
        "{} should match 127.0.0.1",
        laddr.ip()
    );
    assert_eq!(laddr.port(), 50916, "{} should match 50916", laddr.port());

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_connect_lo0() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let conn = nw.bind(SocketAddr::from_str("127.0.0.1:1234")?).await?;
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string().as_str(),
        "127.0.0.1",
        "{} should match 127.0.0.1",
        laddr.ip()
    );
    assert_eq!(laddr.port(), 1234, "{} should match 1234", laddr.port());

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_connect_eth0() -> Result<(), Error> {
    let wan = Arc::new(Mutex::new(Router::new(RouterConfig {
        cidr: "1.2.3.0/24".to_string(),
        ..Default::default()
    })?));

    let nw = Arc::new(Mutex::new(Net::new(Some(NetConfig::default()))));

    {
        let n = Arc::clone(&nw) as Arc<Mutex<dyn NIC + Send + Sync>>;
        let mut w = wan.lock().await;
        w.add_net(n).await?;
    }
    {
        let n = nw.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }

    let (conn, raddr) = {
        let n = nw.lock().await;
        let raddr = SocketAddr::from_str("27.3.4.5:1234")?;
        let laddr = if let Net::VNet(vnet) = &*n {
            let vi = vnet.vi.lock().await;
            let any_ip = IpAddr::from_str("0.0.0.0")?;
            vi.determine_source_ip(any_ip, raddr.ip()).unwrap()
        } else {
            IpAddr::from_str("0.0.0.0")?
        };

        (n.bind(SocketAddr::new(laddr, 0)).await?, raddr)
    };

    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string().as_str(),
        "1.2.3.1",
        "{} should match 1.2.3.1",
        laddr.ip()
    );
    conn.connect(raddr).await?;

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_resolver() -> Result<(), Error> {
    let wan = Arc::new(Mutex::new(Router::new(RouterConfig {
        cidr: "1.2.3.0/24".to_string(),
        ..Default::default()
    })?));

    let nw = Arc::new(Mutex::new(Net::new(Some(NetConfig::default()))));

    {
        let n = Arc::clone(&nw) as Arc<Mutex<dyn NIC + Send + Sync>>;
        let mut w = wan.lock().await;
        w.add_net(n).await?;
        w.add_host("test.webrtc.rs".to_owned(), "30.31.32.33".to_owned())
            .await?;
    }
    {
        let n = nw.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }

    let (conn, raddr) = {
        let n = nw.lock().await;
        let raddr = n.resolve_addr(true, "test.webrtc.rs:1234").await?;
        let laddr = if let Net::VNet(vnet) = &*n {
            let vi = vnet.vi.lock().await;
            let any_ip = IpAddr::from_str("0.0.0.0")?;
            vi.determine_source_ip(any_ip, raddr.ip()).unwrap()
        } else {
            IpAddr::from_str("0.0.0.0")?
        };

        (n.bind(SocketAddr::new(laddr, 0)).await?, raddr)
    };
    conn.connect(raddr).await?;

    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string().as_str(),
        "1.2.3.1",
        "{} should match  1.2.3.1",
        laddr.ip()
    );

    assert_eq!(
        raddr.to_string(),
        "30.31.32.33:1234",
        "{} should match 30.31.32.33:1234",
        raddr
    );

    Ok(())
}
