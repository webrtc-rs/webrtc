use super::*;
use crate::relay_address_generator::relay_address_generator_none::*;

use crate::proto::lifetime::DEFAULT_LIFETIME;
use std::str::FromStr;
use tokio::net::UdpSocket;
use util::Error;

fn new_test_manager() -> Manager {
    let config = ManagerConfig {
        relay_addr_generator: Box::new(RelayAddressGeneratorNone {
            address: "0.0.0.0".to_owned(),
        }),
    };
    Manager::new(config)
}

#[tokio::test]
async fn test_packet_handler() -> Result<(), Error> {
    //env_logger::init();

    // turn server initialization
    let turn_socket = UdpSocket::bind("127.0.0.1:0").await?;

    // client listener initialization
    let client_listener = UdpSocket::bind("127.0.0.1:0").await?;
    let src_addr = client_listener.local_addr()?;
    let (data_ch_tx, mut data_ch_rx) = mpsc::channel(1);
    // client listener read data
    tokio::spawn(async move {
        let mut buffer = vec![0u8; RTP_MTU];
        loop {
            let n = match client_listener.recv_from(&mut buffer).await {
                Ok((n, _)) => n,
                Err(_) => break,
            };

            let _ = data_ch_tx.send(buffer[..n].to_vec()).await;
        }
    });

    let m = new_test_manager();
    let a = m
        .create_allocation(
            FiveTuple {
                src_addr,
                dst_addr: turn_socket.local_addr()?,
                ..Default::default()
            },
            Arc::new(turn_socket),
            0,
            DEFAULT_LIFETIME,
        )
        .await?;

    let peer_listener1 = UdpSocket::bind("127.0.0.1:0").await?;
    let peer_listener2 = UdpSocket::bind("127.0.0.1:0").await?;

    let channel_bind = ChannelBind::new(
        ChannelNumber(MIN_CHANNEL_NUMBER),
        peer_listener2.local_addr()?,
    );

    let port = {
        let a = a.lock().await;

        // add permission with peer1 address
        a.add_permission(Permission::new(peer_listener1.local_addr()?))
            .await;
        // add channel with min channel number and peer2 address
        a.add_channel_bind(channel_bind.clone(), DEFAULT_LIFETIME)
            .await?;

        a.relay_socket.local_addr()?.port()
    };

    let relay_addr_with_host_str = format!("127.0.0.1:{}", port);
    let relay_addr_with_host = SocketAddr::from_str(&relay_addr_with_host_str)?;

    // test for permission and data message
    let target_text = "permission";
    let _ = peer_listener1
        .send_to(target_text.as_bytes(), relay_addr_with_host)
        .await?;
    let data = data_ch_rx
        .recv()
        .await
        .ok_or_else(|| Error::new("data ch closed".to_owned()))?;

    // resolve stun data message
    assert!(is_message(&data), "should be stun message");

    let mut msg = Message::new();
    msg.raw = data;
    msg.decode()?;

    let mut msg_data = Data::default();
    msg_data.get_from(&msg)?;
    assert_eq!(
        target_text.as_bytes(),
        &msg_data.0,
        "get message doesn't equal the target text"
    );

    // test for channel bind and channel data
    let target_text2 = "channel bind";
    let _ = peer_listener2
        .send_to(target_text2.as_bytes(), relay_addr_with_host)
        .await?;
    let data = data_ch_rx
        .recv()
        .await
        .ok_or_else(|| Error::new("data ch closed".to_owned()))?;

    // resolve channel data
    assert!(
        ChannelData::is_channel_data(&data),
        "should be channel data"
    );

    let mut channel_data = ChannelData {
        raw: data,
        ..Default::default()
    };
    channel_data.decode()?;
    assert_eq!(
        channel_bind.number, channel_data.number,
        "get channel data's number is invalid"
    );
    assert_eq!(
        target_text2.as_bytes(),
        &channel_data.data,
        "get data doesn't equal the target text."
    );

    // listeners close
    m.close().await?;

    Ok(())
}
