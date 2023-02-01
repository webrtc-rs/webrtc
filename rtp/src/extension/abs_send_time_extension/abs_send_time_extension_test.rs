use super::*;
use crate::error::Result;

use bytes::BytesMut;
use chrono::prelude::*;
use std::time::Duration;

const ABS_SEND_TIME_RESOLUTION: i128 = 1000;

#[test]
fn test_ntp_conversion() -> Result<()> {
    let loc = FixedOffset::west_opt(5 * 60 * 60).unwrap(); // UTC-5
    let tests = vec![
        (
            loc.with_ymd_and_hms(1985, 6, 23, 4, 0, 0).unwrap(),
            0xa0c65b1000000000_u64,
        ),
        (
            // TODO: fix this. MA: There's only so long I will stare at
            // APIs that sacrifice convenience for correctness.
            #[allow(deprecated)]
            loc.ymd(1999, 12, 31)
                .and_hms_nano_opt(23, 59, 59, 500000)
                .unwrap(),
            0xbc18084f0020c49b_u64,
        ),
        (
            #[allow(deprecated)]
            loc.ymd(2019, 3, 27)
                .and_hms_nano_opt(13, 39, 30, 8675309)
                .unwrap(),
            0xe04641e202388b88_u64,
        ),
    ];

    for (t, n) in &tests {
        let st = UNIX_EPOCH
            .checked_add(Duration::from_nanos(t.timestamp_nanos() as u64))
            .unwrap_or(UNIX_EPOCH);
        let ntp = unix2ntp(st);

        if cfg!(target_os = "windows") {
            let actual = ntp as i128;
            let expected = *n as i128;
            let diff = actual - expected;
            if !(-ABS_SEND_TIME_RESOLUTION..=ABS_SEND_TIME_RESOLUTION).contains(&diff) {
                panic!("unix2ntp error, expected: {:?}, got: {:?}", ntp, *n,);
            }
        } else {
            assert_eq!(ntp, *n, "unix2ntp error");
        }
    }

    for (t, n) in &tests {
        let output = ntp2unix(*n);
        let input = UNIX_EPOCH
            .checked_add(Duration::from_nanos(t.timestamp_nanos() as u64))
            .unwrap_or(UNIX_EPOCH);
        let diff = input.duration_since(output).unwrap().as_nanos() as i128;
        if !(-ABS_SEND_TIME_RESOLUTION..=ABS_SEND_TIME_RESOLUTION).contains(&diff) {
            panic!(
                "Converted time.Time from NTP time differs, expected: {input:?}, got: {output:?}",
            );
        }
    }

    Ok(())
}

#[test]
fn test_abs_send_time_extension_roundtrip() -> Result<()> {
    let tests = vec![
        AbsSendTimeExtension { timestamp: 123456 },
        AbsSendTimeExtension { timestamp: 654321 },
    ];

    for test in &tests {
        let mut raw = BytesMut::with_capacity(test.marshal_size());
        raw.resize(test.marshal_size(), 0);
        test.marshal_to(&mut raw)?;
        let raw = raw.freeze();
        let buf = &mut raw.clone();
        let out = AbsSendTimeExtension::unmarshal(buf)?;
        assert_eq!(test.timestamp, out.timestamp);
    }

    Ok(())
}

#[test]
fn test_abs_send_time_extension_estimate() -> Result<()> {
    let tests = vec![
        //FFFFFFC000000000 mask of second
        (0xa0c65b1000100000, 0xa0c65b1001000000), // not carried
        (0xa0c65b3f00000000, 0xa0c65b4001000000), // carried during transmission
    ];

    for (send_ntp, receive_ntp) in tests {
        let in_time = ntp2unix(send_ntp);
        let send = AbsSendTimeExtension {
            timestamp: send_ntp >> 14,
        };
        let mut raw = BytesMut::with_capacity(send.marshal_size());
        raw.resize(send.marshal_size(), 0);
        send.marshal_to(&mut raw)?;
        let raw = raw.freeze();
        let buf = &mut raw.clone();
        let receive = AbsSendTimeExtension::unmarshal(buf)?;

        let estimated = receive.estimate(ntp2unix(receive_ntp));
        let diff = estimated.duration_since(in_time).unwrap().as_nanos() as i128;
        if !(-ABS_SEND_TIME_RESOLUTION..=ABS_SEND_TIME_RESOLUTION).contains(&diff) {
            panic!(
                "Converted time.Time from NTP time differs, expected: {in_time:?}, got: {estimated:?}",
            );
        }
    }

    Ok(())
}
