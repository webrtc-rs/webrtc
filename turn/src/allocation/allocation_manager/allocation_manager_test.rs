use super::*;

use crate::{
    client::{Client, ClientConfig},
    error::Result,
    proto::lifetime::DEFAULT_LIFETIME,
    relay::relay_none::*,
    server::server_test::build_vnet,
};

use std::{net::Ipv4Addr, str::FromStr};
use stun::{attributes::ATTR_USERNAME, textattrs::TextAttribute};
use tokio::net::UdpSocket;
use util::vnet::net::*;

fn new_test_manager(gather_metrics: bool) -> Manager {
    let config = ManagerConfig {
        relay_addr_generator: Box::new(RelayAddressGeneratorNone {
            address: "0.0.0.0".to_owned(),
            net: Arc::new(Net::new(None)),
        }),
        gather_metrics,
    };
    Manager::new(config)
}

fn random_five_tuple() -> FiveTuple {
    /* #nosec */
    FiveTuple {
        src_addr: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), rand::random()),
        dst_addr: SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), rand::random()),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_packet_handler() -> Result<()> {
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

    let m = new_test_manager(false);
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
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;

    let peer_listener1 = UdpSocket::bind("127.0.0.1:0").await?;
    let peer_listener2 = UdpSocket::bind("127.0.0.1:0").await?;

    let channel_bind = ChannelBind::new(
        ChannelNumber(MIN_CHANNEL_NUMBER),
        peer_listener2.local_addr()?,
    );

    let port = {
        // add permission with peer1 address
        a.add_permission(Permission::new(peer_listener1.local_addr()?))
            .await;
        // add channel with min channel number and peer2 address
        a.add_channel_bind(channel_bind.clone(), DEFAULT_LIFETIME)
            .await?;

        a.relay_socket.local_addr().await?.port()
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
        .ok_or(Error::Other("data ch closed".to_owned()))?;

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
        .ok_or(Error::Other("data ch closed".to_owned()))?;

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

#[tokio::test]
async fn test_create_allocation_duplicate_five_tuple() -> Result<()> {
    //env_logger::init();

    // turn server initialization
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager(false);

    let five_tuple = random_five_tuple();

    let _ = m
        .create_allocation(
            five_tuple.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;

    let result = m
        .create_allocation(
            five_tuple,
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await;
    assert!(result.is_err(), "expected error, but got ok");

    Ok(())
}

#[tokio::test]
async fn test_delete_allocation() -> Result<()> {
    //env_logger::init();

    // turn server initialization
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager(false);

    let five_tuple = random_five_tuple();

    let _ = m
        .create_allocation(
            five_tuple.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;

    assert!(
        m.get_allocation(&five_tuple).await.is_some(),
        "Failed to get allocation right after creation"
    );

    m.delete_allocation(&five_tuple).await;

    assert!(
        m.get_allocation(&five_tuple).await.is_none(),
        "Get allocation with {} should be nil after delete",
        five_tuple
    );

    Ok(())
}

#[tokio::test]
async fn test_allocation_timeout() -> Result<()> {
    //env_logger::init();

    // turn server initialization
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager(false);

    let mut allocations = vec![];
    let lifetime = Duration::from_millis(100);

    for _ in 0..5 {
        let five_tuple = random_five_tuple();

        let a = m
            .create_allocation(
                five_tuple,
                Arc::clone(&turn_socket),
                0,
                lifetime,
                TextAttribute::new(ATTR_USERNAME, "user".into()),
            )
            .await?;

        allocations.push(a);
    }

    let mut count = 0;

    'outer: loop {
        count += 1;

        if count >= 10 {
            assert!(false, "Allocations didn't timeout");
        }

        tokio::time::sleep(lifetime + Duration::from_millis(100)).await;

        let any_outstanding = false;

        for a in &allocations {
            if a.close().await.is_ok() {
                continue 'outer;
            }
        }

        if !any_outstanding {
            return Ok(());
        }
    }
}

#[tokio::test]
async fn test_manager_close() -> Result<()> {
    // env_logger::init();

    // turn server initialization
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager(false);

    let mut allocations = vec![];

    let a1 = m
        .create_allocation(
            random_five_tuple(),
            Arc::clone(&turn_socket),
            0,
            Duration::from_millis(100),
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;
    allocations.push(a1);

    let a2 = m
        .create_allocation(
            random_five_tuple(),
            Arc::clone(&turn_socket),
            0,
            Duration::from_millis(200),
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;
    allocations.push(a2);

    tokio::time::sleep(Duration::from_millis(150)).await;

    log::trace!("Mgr is going to be closed...");

    m.close().await?;

    for a in allocations {
        assert!(
            a.close().await.is_err(),
            "Allocation should be closed if lifetime timeout"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_delete_allocation_by_username() -> Result<()> {
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager(false);

    let five_tuple1 = random_five_tuple();
    let five_tuple2 = random_five_tuple();
    let five_tuple3 = random_five_tuple();

    let _ = m
        .create_allocation(
            five_tuple1.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;
    let _ = m
        .create_allocation(
            five_tuple2.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;
    let _ = m
        .create_allocation(
            five_tuple3.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user2".into()),
        )
        .await?;

    assert_eq!(m.allocations.lock().await.len(), 3);

    m.delete_allocations_by_username("user").await;

    assert_eq!(m.allocations.lock().await.len(), 1);

    assert!(
        m.get_allocation(&five_tuple1).await.is_none()
            && m.get_allocation(&five_tuple2).await.is_none()
            && m.get_allocation(&five_tuple3).await.is_some()
    );

    Ok(())
}

#[tokio::test]
async fn test_get_allocations_info() -> Result<()> {
    // TODO: maybe add integration test?
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager(true);

    let five_tuple1 = random_five_tuple();
    let five_tuple2 = random_five_tuple();
    let five_tuple3 = random_five_tuple();

    let alloc1 = m
        .create_allocation(
            five_tuple1.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user1".into()),
        )
        .await?;
    alloc1.relayed_bytes.store(111, Ordering::SeqCst);

    let alloc2 = m
        .create_allocation(
            five_tuple2.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user2".into()),
        )
        .await?;
    alloc2.relayed_bytes.store(222, Ordering::SeqCst);

    let alloc3 = m
        .create_allocation(
            five_tuple3.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user3".into()),
        )
        .await?;
    alloc3.relayed_bytes.store(333, Ordering::SeqCst);

    let infos = m
        .get_allocations_info(Some(Vec::from([
            five_tuple1.clone(),
            five_tuple2.clone(),
            five_tuple3.clone(),
        ])))
        .await;

    assert_eq!(infos.len(), 3);

    assert!(
        infos.get(&five_tuple1).is_some()
            && infos.get(&five_tuple2).is_some()
            && infos.get(&five_tuple3).is_some()
    );

    assert_eq!(infos.get(&five_tuple1).unwrap().username, "user1");
    assert_eq!(infos.get(&five_tuple2).unwrap().username, "user2");
    assert_eq!(infos.get(&five_tuple3).unwrap().username, "user3");

    assert_eq!(infos.get(&five_tuple1).unwrap().five_tuple, five_tuple1);
    assert_eq!(infos.get(&five_tuple2).unwrap().five_tuple, five_tuple2);
    assert_eq!(infos.get(&five_tuple3).unwrap().five_tuple, five_tuple3);

    assert_eq!(
        infos.get(&five_tuple1).unwrap().transmitted_bytes,
        Some(111)
    );
    assert_eq!(
        infos.get(&five_tuple2).unwrap().transmitted_bytes,
        Some(222)
    );
    assert_eq!(
        infos.get(&five_tuple3).unwrap().transmitted_bytes,
        Some(333)
    );

    let m2 = new_test_manager(false);

    let five_tuple222 = random_five_tuple();

    let _ = m2
        .create_allocation(
            five_tuple222.clone(),
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user222".into()),
        )
        .await?;

    let infos2 = m2
        .get_allocations_info(Some(Vec::from([five_tuple222.clone()])))
        .await;

    assert!(infos2.get(&five_tuple222).is_some());

    assert!(infos2
        .get(&five_tuple222)
        .unwrap()
        .transmitted_bytes
        .is_none());

    Ok(())
}

#[tokio::test]
async fn test_test_get_allocations_info_with_server() -> Result<()> {
    let v = build_vnet().await?;

    assert!(v.server.get_allocations_info(None).await?.is_empty());

    let lconn = v.netl0.bind(SocketAddr::from_str("0.0.0.0:0")?).await?;
    let client = Client::new(ClientConfig {
        stun_serv_addr: "stun.webrtc.rs:3478".to_owned(),
        turn_serv_addr: "turn.webrtc.rs:3478".to_owned(),
        username: "user".to_owned(),
        password: "pass".to_owned(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn: lconn,
        vnet: Some(Arc::clone(&v.netl0)),
    })
    .await?;

    client.listen().await?;

    let conn = client.allocate().await?;

    assert!(!v.server.get_allocations_info(None).await?.is_empty());

    let echo_conn = v.net1.bind(SocketAddr::from_str("1.2.3.5:5678")?).await?;
    let echo_addr = echo_conn.local_addr().await?;

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            tokio::select! {
                _ = done_rx.recv() => break,
                _ = echo_conn.recv_from(&mut buf) => {
                }
            }
        }
    });

    assert_eq!(
        v.server
            .get_allocations_info(None)
            .await?
            .values()
            .last()
            .unwrap()
            .transmitted_bytes,
        Some(0)
    );

    for _ in 0..10 {
        conn.send_to(b"Hello", echo_addr).await?;

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    assert_eq!(
        v.server
            .get_allocations_info(None)
            .await?
            .values()
            .last()
            .unwrap()
            .transmitted_bytes,
        Some(50)
    );

    for _ in 0..10 {
        conn.send_to(b"Hello", echo_addr).await?;

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    assert_eq!(
        v.server
            .get_allocations_info(None)
            .await?
            .values()
            .last()
            .unwrap()
            .transmitted_bytes,
        Some(100)
    );

    client.close().await?;
    drop(done_tx);

    Ok(())
}
