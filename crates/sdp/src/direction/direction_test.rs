use super::*;
use std::iter::Iterator;

#[test]
fn test_new_direction() {
    let passingtests = vec![
        ("sendrecv", Direction::DirectionSendRecv),
        ("sendonly", Direction::DirectionSendOnly),
        ("recvonly", Direction::DirectionRecvOnly),
        ("inactive", Direction::DirectionInactive),
    ];

    let failingtests = vec!["", "notadirection"];

    for (i, u) in passingtests.iter().enumerate() {
        let dir = Direction::new(u.0);
        assert!(u.1 == dir, "{}: {}", i, u.0);
    }
    for (_, &u) in failingtests.iter().enumerate() {
        let dir = Direction::new(u);
        assert!(dir == Direction::DirectionUnknown);
    }
}

#[test]
fn test_direction_string() {
    let tests = vec![
        (Direction::DirectionUnknown, DIRECTION_UNKNOWN_STR),
        (Direction::DirectionSendRecv, "sendrecv"),
        (Direction::DirectionSendOnly, "sendonly"),
        (Direction::DirectionRecvOnly, "recvonly"),
        (Direction::DirectionInactive, "inactive"),
    ];

    for (i, u) in tests.iter().enumerate() {
        assert!(u.1 == u.0.to_string(), "{}: {}", i, u.1);
    }
}
