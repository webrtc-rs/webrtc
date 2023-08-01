use std::str::FromStr;

use stun::attributes::ATTR_USERNAME;
use stun::textattrs::TextAttribute;
use tokio::net::UdpSocket;

use super::*;
use crate::proto::lifetime::DEFAULT_LIFETIME;

#[tokio::test]
async fn test_has_permission() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr1 = SocketAddr::from_str("127.0.0.1:3478")?;
    let addr2 = SocketAddr::from_str("127.0.0.1:3479")?;
    let addr3 = SocketAddr::from_str("127.0.0.2:3478")?;

    let p1 = Permission::new(addr1);
    let p2 = Permission::new(addr2);
    let p3 = Permission::new(addr3);

    a.add_permission(p1).await;
    a.add_permission(p2).await;
    a.add_permission(p3).await;

    let found_p1 = a.has_permission(&addr1).await;
    assert!(found_p1, "Should keep the first one.");

    let found_p2 = a.has_permission(&addr2).await;
    assert!(found_p2, "Second one should be ignored.");

    let found_p3 = a.has_permission(&addr3).await;
    assert!(found_p3, "Permission with another IP should be found");

    Ok(())
}

#[tokio::test]
async fn test_add_permission() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let p = Permission::new(addr);
    a.add_permission(p).await;

    let found_p = a.has_permission(&addr).await;
    assert!(found_p, "Should keep the first one.");

    Ok(())
}

#[tokio::test]
async fn test_remove_permission() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;

    let p = Permission::new(addr);
    a.add_permission(p).await;

    let found_p = a.has_permission(&addr).await;
    assert!(found_p, "Should keep the first one.");

    a.remove_permission(&addr).await;

    let found_permission = a.has_permission(&addr).await;
    assert!(
        !found_permission,
        "Got permission should be nil after removed."
    );

    Ok(())
}

#[tokio::test]
async fn test_add_channel_bind() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let c = ChannelBind::new(ChannelNumber(MIN_CHANNEL_NUMBER), addr);

    a.add_channel_bind(c, DEFAULT_LIFETIME).await?;

    let c2 = ChannelBind::new(ChannelNumber(MIN_CHANNEL_NUMBER + 1), addr);
    let result = a.add_channel_bind(c2, DEFAULT_LIFETIME).await;
    assert!(
        result.is_err(),
        "should failed with conflicted peer address"
    );

    let addr2 = SocketAddr::from_str("127.0.0.1:3479")?;
    let c3 = ChannelBind::new(ChannelNumber(MIN_CHANNEL_NUMBER), addr2);
    let result = a.add_channel_bind(c3, DEFAULT_LIFETIME).await;
    assert!(result.is_err(), "should fail with conflicted number.");

    Ok(())
}

#[tokio::test]
async fn test_get_channel_by_number() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let c = ChannelBind::new(ChannelNumber(MIN_CHANNEL_NUMBER), addr);

    a.add_channel_bind(c, DEFAULT_LIFETIME).await?;

    let exist_channel_addr = a
        .get_channel_addr(&ChannelNumber(MIN_CHANNEL_NUMBER))
        .await
        .unwrap();
    assert_eq!(addr, exist_channel_addr);

    let not_exist_channel = a
        .get_channel_addr(&ChannelNumber(MIN_CHANNEL_NUMBER + 1))
        .await;
    assert!(
        not_exist_channel.is_none(),
        "should be nil for not existed channel."
    );

    Ok(())
}

#[tokio::test]
async fn test_get_channel_by_addr() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let addr2 = SocketAddr::from_str("127.0.0.1:3479")?;
    let c = ChannelBind::new(ChannelNumber(MIN_CHANNEL_NUMBER), addr);

    a.add_channel_bind(c, DEFAULT_LIFETIME).await?;

    let exist_channel_number = a.get_channel_number(&addr).await.unwrap();
    assert_eq!(ChannelNumber(MIN_CHANNEL_NUMBER), exist_channel_number);

    let not_exist_channel = a.get_channel_number(&addr2).await;
    assert!(
        not_exist_channel.is_none(),
        "should be nil for not existed channel."
    );

    Ok(())
}

#[tokio::test]
async fn test_remove_channel_bind() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let number = ChannelNumber(MIN_CHANNEL_NUMBER);
    let c = ChannelBind::new(number, addr);

    a.add_channel_bind(c, DEFAULT_LIFETIME).await?;

    a.remove_channel_bind(number).await;

    let not_exist_channel = a.get_channel_addr(&number).await;
    assert!(
        not_exist_channel.is_none(),
        "should be nil for not existed channel."
    );

    let not_exist_channel = a.get_channel_number(&addr).await;
    assert!(
        not_exist_channel.is_none(),
        "should be nil for not existed channel."
    );

    Ok(())
}

#[tokio::test]
async fn test_allocation_refresh() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    a.start(DEFAULT_LIFETIME).await;
    a.refresh(Duration::from_secs(0)).await;

    assert!(!a.stop(), "lifetimeTimer has expired");

    Ok(())
}

#[tokio::test]
async fn test_allocation_close() -> Result<()> {
    let turn_socket = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let relay_socket = Arc::clone(&turn_socket);
    let relay_addr = relay_socket.local_addr()?;
    let a = Allocation::new(
        turn_socket,
        relay_socket,
        relay_addr,
        FiveTuple::default(),
        TextAttribute::new(ATTR_USERNAME, "user".into()),
        None,
    );

    // add mock lifetimeTimer
    a.start(DEFAULT_LIFETIME).await;

    // add channel
    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let number = ChannelNumber(MIN_CHANNEL_NUMBER);
    let c = ChannelBind::new(number, addr);

    a.add_channel_bind(c, DEFAULT_LIFETIME).await?;

    // add permission
    a.add_permission(Permission::new(addr)).await;

    a.close().await?;

    Ok(())
}
