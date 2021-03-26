use super::*;
use crate::vnet::chunk::ChunkUDP;

use tokio::sync::{broadcast, mpsc};

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

    let conn = nw.bind(SocketAddr::from_str("127.0.0.1:0")?).await?;
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string(),
        "127.0.0.1",
        "local_addr ip should match 127.0.0.1"
    );
    log::debug!("laddr: {}", laddr);

    Ok(())
}

#[tokio::test]
async fn test_net_native_dail() -> Result<(), Error> {
    let nw = Net::new(None);
    assert!(!nw.is_virtual(), "should be false");

    let conn = nw.dail(true, "127.0.0.1:1234").await?;
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string(),
        "127.0.0.1",
        "local_addr should match 127.0.0.1"
    );
    assert_ne!(laddr.port(), 1234, "local_addr port should match 1234");
    log::debug!("laddr: {}", laddr);

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
async fn test_net_virtual_loopback1() -> Result<(), Error> {
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
async fn test_net_virtual_dail_lo0() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));
    assert!(nw.is_virtual(), "should be true");

    let conn = nw.dail(true, "127.0.0.1:1234").await?;
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string().as_str(),
        "127.0.0.1",
        "{} should match 127.0.0.1",
        laddr.ip()
    );
    assert_ne!(laddr.port(), 1234, "{} should != 1234", laddr.port());

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_dail_eth0() -> Result<(), Error> {
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

    let conn = {
        let n = nw.lock().await;
        n.dail(true, "27.3.4.5:1234").await?
    };
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.ip().to_string().as_str(),
        "1.2.3.1",
        "{} should match 1.2.3.1",
        laddr.ip()
    );
    assert!(laddr.port() != 0, "{} should != 0", laddr.port());

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

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
    tokio::spawn(async move {
        let (conn, raddr) = {
            let n = nw.lock().await;
            let raddr = n.resolve_addr(true, "test.webrtc.rs:1234").await?;
            (n.dail(true, "test.webrtc.rs:1234").await?, raddr)
        };

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

        drop(done_tx);

        Ok::<(), Error>(())
    });

    let _ = done_rx.recv().await;

    Ok(())
}

#[tokio::test]
async fn test_net_virtual_loopback2() -> Result<(), Error> {
    let nw = Net::new(Some(NetConfig::default()));

    let conn = nw.bind(SocketAddr::from_str("127.0.0.1:50916")?).await?;
    let laddr = conn.local_addr()?;
    assert_eq!(
        laddr.to_string().as_str(),
        "127.0.0.1:50916",
        "{} should match 127.0.0.1:50916",
        laddr
    );

    let mut c = ChunkUDP::new(
        SocketAddr::from_str("127.0.0.1:4000")?,
        SocketAddr::from_str("127.0.0.1:50916")?,
    );
    c.user_data = b"Hello!".to_vec();

    let (recv_ch_tx, mut recv_ch_rx) = mpsc::channel(1);
    let (done_ch_tx, mut done_ch_rx) = mpsc::channel::<bool>(1);
    let (close_ch_tx, mut close_ch_rx) = mpsc::channel::<bool>(1);
    let conn_rx = Arc::clone(&conn);

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            tokio::select! {
                result = conn_rx.recv_from(&mut buf) => {
                    let (n, addr) = match result {
                        Ok((n, addr)) => (n, addr),
                        Err(err) => {
                            log::debug!("ReadFrom returned: {}", err);
                            break;
                        }
                    };

                    assert_eq!(6, n, "{} should match 6", n);
                    assert_eq!("127.0.0.1:4000", addr.to_string(), "addr should match");
                    assert_eq!(b"Hello!", &buf[..n], "buf should match");

                    let _ = recv_ch_tx.send(true).await;
                }
                _ = close_ch_rx.recv() => {
                    break;
                }
            }
        }

        drop(done_ch_tx);
    });

    if let Net::VNet(vnet) = &nw {
        vnet.on_inbound_chunk(Box::new(c)).await;
    } else {
        assert!(false, "must be virtual net");
    }

    let _ = recv_ch_rx.recv().await;
    drop(close_ch_tx);

    let _ = done_ch_rx.recv().await;

    Ok(())
}

async fn get_ipaddr(nic: &Arc<Mutex<dyn NIC + Send + Sync>>) -> Result<IpAddr, Error> {
    let n = nic.lock().await;
    let eth0 = n
        .get_interface("eth0")
        .ok_or_else(|| ERR_NO_INTERFACE.to_owned())?;
    let addrs = eth0.addrs();
    if addrs.is_empty() {
        Err(ERR_NO_ADDRESS_ASSIGNED.to_owned())
    } else {
        Ok(addrs[0].addr())
    }
}

//use std::io::Write;

#[tokio::test]
async fn test_net_virtual_end2end() -> Result<(), Error> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, log::LevelFilter::Trace)
    .init();*/

    let wan = Arc::new(Mutex::new(Router::new(RouterConfig {
        cidr: "1.2.3.0/24".to_string(),
        ..Default::default()
    })?));

    let net1 = Arc::new(Mutex::new(Net::new(Some(NetConfig::default()))));
    {
        let n = Arc::clone(&net1) as Arc<Mutex<dyn NIC + Send + Sync>>;
        let mut w = wan.lock().await;
        w.add_net(n).await?;
    }
    {
        let n = net1.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }
    let ip1 = {
        let nic = Arc::clone(&net1) as Arc<Mutex<dyn NIC + Send + Sync>>;
        get_ipaddr(&nic).await?
    };

    let net2 = Arc::new(Mutex::new(Net::new(Some(NetConfig::default()))));
    {
        let n = Arc::clone(&net2) as Arc<Mutex<dyn NIC + Send + Sync>>;
        let mut w = wan.lock().await;
        w.add_net(n).await?;
    }
    {
        let n = net2.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }
    let ip2 = {
        let nic = Arc::clone(&net2) as Arc<Mutex<dyn NIC + Send + Sync>>;
        get_ipaddr(&nic).await?
    };

    let conn1 = {
        let n = net1.lock().await;
        n.bind(SocketAddr::new(ip1, 1234)).await?
    };

    let conn2 = {
        let n = net2.lock().await;
        n.bind(SocketAddr::new(ip2, 5678)).await?
    };

    {
        let mut w = wan.lock().await;
        w.start().await?;
    }

    let (close_ch_tx, mut close_ch_rx1) = broadcast::channel::<bool>(1);
    let (done_ch_tx, mut done_ch_rx) = mpsc::channel::<bool>(1);
    let (conn1_recv_ch_tx, mut conn1_recv_ch_rx) = mpsc::channel(1);
    let conn1_rx = Arc::clone(&conn1);
    let conn2_tr = Arc::clone(&conn2);
    let mut close_ch_rx2 = close_ch_tx.subscribe();

    // conn1
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            log::debug!("conn1: wait for a message..");
            tokio::select! {
                result = conn1_rx.recv_from(&mut buf) =>{
                    let n = match result{
                        Ok((n, _)) => n,
                        Err(err) => {
                            log::debug!("ReadFrom returned: {}", err);
                            break;
                        }
                    };

                    log::debug!("conn1 received {:?}", &buf[..n]);
                    let _ = conn1_recv_ch_tx.send(true).await;
                }
                _ = close_ch_rx1.recv() => {
                    log::debug!("conn1 received close_ch_rx1");
                    break;
                }
            }
        }
        drop(done_ch_tx);
        log::debug!("conn1 drop done_ch_tx, exit spawn");
    });

    // conn2
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            log::debug!("conn2: wait for a message..");
            tokio::select! {
                result = conn2_tr.recv_from(&mut buf) =>{
                    let (n, addr) = match result{
                        Ok((n, addr)) => (n, addr),
                        Err(err) => {
                            log::debug!("ReadFrom returned: {}", err);
                            break;
                        }
                    };

                    log::debug!("conn2 received {:?}", &buf[..n]);

                    // echo back to conn1
                    let n = conn2_tr.send_to(b"Good-bye!", addr).await?;
                    assert_eq!( 9, n, "should match");
                }
                _ = close_ch_rx2.recv() => {
                    log::debug!("conn1 received close_ch_rx2");
                    break;
                }
            }
        }

        log::debug!("conn2 exit spawn");

        Ok::<(), Error>(())
    });

    log::debug!("conn1: sending");
    let n = conn1.send_to(b"Hello!", conn2.local_addr()?).await?;
    assert_eq!(6, n, "should match");

    let _ = conn1_recv_ch_rx.recv().await;
    log::debug!("main recv conn1_recv_ch_rx");
    drop(close_ch_tx);
    log::debug!("main drop close_ch_tx");
    let _ = done_ch_rx.recv().await;
    log::debug!("main recv done_ch_rx");
    Ok(())
}

//use std::io::Write;

#[tokio::test]
async fn test_net_virtual_two_ips_on_a_nic() -> Result<(), Error> {
    /*env_logger::Builder::new()
    .format(|buf, record| {
        writeln!(
            buf,
            "{}:{} [{}] {} - {}",
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            chrono::Local::now().format("%H:%M:%S.%6f"),
            record.args()
        )
    })
    .filter(None, log::LevelFilter::Trace)
    .init();*/

    let wan = Arc::new(Mutex::new(Router::new(RouterConfig {
        cidr: "1.2.3.0/24".to_string(),
        ..Default::default()
    })?));

    let net = Arc::new(Mutex::new(Net::new(Some(NetConfig {
        static_ips: vec![DEMO_IP.to_owned(), "1.2.3.5".to_owned()],
        ..Default::default()
    }))));

    {
        let n = Arc::clone(&net) as Arc<Mutex<dyn NIC + Send + Sync>>;
        let mut w = wan.lock().await;
        w.add_net(n).await?;
    }
    {
        let n = net.lock().await;
        n.set_router(Arc::clone(&wan)).await?;
    }

    // start the router
    {
        let mut w = wan.lock().await;
        w.start().await?;
    }

    let (conn1, conn2) = {
        let n = net.lock().await;
        (
            n.bind(SocketAddr::new(Ipv4Addr::from_str(DEMO_IP)?.into(), 1234))
                .await?,
            n.bind(SocketAddr::new(Ipv4Addr::from_str("1.2.3.5")?.into(), 1234))
                .await?,
        )
    };

    let (close_ch_tx, mut close_ch_rx1) = broadcast::channel::<bool>(1);
    let (done_ch_tx, mut done_ch_rx) = mpsc::channel::<bool>(1);
    let (conn1_recv_ch_tx, mut conn1_recv_ch_rx) = mpsc::channel(1);
    let conn1_rx = Arc::clone(&conn1);
    let conn2_tr = Arc::clone(&conn2);
    let mut close_ch_rx2 = close_ch_tx.subscribe();

    // conn1
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            log::debug!("conn1: wait for a message..");
            tokio::select! {
                result = conn1_rx.recv_from(&mut buf) =>{
                    let n = match result{
                        Ok((n, _)) => n,
                        Err(err) => {
                            log::debug!("ReadFrom returned: {}", err);
                            break;
                        }
                    };

                    log::debug!("conn1 received {:?}", &buf[..n]);
                    let _ = conn1_recv_ch_tx.send(true).await;
                }
                _ = close_ch_rx1.recv() => {
                    log::debug!("conn1 received close_ch_rx1");
                    break;
                }
            }
        }
        drop(done_ch_tx);
        log::debug!("conn1 drop done_ch_tx, exit spawn");
    });

    // conn2
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            log::debug!("conn2: wait for a message..");
            tokio::select! {
                result = conn2_tr.recv_from(&mut buf) =>{
                    let (n, addr) = match result{
                        Ok((n, addr)) => (n, addr),
                        Err(err) => {
                            log::debug!("ReadFrom returned: {}", err);
                            break;
                        }
                    };

                    log::debug!("conn2 received {:?}", &buf[..n]);

                    // echo back to conn1
                    let n = conn2_tr.send_to(b"Good-bye!", addr).await?;
                    assert_eq!( 9, n, "should match");
                }
                _ = close_ch_rx2.recv() => {
                    log::debug!("conn1 received close_ch_rx2");
                    break;
                }
            }
        }

        log::debug!("conn2 exit spawn");

        Ok::<(), Error>(())
    });

    log::debug!("conn1: sending");
    let n = conn1.send_to(b"Hello!", conn2.local_addr()?).await?;
    assert_eq!(6, n, "should match");

    let _ = conn1_recv_ch_rx.recv().await;
    log::debug!("main recv conn1_recv_ch_rx");
    drop(close_ch_tx);
    log::debug!("main drop close_ch_tx");
    let _ = done_ch_rx.recv().await;
    log::debug!("main recv done_ch_rx");
    Ok(())
}
