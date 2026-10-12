use super::*;
use tokio::net::UdpSocket;

fn generator(address: &str, min_port: u16, max_port: u16) -> RelayAddressGeneratorRanges {
    RelayAddressGeneratorRanges {
        relay_address: address.parse().unwrap(),
        min_port,
        max_port,
        max_retries: 10,
        address: address.into(),
        net: Arc::new(Net::new(None)),
    }
}

#[tokio::test]
async fn random_ports_stay_inside_an_inclusive_range_ending_at_65535() {
    let generator = generator("127.0.0.1", 49152, u16::MAX);
    let (conn, address) = generator.allocate_conn(true, 0).await.unwrap();
    assert!((49152..=u16::MAX).contains(&address.port()));
    conn.close().await.unwrap();
}

#[tokio::test]
async fn requested_port_cannot_escape_the_configured_range() {
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let outside = probe.local_addr().unwrap().port();
    let inside = if outside == 49152 { 49153 } else { 49152 };
    let generator = generator("127.0.0.1", inside, inside);
    drop(probe);
    assert!(generator.allocate_conn(true, outside).await.is_err());
}

#[tokio::test]
async fn ipv6_relay_address_is_supported() {
    let generator = generator("::1", 49152, 65000);
    let (conn, address) = generator.allocate_conn(false, 0).await.unwrap();
    assert!(address.is_ipv6());
    assert_eq!(address.ip(), "::1".parse::<IpAddr>().unwrap());
    conn.close().await.unwrap();
}

#[tokio::test]
async fn requested_family_must_match_the_advertised_relay() {
    let generator = generator("127.0.0.1", 49152, u16::MAX);
    assert!(generator.allocate_conn(false, 0).await.is_err());
}

#[tokio::test]
async fn relay_bind_address_can_still_be_a_hostname() {
    let mut generator = generator("127.0.0.1", 49152, 65000);
    generator.address = "localhost".into();
    let (conn, address) = generator.allocate_conn(true, 0).await.unwrap();
    assert_eq!(address.ip(), "127.0.0.1".parse::<IpAddr>().unwrap());
    conn.close().await.unwrap();
}

#[tokio::test]
async fn requested_port_inside_a_single_port_range_is_accepted() {
    let probe = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    let generator = generator("127.0.0.1", port, port);
    drop(probe);
    let (conn, address) = generator.allocate_conn(true, port).await.unwrap();
    assert_eq!(address.port(), port);
    conn.close().await.unwrap();
}
