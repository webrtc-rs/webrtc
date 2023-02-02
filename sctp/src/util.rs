use bytes::Bytes;
use crc::{Crc, CRC_32_ISCSI};

pub(crate) const PADDING_MULTIPLE: usize = 4;

pub(crate) fn get_padding_size(len: usize) -> usize {
    (PADDING_MULTIPLE - (len % PADDING_MULTIPLE)) % PADDING_MULTIPLE
}

/// Allocate and zero this data once.
/// We need to use it for the checksum and don't want to allocate/clear each time.
pub(crate) static FOUR_ZEROES: Bytes = Bytes::from_static(&[0, 0, 0, 0]);

pub(crate) const ISCSI_CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// Fastest way to do a crc32 without allocating.
pub(crate) fn generate_packet_checksum(raw: &Bytes) -> u32 {
    let mut digest = ISCSI_CRC.digest();
    digest.update(&raw[0..8]);
    digest.update(&FOUR_ZEROES[..]);
    digest.update(&raw[12..]);
    digest.finalize()
}

/// Serial Number Arithmetic (RFC 1982)
#[inline]
pub(crate) fn sna32lt(i1: u32, i2: u32) -> bool {
    (i1 < i2 && i2 - i1 < 1 << 31) || (i1 > i2 && i1 - i2 > 1 << 31)
}

#[inline]
pub(crate) fn sna32lte(i1: u32, i2: u32) -> bool {
    i1 == i2 || sna32lt(i1, i2)
}

#[inline]
pub(crate) fn sna32gt(i1: u32, i2: u32) -> bool {
    (i1 < i2 && (i2 - i1) >= 1 << 31) || (i1 > i2 && (i1 - i2) <= 1 << 31)
}

#[inline]
pub(crate) fn sna32gte(i1: u32, i2: u32) -> bool {
    i1 == i2 || sna32gt(i1, i2)
}

#[inline]
pub(crate) fn sna32eq(i1: u32, i2: u32) -> bool {
    i1 == i2
}

#[inline]
pub(crate) fn sna16lt(i1: u16, i2: u16) -> bool {
    (i1 < i2 && (i2 - i1) < 1 << 15) || (i1 > i2 && (i1 - i2) > 1 << 15)
}

#[inline]
pub(crate) fn sna16lte(i1: u16, i2: u16) -> bool {
    i1 == i2 || sna16lt(i1, i2)
}

#[inline]
pub(crate) fn sna16gt(i1: u16, i2: u16) -> bool {
    (i1 < i2 && (i2 - i1) >= 1 << 15) || (i1 > i2 && (i1 - i2) <= 1 << 15)
}

#[inline]
pub(crate) fn sna16gte(i1: u16, i2: u16) -> bool {
    i1 == i2 || sna16gt(i1, i2)
}

#[inline]
pub(crate) fn sna16eq(i1: u16, i2: u16) -> bool {
    i1 == i2
}

#[cfg(test)]
mod test {
    use crate::error::Result;

    use super::*;

    const DIV: isize = 16;

    #[test]
    fn test_serial_number_arithmetic32bit() -> Result<()> {
        const SERIAL_BITS: u32 = 32;
        const INTERVAL: u32 = ((1u64 << (SERIAL_BITS as u64)) / (DIV as u64)) as u32;
        const MAX_FORWARD_DISTANCE: u32 = 1 << ((SERIAL_BITS - 1) - 1);
        const MAX_BACKWARD_DISTANCE: u32 = 1 << (SERIAL_BITS - 1);

        for i in 0..DIV as u32 {
            let s1 = i * INTERVAL;
            let s2f = s1.checked_add(MAX_FORWARD_DISTANCE);
            let s2b = s1.checked_add(MAX_BACKWARD_DISTANCE);

            if let (Some(s2f), Some(s2b)) = (s2f, s2b) {
                assert!(sna32lt(s1, s2f), "s1 < s2 should be true: s1={s1} s2={s2f}");
                assert!(
                    !sna32lt(s1, s2b),
                    "s1 < s2 should be false: s1={s1} s2={s2b}"
                );

                assert!(
                    !sna32gt(s1, s2f),
                    "s1 > s2 should be false: s1={s1} s2={s2f}"
                );
                assert!(sna32gt(s1, s2b), "s1 > s2 should be true: s1={s1} s2={s2b}");

                assert!(
                    sna32lte(s1, s2f),
                    "s1 <= s2 should be true: s1={s1} s2={s2f}"
                );
                assert!(
                    !sna32lte(s1, s2b),
                    "s1 <= s2 should be false: s1={s1} s2={s2b}"
                );

                assert!(
                    !sna32gte(s1, s2f),
                    "s1 >= s2 should be fales: s1={s1} s2={s2f}"
                );
                assert!(
                    sna32gte(s1, s2b),
                    "s1 >= s2 should be true: s1={s1} s2={s2b}"
                );

                assert!(
                    sna32eq(s2b, s2b),
                    "s2 == s2 should be true: s2={s2b} s2={s2b}"
                );
                assert!(
                    sna32lte(s2b, s2b),
                    "s2 == s2 should be true: s2={s2b} s2={s2b}"
                );
                assert!(
                    sna32gte(s2b, s2b),
                    "s2 == s2 should be true: s2={s2b} s2={s2b}"
                );
            }

            if let Some(s1add1) = s1.checked_add(1) {
                assert!(
                    !sna32eq(s1, s1add1),
                    "s1 == s1+1 should be false: s1={s1} s1+1={s1add1}"
                );
            }

            if let Some(s1sub1) = s1.checked_sub(1) {
                assert!(
                    !sna32eq(s1, s1sub1),
                    "s1 == s1-1 hould be false: s1={s1} s1-1={s1sub1}"
                );
            }

            assert!(sna32eq(s1, s1), "s1 == s1 should be true: s1={s1} s2={s1}");
            assert!(sna32lte(s1, s1), "s1 == s1 should be true: s1={s1} s2={s1}");

            assert!(sna32gte(s1, s1), "s1 == s1 should be true: s1={s1} s2={s1}");
        }

        Ok(())
    }

    #[test]
    fn test_serial_number_arithmetic16bit() -> Result<()> {
        const SERIAL_BITS: u16 = 16;
        const INTERVAL: u16 = ((1u64 << (SERIAL_BITS as u64)) / (DIV as u64)) as u16;
        const MAX_FORWARD_DISTANCE: u16 = 1 << ((SERIAL_BITS - 1) - 1);
        const MAX_BACKWARD_DISTANCE: u16 = 1 << (SERIAL_BITS - 1);

        for i in 0..DIV as u16 {
            let s1 = i * INTERVAL;
            let s2f = s1.checked_add(MAX_FORWARD_DISTANCE);
            let s2b = s1.checked_add(MAX_BACKWARD_DISTANCE);

            if let (Some(s2f), Some(s2b)) = (s2f, s2b) {
                assert!(sna16lt(s1, s2f), "s1 < s2 should be true: s1={s1} s2={s2f}");
                assert!(
                    !sna16lt(s1, s2b),
                    "s1 < s2 should be false: s1={s1} s2={s2b}"
                );

                assert!(
                    !sna16gt(s1, s2f),
                    "s1 > s2 should be fales: s1={s1} s2={s2f}"
                );
                assert!(sna16gt(s1, s2b), "s1 > s2 should be true: s1={s1} s2={s2b}");

                assert!(
                    sna16lte(s1, s2f),
                    "s1 <= s2 should be true: s1={s1} s2={s2f}"
                );
                assert!(
                    !sna16lte(s1, s2b),
                    "s1 <= s2 should be false: s1={s1} s2={s2b}"
                );

                assert!(
                    !sna16gte(s1, s2f),
                    "s1 >= s2 should be fales: s1={s1} s2={s2f}"
                );
                assert!(
                    sna16gte(s1, s2b),
                    "s1 >= s2 should be true: s1={s1} s2={s2b}"
                );

                assert!(
                    sna16eq(s2b, s2b),
                    "s2 == s2 should be true: s2={s2b} s2={s2b}"
                );
                assert!(
                    sna16lte(s2b, s2b),
                    "s2 == s2 should be true: s2={s2b} s2={s2b}"
                );
                assert!(
                    sna16gte(s2b, s2b),
                    "s2 == s2 should be true: s2={s2b} s2={s2b}"
                );
            }

            assert!(sna16eq(s1, s1), "s1 == s1 should be true: s1={s1} s2={s1}");

            if let Some(s1add1) = s1.checked_add(1) {
                assert!(
                    !sna16eq(s1, s1add1),
                    "s1 == s1+1 should be false: s1={s1} s1+1={s1add1}"
                );
            }
            if let Some(s1sub1) = s1.checked_sub(1) {
                assert!(
                    !sna16eq(s1, s1sub1),
                    "s1 == s1-1 hould be false: s1={s1} s1-1={s1sub1}"
                );
            }

            assert!(sna16lte(s1, s1), "s1 == s1 should be true: s1={s1} s2={s1}");
            assert!(sna16gte(s1, s1), "s1 == s1 should be true: s1={s1} s2={s1}");
        }

        Ok(())
    }
}
