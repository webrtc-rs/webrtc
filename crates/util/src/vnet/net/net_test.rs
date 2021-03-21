use super::*;

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
