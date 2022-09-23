use crate::shared::AssociationId;

use bytes::Bytes;
use crc::{Crc, CRC_32_ISCSI};
use std::time::Duration;

/// This function is non-inline to prevent the optimizer from looking inside it.
#[inline(never)]
fn constant_time_ne(a: &[u8], b: &[u8]) -> u8 {
    assert!(a.len() == b.len());

    // These useless slices make the optimizer elide the bounds checks.
    // See the comment in clone_from_slice() added on Rust commit 6a7bc47.
    let len = a.len();
    let a = &a[..len];
    let b = &b[..len];

    let mut tmp = 0;
    for i in 0..len {
        tmp |= a[i] ^ b[i];
    }
    tmp // The compare with 0 must happen outside this function.
}

/// Compares byte strings in constant time.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && constant_time_ne(a, b) == 0
}

/// Generates association id for incoming associations
pub trait AssociationIdGenerator: Send {
    /// Generates a new AID
    ///
    /// Association IDs MUST NOT contain any information that can be used by
    /// an external observer (that is, one that does not cooperate with the
    /// issuer) to correlate them with other Association IDs for the same
    /// Association.
    fn generate_aid(&mut self) -> AssociationId;

    /// Returns the lifetime of generated Association IDs
    ///
    /// Association IDs will be retired after the returned `Duration`, if any. Assumed to be constant.
    fn aid_lifetime(&self) -> Option<Duration>;
}

/// Generates purely random Association IDs of a certain length
#[derive(Default, Debug, Clone, Copy)]
pub struct RandomAssociationIdGenerator {
    lifetime: Option<Duration>,
}

impl RandomAssociationIdGenerator {
    /// Initialize Random AID generator
    pub fn new() -> Self {
        RandomAssociationIdGenerator::default()
    }

    /// Set the lifetime of CIDs created by this generator
    pub fn set_lifetime(&mut self, d: Duration) -> &mut Self {
        self.lifetime = Some(d);
        self
    }
}

impl AssociationIdGenerator for RandomAssociationIdGenerator {
    fn generate_aid(&mut self) -> AssociationId {
        rand::random::<u32>()
    }

    fn aid_lifetime(&self) -> Option<Duration> {
        self.lifetime
    }
}

const PADDING_MULTIPLE: usize = 4;

pub(crate) fn get_padding_size(len: usize) -> usize {
    (PADDING_MULTIPLE - (len % PADDING_MULTIPLE)) % PADDING_MULTIPLE
}

/// Allocate and zero this data once.
/// We need to use it for the checksum and don't want to allocate/clear each time.
pub(crate) static FOUR_ZEROES: Bytes = Bytes::from_static(&[0, 0, 0, 0]);

/// Fastest way to do a crc32 without allocating.
pub(crate) fn generate_packet_checksum(raw: &Bytes) -> u32 {
    let hasher = Crc::<u32>::new(&CRC_32_ISCSI);
    let mut digest = hasher.digest();
    digest.update(&raw[0..8]);
    digest.update(&FOUR_ZEROES[..]);
    digest.update(&raw[12..]);
    digest.finalize()
}

/// A [`BytesSource`] implementation for `&'a mut [Bytes]`
///
/// The type allows to dequeue [`Bytes`] chunks from an array of chunks, up to
/// a configured limit.
pub struct BytesArray<'a> {
    /// The wrapped slice of `Bytes`
    chunks: &'a mut [Bytes],
    /// The amount of chunks consumed from this source
    consumed: usize,
    length: usize,
}

impl<'a> BytesArray<'a> {
    pub fn from_chunks(chunks: &'a mut [Bytes]) -> Self {
        let mut length = 0;
        for chunk in chunks.iter() {
            length += chunk.len();
        }

        Self {
            chunks,
            consumed: 0,
            length,
        }
    }
}

impl<'a> BytesSource for BytesArray<'a> {
    fn pop_chunk(&mut self, limit: usize) -> (Bytes, usize) {
        // The loop exists to skip empty chunks while still marking them as
        // consumed
        let mut chunks_consumed = 0;

        while self.consumed < self.chunks.len() {
            let chunk = &mut self.chunks[self.consumed];

            if chunk.len() <= limit {
                let chunk = std::mem::take(chunk);
                self.consumed += 1;
                chunks_consumed += 1;
                if chunk.is_empty() {
                    continue;
                }
                return (chunk, chunks_consumed);
            } else if limit > 0 {
                let chunk = chunk.split_to(limit);
                return (chunk, chunks_consumed);
            } else {
                break;
            }
        }

        (Bytes::new(), chunks_consumed)
    }

    fn has_remaining(&self) -> bool {
        self.consumed < self.length
    }

    fn remaining(&self) -> usize {
        self.length - self.consumed
    }
}

/// A [`BytesSource`] implementation for `&[u8]`
///
/// The type allows to dequeue a single [`Bytes`] chunk, which will be lazily
/// created from a reference. This allows to defer the allocation until it is
/// known how much data needs to be copied.
pub struct ByteSlice<'a> {
    /// The wrapped byte slice
    data: &'a [u8],
}

impl<'a> ByteSlice<'a> {
    pub fn from_slice(data: &'a [u8]) -> Self {
        Self { data }
    }
}

impl<'a> BytesSource for ByteSlice<'a> {
    fn pop_chunk(&mut self, limit: usize) -> (Bytes, usize) {
        let limit = limit.min(self.data.len());
        if limit == 0 {
            return (Bytes::new(), 0);
        }

        let chunk = Bytes::from(self.data[..limit].to_owned());
        self.data = &self.data[chunk.len()..];

        let chunks_consumed = if self.data.is_empty() { 1 } else { 0 };
        (chunk, chunks_consumed)
    }

    fn has_remaining(&self) -> bool {
        !self.data.is_empty()
    }

    fn remaining(&self) -> usize {
        self.data.len()
    }
}

/// A source of one or more buffers which can be converted into `Bytes` buffers on demand
///
/// The purpose of this data type is to defer conversion as long as possible,
/// so that no heap allocation is required in case no data is writable.
pub trait BytesSource {
    /// Returns the next chunk from the source of owned chunks.
    ///
    /// This method will consume parts of the source.
    /// Calling it will yield `Bytes` elements up to the configured `limit`.
    ///
    /// The method returns a tuple:
    /// - The first item is the yielded `Bytes` element. The element will be
    ///   empty if the limit is zero or no more data is available.
    /// - The second item returns how many complete chunks inside the source had
    ///   had been consumed. This can be less than 1, if a chunk inside the
    ///   source had been truncated in order to adhere to the limit. It can also
    ///   be more than 1, if zero-length chunks had been skipped.
    fn pop_chunk(&mut self, limit: usize) -> (Bytes, usize);

    fn has_remaining(&self) -> bool;

    fn remaining(&self) -> usize;
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
                assert!(
                    sna32lt(s1, s2f),
                    "s1 < s2 should be true: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    !sna32lt(s1, s2b),
                    "s1 < s2 should be false: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    !sna32gt(s1, s2f),
                    "s1 > s2 should be false: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    sna32gt(s1, s2b),
                    "s1 > s2 should be true: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    sna32lte(s1, s2f),
                    "s1 <= s2 should be true: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    !sna32lte(s1, s2b),
                    "s1 <= s2 should be false: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    !sna32gte(s1, s2f),
                    "s1 >= s2 should be fales: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    sna32gte(s1, s2b),
                    "s1 >= s2 should be true: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    sna32eq(s2b, s2b),
                    "s2 == s2 should be true: s2={} s2={}",
                    s2b,
                    s2b
                );
                assert!(
                    sna32lte(s2b, s2b),
                    "s2 == s2 should be true: s2={} s2={}",
                    s2b,
                    s2b
                );
                assert!(
                    sna32gte(s2b, s2b),
                    "s2 == s2 should be true: s2={} s2={}",
                    s2b,
                    s2b
                );
            }

            if let Some(s1add1) = s1.checked_add(1) {
                assert!(
                    !sna32eq(s1, s1add1),
                    "s1 == s1+1 should be false: s1={} s1+1={}",
                    s1,
                    s1add1
                );
            }

            if let Some(s1sub1) = s1.checked_sub(1) {
                assert!(
                    !sna32eq(s1, s1sub1),
                    "s1 == s1-1 hould be false: s1={} s1-1={}",
                    s1,
                    s1sub1
                );
            }

            assert!(
                sna32eq(s1, s1),
                "s1 == s1 should be true: s1={} s2={}",
                s1,
                s1
            );
            assert!(
                sna32lte(s1, s1),
                "s1 == s1 should be true: s1={} s2={}",
                s1,
                s1
            );

            assert!(
                sna32gte(s1, s1),
                "s1 == s1 should be true: s1={} s2={}",
                s1,
                s1
            );
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
                assert!(
                    sna16lt(s1, s2f),
                    "s1 < s2 should be true: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    !sna16lt(s1, s2b),
                    "s1 < s2 should be false: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    !sna16gt(s1, s2f),
                    "s1 > s2 should be fales: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    sna16gt(s1, s2b),
                    "s1 > s2 should be true: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    sna16lte(s1, s2f),
                    "s1 <= s2 should be true: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    !sna16lte(s1, s2b),
                    "s1 <= s2 should be false: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    !sna16gte(s1, s2f),
                    "s1 >= s2 should be fales: s1={} s2={}",
                    s1,
                    s2f
                );
                assert!(
                    sna16gte(s1, s2b),
                    "s1 >= s2 should be true: s1={} s2={}",
                    s1,
                    s2b
                );

                assert!(
                    sna16eq(s2b, s2b),
                    "s2 == s2 should be true: s2={} s2={}",
                    s2b,
                    s2b
                );
                assert!(
                    sna16lte(s2b, s2b),
                    "s2 == s2 should be true: s2={} s2={}",
                    s2b,
                    s2b
                );
                assert!(
                    sna16gte(s2b, s2b),
                    "s2 == s2 should be true: s2={} s2={}",
                    s2b,
                    s2b
                );
            }

            assert!(
                sna16eq(s1, s1),
                "s1 == s1 should be true: s1={} s2={}",
                s1,
                s1
            );

            if let Some(s1add1) = s1.checked_add(1) {
                assert!(
                    !sna16eq(s1, s1add1),
                    "s1 == s1+1 should be false: s1={} s1+1={}",
                    s1,
                    s1add1
                );
            }
            if let Some(s1sub1) = s1.checked_sub(1) {
                assert!(
                    !sna16eq(s1, s1sub1),
                    "s1 == s1-1 hould be false: s1={} s1-1={}",
                    s1,
                    s1sub1
                );
            }

            assert!(
                sna16lte(s1, s1),
                "s1 == s1 should be true: s1={} s2={}",
                s1,
                s1
            );
            assert!(
                sna16gte(s1, s1),
                "s1 == s1 should be true: s1={} s2={}",
                s1,
                s1
            );
        }

        Ok(())
    }
}
