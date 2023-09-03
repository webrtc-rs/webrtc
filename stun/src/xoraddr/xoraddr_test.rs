use std::io::BufReader;

use base64::prelude::BASE64_STANDARD;
use base64::Engine;

use super::*;
use crate::checks::*;

#[test]
fn test_xor_safe() -> Result<()> {
    let mut dst = vec![0; 8];
    let a = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let b = vec![8, 7, 7, 6, 6, 3, 4, 1];
    safe_xor_bytes(&mut dst, &a, &b);
    let c = dst.clone();
    safe_xor_bytes(&mut dst, &c, &a);
    for i in 0..dst.len() {
        assert_eq!(b[i], dst[i], "{} != {}", b[i], dst[i]);
    }

    Ok(())
}

#[test]
fn test_xor_safe_bsmaller() -> Result<()> {
    let mut dst = vec![0; 5];
    let a = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let b = vec![8, 7, 7, 6, 6];
    safe_xor_bytes(&mut dst, &a, &b);
    let c = dst.clone();
    safe_xor_bytes(&mut dst, &c, &a);
    for i in 0..dst.len() {
        assert_eq!(b[i], dst[i], "{} != {}", b[i], dst[i]);
    }

    Ok(())
}

#[test]
fn test_xormapped_address_get_from() -> Result<()> {
    let mut m = Message::new();
    let transaction_id = BASE64_STANDARD.decode("jxhBARZwX+rsC6er").unwrap();
    m.transaction_id.0.copy_from_slice(&transaction_id);
    let addr_value = vec![0x00, 0x01, 0x9c, 0xd5, 0xf4, 0x9f, 0x38, 0xae];
    m.add(ATTR_XORMAPPED_ADDRESS, &addr_value);
    let mut addr = XorMappedAddress {
        ip: "0.0.0.0".parse().unwrap(),
        port: 0,
    };
    addr.get_from(&m)?;
    assert_eq!(
        addr.ip.to_string(),
        "213.141.156.236",
        "bad IP {} != 213.141.156.236",
        addr.ip
    );
    assert_eq!(addr.port, 48583, "bad Port {} != 48583", addr.port);

    //"UnexpectedEOF"
    {
        let mut m = Message::new();
        // {0, 1} is correct addr family.
        m.add(ATTR_XORMAPPED_ADDRESS, &[0, 1, 3, 4]);
        let mut addr = XorMappedAddress {
            ip: "0.0.0.0".parse().unwrap(),
            port: 0,
        };
        let result = addr.get_from(&m);
        if let Err(err) = result {
            assert_eq!(
                Error::ErrUnexpectedEof,
                err,
                "len(v) = 4 should render <{}> error, got <{}>",
                Error::ErrUnexpectedEof,
                err
            );
        } else {
            panic!("expected error, got ok");
        }
    }
    //"AttrOverflowErr"
    {
        let mut m = Message::new();
        // {0, 1} is correct addr family.
        m.add(
            ATTR_XORMAPPED_ADDRESS,
            &[0, 1, 3, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1, 1, 2, 3, 4],
        );
        let mut addr = XorMappedAddress {
            ip: "0.0.0.0".parse().unwrap(),
            port: 0,
        };
        let result = addr.get_from(&m);
        if let Err(err) = result {
            assert!(
                is_attr_size_overflow(&err),
                "AddTo should return AttrOverflowErr, got: {err}"
            );
        } else {
            panic!("expected error, got ok");
        }
    }

    Ok(())
}

#[test]
fn test_xormapped_address_get_from_invalid() -> Result<()> {
    let mut m = Message::new();
    let transaction_id = BASE64_STANDARD.decode("jxhBARZwX+rsC6er").unwrap();
    m.transaction_id.0.copy_from_slice(&transaction_id);
    let expected_ip: IpAddr = "213.141.156.236".parse().unwrap();
    let expected_port = 21254u16;
    let mut addr = XorMappedAddress {
        ip: "0.0.0.0".parse().unwrap(),
        port: 0,
    };
    let result = addr.get_from(&m);
    assert!(result.is_err(), "should be error");

    addr.ip = expected_ip;
    addr.port = expected_port;
    addr.add_to(&mut m)?;
    m.write_header();

    let mut m_res = Message::new();
    m.raw[20 + 4 + 1] = 0x21;
    m.decode()?;
    let mut reader = BufReader::new(m.raw.as_slice());
    m_res.read_from(&mut reader)?;
    let result = addr.get_from(&m);
    assert!(result.is_err(), "should be error");

    Ok(())
}

#[test]
fn test_xormapped_address_add_to() -> Result<()> {
    let mut m = Message::new();
    let transaction_id = BASE64_STANDARD.decode("jxhBARZwX+rsC6er").unwrap();
    m.transaction_id.0.copy_from_slice(&transaction_id);
    let expected_ip: IpAddr = "213.141.156.236".parse().unwrap();
    let expected_port = 21254u16;
    let mut addr = XorMappedAddress {
        ip: "213.141.156.236".parse().unwrap(),
        port: expected_port,
    };
    addr.add_to(&mut m)?;
    m.write_header();

    let mut m_res = Message::new();
    m_res.write(&m.raw)?;
    addr.get_from(&m_res)?;
    assert_eq!(
        addr.ip, expected_ip,
        "{} (got) != {} (expected)",
        addr.ip, expected_ip
    );

    assert_eq!(
        addr.port, expected_port,
        "bad Port {} != {}",
        addr.port, expected_port
    );

    Ok(())
}

#[test]
fn test_xormapped_address_add_to_ipv6() -> Result<()> {
    let mut m = Message::new();
    let transaction_id = BASE64_STANDARD.decode("jxhBARZwX+rsC6er").unwrap();
    m.transaction_id.0.copy_from_slice(&transaction_id);
    let expected_ip: IpAddr = "fe80::dc2b:44ff:fe20:6009".parse().unwrap();
    let expected_port = 21254u16;
    let addr = XorMappedAddress {
        ip: "fe80::dc2b:44ff:fe20:6009".parse().unwrap(),
        port: 21254,
    };
    addr.add_to(&mut m)?;
    m.write_header();

    let mut m_res = Message::new();
    let mut reader = BufReader::new(m.raw.as_slice());
    m_res.read_from(&mut reader)?;

    let mut got_addr = XorMappedAddress {
        ip: "0.0.0.0".parse().unwrap(),
        port: 0,
    };
    got_addr.get_from(&m)?;

    assert_eq!(
        got_addr.ip, expected_ip,
        "bad IP {} != {}",
        got_addr.ip, expected_ip
    );
    assert_eq!(
        got_addr.port, expected_port,
        "bad Port {} != {}",
        got_addr.port, expected_port
    );

    Ok(())
}

/*
#[test]
fn TestXORMappedAddress_AddTo_Invalid() -> Result<()> {
    let mut m = Message::new();
    let mut addr = XORMappedAddress{
        ip:   1, 2, 3, 4, 5, 6, 7, 8},
        port: 21254,
    }
    if err := addr.AddTo(m); !errors.Is(err, ErrBadIPLength) {
        t.Errorf("AddTo should return %q, got: %v", ErrBadIPLength, err)
    }
}*/

#[test]
fn test_xormapped_address_string() -> Result<()> {
    let tests = vec![
        (
            // 0
            XorMappedAddress {
                ip: "fe80::dc2b:44ff:fe20:6009".parse().unwrap(),
                port: 124,
            },
            "[fe80::dc2b:44ff:fe20:6009]:124",
        ),
        (
            // 1
            XorMappedAddress {
                ip: "213.141.156.236".parse().unwrap(),
                port: 8147,
            },
            "213.141.156.236:8147",
        ),
    ];

    for (addr, ip) in tests {
        assert_eq!(
            addr.to_string(),
            ip,
            " XORMappesAddress.String() {addr} (got) != {ip} (expected)",
        );
    }

    Ok(())
}
