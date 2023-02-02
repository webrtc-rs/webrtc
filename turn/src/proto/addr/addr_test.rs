use super::*;
use crate::error::Result;

use std::net::Ipv4Addr;

#[test]
fn test_addr_from_socket_addr() -> Result<()> {
    let u = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1234);

    let a = Addr::from_socket_addr(&u);
    assert!(
        u.ip() == a.ip || u.port() != a.port || u.to_string() != a.to_string(),
        "not equal"
    );
    assert_eq!(a.network(), "turn", "unexpected network");

    Ok(())
}

#[test]
fn test_addr_equal_ip() -> Result<()> {
    let a = Addr {
        ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        port: 1337,
    };
    let b = Addr {
        ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        port: 1338,
    };
    assert_ne!(a, b, "a != b");
    assert!(a.equal_ip(&b), "a.IP should equal to b.IP");

    Ok(())
}

#[test]
fn test_five_tuple_equal() -> Result<()> {
    let tests = vec![
        ("blank", FiveTuple::default(), FiveTuple::default(), true),
        (
            "proto",
            FiveTuple {
                proto: PROTO_UDP,
                ..Default::default()
            },
            FiveTuple::default(),
            false,
        ),
        (
            "server",
            FiveTuple {
                server: Addr {
                    port: 100,
                    ..Default::default()
                },
                ..Default::default()
            },
            FiveTuple::default(),
            false,
        ),
        (
            "client",
            FiveTuple {
                client: Addr {
                    port: 100,
                    ..Default::default()
                },
                ..Default::default()
            },
            FiveTuple::default(),
            false,
        ),
    ];

    for (name, a, b, r) in tests {
        let v = a == b;
        assert_eq!(v, r, "({name}) {a} [{v}!={r}] {b}");
    }

    Ok(())
}

#[test]
fn test_five_tuple_string() -> Result<()> {
    let s = FiveTuple {
        proto: PROTO_UDP,
        server: Addr {
            port: 100,
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        },
        client: Addr {
            port: 200,
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        },
    }
    .to_string();

    assert_eq!(
        s, "127.0.0.1:200->127.0.0.1:100 (UDP)",
        "unexpected stringer output"
    );

    Ok(())
}
