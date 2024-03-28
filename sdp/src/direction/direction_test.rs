use std::iter::Iterator;

use super::*;

#[test]
fn test_new_direction() {
    let passingtests = [
        ("sendrecv", Direction::SendRecv),
        ("sendonly", Direction::SendOnly),
        ("recvonly", Direction::RecvOnly),
        ("inactive", Direction::Inactive),
    ];

    let failingtests = ["", "notadirection"];

    for (i, u) in passingtests.iter().enumerate() {
        let dir = Direction::new(u.0);
        assert!(u.1 == dir, "{}: {}", i, u.0);
    }
    for &u in failingtests.iter() {
        let dir = Direction::new(u);
        assert!(dir == Direction::Unspecified);
    }
}

#[test]
fn test_direction_string() {
    let tests = [
        (Direction::Unspecified, DIRECTION_UNSPECIFIED_STR),
        (Direction::SendRecv, "sendrecv"),
        (Direction::SendOnly, "sendonly"),
        (Direction::RecvOnly, "recvonly"),
        (Direction::Inactive, "inactive"),
    ];

    for (i, u) in tests.iter().enumerate() {
        assert!(u.1 == u.0.to_string(), "{}: {}", i, u.1);
    }
}
