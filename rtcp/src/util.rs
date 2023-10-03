use bytes::BufMut;

use crate::error::{Error, Result};

// returns the padding required to make the length a multiple of 4
pub(crate) fn get_padding_size(len: usize) -> usize {
    if len % 4 == 0 {
        0
    } else {
        4 - (len % 4)
    }
}

pub(crate) fn put_padding(mut buf: &mut [u8], len: usize) {
    let padding_size = get_padding_size(len);
    for i in 0..padding_size {
        if i == padding_size - 1 {
            buf.put_u8(padding_size as u8);
        } else {
            buf.put_u8(0);
        }
    }
}

// set_nbits_of_uint16 will truncate the value to size, left-shift to start_index position and set
pub(crate) fn set_nbits_of_uint16(
    src: u16,
    size: u16,
    start_index: u16,
    mut val: u16,
) -> Result<u16> {
    if start_index + size > 16 {
        return Err(Error::InvalidSizeOrStartIndex);
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_padding() -> Result<()> {
        let tests = vec![(0, 0), (1, 3), (2, 2), (3, 1), (4, 0), (100, 0), (500, 0)];

        for (n, p) in tests {
            assert_eq!(
                get_padding_size(n),
                p,
                "Test case returned wrong value for input {n}"
            );
        }

        Ok(())
    }

    #[test]
    fn test_set_nbits_of_uint16() -> Result<()> {
        let tests = vec![
            ("setOneBit", 0, 1, 8, 1, 128, None),
            ("setStatusVectorBit", 0, 1, 0, 1, 32768, None),
            ("setStatusVectorSecondBit", 32768, 1, 1, 1, 49152, None),
            (
                "setStatusVectorInnerBitsAndCutValue",
                49152,
                2,
                6,
                11111,
                49920,
                None,
            ),
            ("setRunLengthSecondTwoBit", 32768, 2, 1, 1, 40960, None),
            (
                "setOneBitOutOfBounds",
                32768,
                2,
                15,
                1,
                0,
                Some("invalid size or startIndex"),
            ),
        ];

        for (name, source, size, index, value, result, err) in tests {
            let res = set_nbits_of_uint16(source, size, index, value);
            if err.is_some() {
                assert!(res.is_err(), "setNBitsOfUint16 {name} : should be error");
            } else if let Ok(got) = res {
                assert_eq!(got, result, "setNBitsOfUint16 {name}");
            } else {
                panic!("setNBitsOfUint16 {name} :unexpected error result");
            }
        }

        Ok(())
    }
}
