use super::*;
//use std::io::BufReader;
use std::time::Duration;

use chrono::prelude::*;

use std::ops::Sub;
use util::Error;

const ABS_SEND_TIME_RESOLUTION: i128 = 3800 * 1_000_000_000; // time.Nanosecond;

#[test]
fn test_ntp_conversion() -> Result<(), Error> {
    let loc = FixedOffset::west(5 * 60 * 60); // UTC-5
    let tests = vec![
        (
            loc.ymd(1985, 6, 23).and_hms_nano(4, 0, 0, 0),
            0xa0c65b1000000000 as u64,
        ),
        (
            loc.ymd(1999, 12, 31).and_hms_nano(23, 59, 59, 500000),
            0xbc18084f0020c49b as u64,
        ),
        (
            loc.ymd(2019, 3, 27).and_hms_nano(13, 39, 30, 8675309),
            0xe04641e202388b88 as u64,
        ),
    ];

    for (t, n) in &tests {
        let ntp = unix2ntp(Duration::from_nanos(t.timestamp_nanos() as u64));
        assert_eq!(ntp, *n, "unix2ntp error");
    }

    for (t, n) in &tests {
        let output = ntp2unix(*n);
        let input = Duration::from_nanos(t.timestamp_nanos() as u64);
        let diff = input.sub(output).as_nanos() as i128;
        if diff < -ABS_SEND_TIME_RESOLUTION || ABS_SEND_TIME_RESOLUTION < diff {
            assert!(
                false,
                "Converted time.Time from NTP time differs, expected: {:?}, got: {:?}",
                input, output,
            );
        }
    }

    Ok(())
}
