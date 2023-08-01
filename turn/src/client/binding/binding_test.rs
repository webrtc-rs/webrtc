use std::net::{Ipv4Addr, SocketAddrV4};

use super::*;
use crate::error::Result;

#[test]
fn test_binding_manager_number_assignment() -> Result<()> {
    let mut m = BindingManager::new();
    let mut n: u16;
    for i in 0..10 {
        n = m.assign_channel_number();
        assert_eq!(MIN_CHANNEL_NUMBER + i, n, "should match");
    }

    m.next = 0x7ff0;
    for i in 0..16 {
        n = m.assign_channel_number();
        assert_eq!(0x7ff0 + i, n, "should match");
    }
    // back to min
    n = m.assign_channel_number();
    assert_eq!(MIN_CHANNEL_NUMBER, n, "should match");

    Ok(())
}

#[test]
fn test_binding_manager_method() -> Result<()> {
    let lo = Ipv4Addr::new(127, 0, 0, 1);
    let count = 100;
    let mut m = BindingManager::new();
    for i in 0..count {
        let addr = SocketAddr::V4(SocketAddrV4::new(lo, 10000 + i));
        let b0 = {
            let b = m.create(addr);
            *b.unwrap()
        };
        let b1 = m.find_by_addr(&addr);
        assert!(b1.is_some(), "should succeed");
        let b2 = m.find_by_number(b0.number);
        assert!(b2.is_some(), "should succeed");

        assert_eq!(b0, *b1.unwrap(), "should match");
        assert_eq!(b0, *b2.unwrap(), "should match");
    }

    assert_eq!(count, m.size() as u16, "should match");
    assert_eq!(count, m.addr_map.len() as u16, "should match");

    for i in 0..count {
        let addr = SocketAddr::V4(SocketAddrV4::new(lo, 10000 + i));
        if i % 2 == 0 {
            assert!(m.delete_by_addr(&addr), "should return true");
        } else {
            assert!(
                m.delete_by_number(MIN_CHANNEL_NUMBER + i),
                "should return true"
            );
        }
    }

    assert_eq!(0, m.size(), "should match");
    assert_eq!(0, m.addr_map.len(), "should match");

    Ok(())
}

#[test]
fn test_binding_manager_failure() -> Result<()> {
    let ipv4 = Ipv4Addr::new(127, 0, 0, 1);
    let addr = SocketAddr::V4(SocketAddrV4::new(ipv4, 7777));
    let mut m = BindingManager::new();
    let b = m.find_by_addr(&addr);
    assert!(b.is_none(), "should fail");
    let b = m.find_by_number(5555);
    assert!(b.is_none(), "should fail");
    let ok = m.delete_by_addr(&addr);
    assert!(!ok, "should fail");
    let ok = m.delete_by_number(5555);
    assert!(!ok, "should fail");

    Ok(())
}
