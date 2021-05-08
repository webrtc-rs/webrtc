use bytes::Bytes;
use crc::{crc32, Hasher32};

const PADDING_MULTIPLE: usize = 4;

pub(crate) fn get_padding_size(len: usize) -> usize {
    (PADDING_MULTIPLE - (len % PADDING_MULTIPLE)) % PADDING_MULTIPLE
}

/// Allocate and zero this data once.
/// We need to use it for the checksum and don't want to allocate/clear each time.
pub(crate) static FOUR_ZEROES: Bytes = Bytes::from_static(&[0, 0, 0, 0]);

/// Fastest way to do a crc32 without allocating.
pub(crate) fn generate_packet_checksum(raw: &Bytes) -> u32 {
    let mut hasher = crc32::Digest::new(crc32::CASTAGNOLI);
    hasher.write(&raw[0..8]);
    hasher.write(&FOUR_ZEROES[..]);
    hasher.write(&raw[12..]);
    hasher.sum32()
}
