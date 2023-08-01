use std::net::Ipv4Addr;

use super::*;

#[test]
fn test_peer_address() -> Result<(), stun::Error> {
    // Simple tests because already tested in stun.
    let a = PeerAddress {
        ip: IpAddr::V4(Ipv4Addr::new(111, 11, 1, 2)),
        port: 333,
    };

    assert_eq!(a.to_string(), "111.11.1.2:333", "invalid string");

    let mut m = Message::new();
    a.add_to(&mut m)?;
    m.write_header();

    let mut decoded = Message::new();
    decoded.write(&m.raw)?;

    let mut a_got = PeerAddress::default();
    a_got.get_from(&decoded)?;

    Ok(())
}
