use super::*;

use crate::{
    auth::{generate_auth_key, AuthHandler},
    client::{Client, ClientConfig},
    error::Result,
    proto::lifetime::DEFAULT_LIFETIME,
    relay::{relay_none::*, relay_static::RelayAddressGeneratorStatic},
    server::{
        config::{ConnConfig, ServerConfig},
        Server,
    },
};

use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
};
use stun::{attributes::ATTR_USERNAME, textattrs::TextAttribute};
use tokio::net::UdpSocket;
use util::vnet::net::*;

fn new_test_manager() -> Manager {
    let config = ManagerConfig {
        relay_addr_generator: Box::new(RelayAddressGeneratorNone {
            address: "0.0.0.0".to_owned(),
            net: Arc::new(Net::new(None)),
        }),
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

        a.relay_socket.local_addr()?.port()
    };

    let relay_addr_with_host_str = format!("127.0.0.1:{port}");
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

    let m = new_test_manager();

    let five_tuple = random_five_tuple();

    let _ = m
        .create_allocation(
            five_tuple,
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

    let m = new_test_manager();

    let five_tuple = random_five_tuple();

    let _ = m
        .create_allocation(
            five_tuple,
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
        "Get allocation with {five_tuple} should be nil after delete"
    );

    Ok(())
}

#[tokio::test]
async fn test_allocation_timeout() -> Result<()> {
    //env_logger::init();

    // turn server initialization
    let turn_socket: Arc<dyn Conn + Send + Sync> = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let m = new_test_manager();

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
            panic!("Allocations didn't timeout");
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

    let m = new_test_manager();

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

    let m = new_test_manager();

    let five_tuple1 = random_five_tuple();
    let five_tuple2 = random_five_tuple();
    let five_tuple3 = random_five_tuple();

    let _ = m
        .create_allocation(
            five_tuple1,
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;
    let _ = m
        .create_allocation(
            five_tuple2,
            Arc::clone(&turn_socket),
            0,
            DEFAULT_LIFETIME,
            TextAttribute::new(ATTR_USERNAME, "user".into()),
        )
        .await?;
    let _ = m
        .create_allocation(
            five_tuple3,
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

struct TestAuthHandler;
impl AuthHandler for TestAuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, _src_addr: SocketAddr) -> Result<Vec<u8>> {
        Ok(generate_auth_key(username, realm, "pass"))
    }
}

async fn create_server() -> Result<(Server, u16)> {
    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let server_port = conn.local_addr()?.port();

    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: IpAddr::from_str("127.0.0.1")?,
                address: "0.0.0.0".to_owned(),
                net: Arc::new(Net::new(None)),
            }),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(TestAuthHandler {}),
        channel_bind_timeout: Duration::from_secs(0),
    })
    .await?;

    Ok((server, server_port))
}

async fn create_client(username: String, server_port: u16) -> Result<Client> {
    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    Client::new(ClientConfig {
        stun_serv_addr: format!("127.0.0.1:{server_port}"),
        turn_serv_addr: format!("127.0.0.1:{server_port}"),
        username,
        password: "pass".to_owned(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn,
        vnet: None,
    })
    .await
}

#[cfg(feature = "metrics")]
#[tokio::test]
async fn test_get_allocations_info() -> Result<()> {
    let (server, server_port) = create_server().await?;

    let client1 = create_client("user1".to_owned(), server_port).await?;
    client1.listen().await?;

    let client2 = create_client("user2".to_owned(), server_port).await?;
    client2.listen().await?;

    let client3 = create_client("user3".to_owned(), server_port).await?;
    client3.listen().await?;

    assert!(server.get_allocations_info(None).await?.is_empty());

    let user1 = client1.allocate().await?;
    let user2 = client2.allocate().await?;
    let user3 = client3.allocate().await?;

    assert_eq!(server.get_allocations_info(None).await?.len(), 3);

    let addr1 = client1
        .send_binding_request_to(format!("127.0.0.1:{server_port}").as_str())
        .await?;
    let addr2 = client2
        .send_binding_request_to(format!("127.0.0.1:{server_port}").as_str())
        .await?;
    let addr3 = client3
        .send_binding_request_to(format!("127.0.0.1:{server_port}").as_str())
        .await?;

    user1.send_to(b"1", addr1).await?;
    user2.send_to(b"12", addr2).await?;
    user3.send_to(b"123", addr3).await?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    server
        .get_allocations_info(None)
        .await?
        .iter()
        .for_each(|(_, ai)| match ai.username.as_str() {
            "user1" => assert_eq!(ai.relayed_bytes, 1),
            "user2" => assert_eq!(ai.relayed_bytes, 2),
            "user3" => assert_eq!(ai.relayed_bytes, 3),
            _ => unreachable!(),
        });

    Ok(())
}

#[cfg(feature = "metrics")]
#[tokio::test]
async fn test_get_allocations_info_bytes_count() -> Result<()> {
    let (server, server_port) = create_server().await?;

    let client = create_client("foo".to_owned(), server_port).await?;

    client.listen().await?;

    assert!(server.get_allocations_info(None).await?.is_empty());

    let conn = client.allocate().await?;
    let addr = client
        .send_binding_request_to(format!("127.0.0.1:{server_port}").as_str())
        .await?;

    assert!(!server.get_allocations_info(None).await?.is_empty());

    assert_eq!(
        server
            .get_allocations_info(None)
            .await?
            .values()
            .last()
            .unwrap()
            .relayed_bytes,
        0
    );

    for _ in 0..10 {
        conn.send_to(b"Hello", addr).await?;

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    assert_eq!(
        server
            .get_allocations_info(None)
            .await?
            .values()
            .last()
            .unwrap()
            .relayed_bytes,
        50
    );

    for _ in 0..10 {
        conn.send_to(b"Hello", addr).await?;

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    assert_eq!(
        server
            .get_allocations_info(None)
            .await?
            .values()
            .last()
            .unwrap()
            .relayed_bytes,
        100
    );

    client.close().await?;
    server.close().await?;

    Ok(())
}
