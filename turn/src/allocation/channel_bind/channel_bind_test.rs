use std::net::Ipv4Addr;

use stun::attributes::ATTR_USERNAME;
use stun::textattrs::TextAttribute;
use tokio::net::UdpSocket;

use super::*;
use crate::allocation::*;
use crate::error::Result;

async fn create_channel_bind(lifetime: Duration) -> Result<Allocation> {
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

    let addr = SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 0);
    let c = ChannelBind::new(ChannelNumber(MIN_CHANNEL_NUMBER), addr);

    a.add_channel_bind(c, lifetime).await?;

    Ok(a)
}

#[tokio::test]
async fn test_channel_bind() -> Result<()> {
    let a = create_channel_bind(Duration::from_millis(20)).await?;

    let result = a.get_channel_addr(&ChannelNumber(MIN_CHANNEL_NUMBER)).await;
    if let Some(addr) = result {
        assert_eq!(addr.ip().to_string(), "0.0.0.0");
    } else {
        panic!("expected some, but got none");
    }

    Ok(())
}

async fn test_channel_bind_start() -> Result<()> {
    let a = create_channel_bind(Duration::from_millis(20)).await?;
    tokio::time::sleep(Duration::from_millis(30)).await;

    assert!(a
        .get_channel_addr(&ChannelNumber(MIN_CHANNEL_NUMBER))
        .await
        .is_none());

    Ok(())
}

async fn test_channel_bind_reset() -> Result<()> {
    let a = create_channel_bind(Duration::from_millis(30)).await?;

    tokio::time::sleep(Duration::from_millis(20)).await;
    {
        let channel_bindings = a.channel_bindings.lock().await;
        if let Some(c) = channel_bindings.get(&ChannelNumber(MIN_CHANNEL_NUMBER)) {
            c.refresh(Duration::from_millis(30)).await;
        }
    }
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert!(a
        .get_channel_addr(&ChannelNumber(MIN_CHANNEL_NUMBER))
        .await
        .is_some());

    Ok(())
}
