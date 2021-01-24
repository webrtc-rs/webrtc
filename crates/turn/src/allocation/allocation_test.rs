use super::*;

use crate::proto::lifetime::DEFAULT_LIFETIME;
use std::str::FromStr;
use tokio::net::UdpSocket;
use util::Error;

#[tokio::test]
async fn test_has_permission() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

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
async fn test_add_permission() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

    let addr = SocketAddr::from_str("127.0.0.1:3478")?;
    let p = Permission::new(addr);
    a.add_permission(p).await;

    let found_p = a.has_permission(&addr).await;
    assert!(found_p, "Should keep the first one.");

    Ok(())
}

#[tokio::test]
async fn test_remove_permission() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

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
async fn test_add_channel_bind() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

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
async fn test_get_channel_by_number() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

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
async fn test_get_channel_by_addr() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

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
async fn test_remove_channel_bind() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let a = Allocation::new(socket, FiveTuple::default());

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
async fn test_allocation_refresh() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let mut a = Allocation::new(socket, FiveTuple::default());

    a.start(DEFAULT_LIFETIME).await;
    a.refresh(Duration::from_secs(0)).await;

    assert!(!a.stop(), "lifetimeTimer has expired");

    Ok(())
}

#[tokio::test]
async fn test_allocation_close() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let mut a = Allocation::new(socket, FiveTuple::default());

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

/*
func subTestPacketHandler(t *testing.T) {
    network := "udp"

    m, _ := newTestManager()

    // turn server initialization
    turnSocket, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    // client listener initialization
    clientListener, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    dataCh := make(chan []byte)
    // client listener read data
    go func() {
        buffer := make([]byte, rtpMTU)
        for {
            n, _, err2 := clientListener.ReadFrom(buffer)
            if err2 != nil {
                return
            }

            dataCh <- buffer[:n]
        }
    }()

    a, err := m.CreateAllocation(&FiveTuple{
        SrcAddr: clientListener.LocalAddr(),
        DstAddr: turnSocket.LocalAddr(),
    }, turnSocket, 0, proto.DefaultLifetime)

    assert.Nil(t, err, "should succeed")

    peerListener1, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    peerListener2, err := net.ListenPacket(network, "127.0.0.1:0")
    if err != nil {
        panic(err)
    }

    // add permission with peer1 address
    a.AddPermission(NewPermission(peerListener1.LocalAddr(), m.log))
    // add channel with min channel number and peer2 address
    channelBind := NewChannelBind(proto.MinChannelNumber, peerListener2.LocalAddr(), m.log)
    _ = a.AddChannelBind(channelBind, proto.DefaultLifetime)

    _, port, _ := ipnet.AddrIPPort(a.RelaySocket.LocalAddr())
    relayAddrWithHostStr := fmt.Sprintf("127.0.0.1:%d", port)
    relayAddrWithHost, _ := net.ResolveUDPAddr(network, relayAddrWithHostStr)

    // test for permission and data message
    targetText := "permission"
    _, _ = peerListener1.WriteTo([]byte(targetText), relayAddrWithHost)
    data := <-dataCh

    // resolve stun data message
    assert.True(t, stun.IsMessage(data), "should be stun message")

    var msg stun.Message
    err = stun.Decode(data, &msg)
    assert.Nil(t, err, "decode data to stun message failed")

    var msgData proto.Data
    err = msgData.GetFrom(&msg)
    assert.Nil(t, err, "get data from stun message failed")
    assert.Equal(t, targetText, string(msgData), "get message doesn't equal the target text")

    // test for channel bind and channel data
    targetText2 := "channel bind"
    _, _ = peerListener2.WriteTo([]byte(targetText2), relayAddrWithHost)
    data = <-dataCh

    // resolve channel data
    assert.True(t, proto.IsChannelData(data), "should be channel data")

    channelData := proto.ChannelData{
        Raw: data,
    }
    err = channelData.Decode()
    assert.Nil(t, err, fmt.Sprintf("channel data decode with error: %v", err))
    assert.Equal(t, channelBind.Number, channelData.Number, "get channel data's number is invalid")
    assert.Equal(t, targetText2, string(channelData.Data), "get data doesn't equal the target text.")

    // listeners close
    _ = m.Close()
    _ = clientListener.Close()
    _ = peerListener1.Close()
    _ = peerListener2.Close()
}
 */
