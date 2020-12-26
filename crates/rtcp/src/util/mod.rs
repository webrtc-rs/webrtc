mod util_test;

use util::Error;

// getPadding Returns the padding required to make the length a multiple of 4
pub(crate) fn get_padding(len: usize) -> usize {
    if len % 4 == 0 {
        0
    } else {
        4 - (len % 4)
    }
}

// set_nbits_of_uint16 will truncate the value to size, left-shift to start_index position and set
pub(crate) fn set_nbits_of_uint16(
    src: u16,
    size: u16,
    start_index: u16,
    mut val: u16,
) -> Result<u16, Error> {
    if start_index + size > 16 {
        return Err(Error::new("invalid size or start_index".to_owned()));
    }

    // truncate val to size bits
    val &= (1 << size) - 1;

    Ok(src | (val << (16 - size - start_index)))
}

// appendBit32 will left-shift and append n bits of val
pub(crate) fn append_nbits_to_uint32(src: u32, n: u32, val: u32) -> u32 {
    (src << n) | (val & (0xFFFFFFFF >> (32 - n)))
}

// getNBit get n bits from 1 byte, begin with a position
pub(crate) fn get_nbits_from_byte(b: u8, begin: u16, n: u16) -> u16 {
    let end_shift = 8 - (begin + n);
    let mask = (0xFF >> begin) & (0xFF << end_shift) as u8;
    (b & mask) as u16 >> end_shift
}

// get24BitFromBytes get 24bits from `[3]byte` slice
pub(crate) fn get_24bits_from_bytes(b: &[u8]) -> u32 {
    ((b[0] as u32) << 16) + ((b[1] as u32) << 8) + (b[2] as u32)
}
