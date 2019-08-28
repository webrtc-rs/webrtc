use super::*;
use util::Error;

use std::io::BufReader;
use std::iter::Iterator;

#[test]
fn test_ice_candidate() -> Result<(), Error> {
    let tt = vec![
		"1938809241 1 udp 2122262783 fcd9:e3b8:12ce:9fc5:74a5:c6bb:d8b:e08a 53987 typ host generation 0 network-id 4",
		"1986380506 1 udp 2122063615 10.0.75.1 53634 typ host generation 0 network-id 2",
		"1986380506 1 udp 2122063615 10.0.75.1 53634 typ host",
		"4207374051 1 udp 1685790463 191.228.238.68 53991 typ srflx raddr 192.168.0.278 rport 53991 generation 0 network-id 3",
		"4207374051 1 udp 1685790463 191.228.238.68 53991 typ srflx raddr 192.168.0.274 rport 53991",
	];

    for (i, &tc) in tt.iter().enumerate() {
        let mut reader = BufReader::new(tc.as_bytes());
        let parsed = ICECandidate::unmarshal(&mut reader)?;
        let actual = parsed.marshal();
        if tc != &actual {
            assert!(
                false,
                "ICE candidate test {} failed. Got:\n{}\nExpected:\n{}",
                i, actual, tc
            )
        }
    }

    Ok(())
}

#[test]
fn test_ice_candidate_failure() -> Result<(), Error> {
    let tt = vec![
		"",
		"1938809241",
		"1986380506 99999999 udp 2122063615 10.0.75.1 53634 typ host generation 0 network-id 2",
		"1986380506 1 udp 99999999999 10.0.75.1 53634 typ host",
		"4207374051 1 udp 1685790463 191.228.238.68 99999999 typ srflx raddr 192.168.0.278 rport 53991 generation 0 network-id 3",
		"4207374051 1 udp 1685790463 191.228.238.68 53991 typ srflx raddr",
		"4207374051 1 udp 1685790463 191.228.238.68 53991 typ srflx raddr 192.168.0.278 rport 99999999 generation 0 network-id 3",
	];

    for (i, &tc) in tt.iter().enumerate() {
        let mut reader = BufReader::new(tc.as_bytes());
        let parse = ICECandidate::unmarshal(&mut reader);
        assert!(
            parse.is_err(),
            "ICE candidate failure test {}: expected error",
            i,
        )
    }

    Ok(())
}
