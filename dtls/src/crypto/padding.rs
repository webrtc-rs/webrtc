use cbc::cipher::block_padding::{PadType, RawPadding, UnpadError};
use core::panic;

pub enum DtlsPadding {}
/// Reference: RFC5246, 6.2.3.2
impl RawPadding for DtlsPadding {
    const TYPE: PadType = PadType::Reversible;

    fn raw_pad(block: &mut [u8], pos: usize) {
        if pos >= block.len() {
            panic!("`pos` is bigger or equal to block size");
        }

        let padding_length = block.len() - pos - 1;
        if padding_length > 255 {
            panic!("block size is too big for DTLS");
        }

        set(&mut block[pos..], padding_length as u8);
    }

    fn raw_unpad(data: &[u8]) -> Result<&[u8], UnpadError> {
        let padding_length = data.last().copied().unwrap_or(1) as usize;
        if padding_length + 1 > data.len() {
            return Err(UnpadError);
        }

        let padding_begin = data.len() - padding_length - 1;

        if data[padding_begin..data.len() - 1]
            .iter()
            .any(|&byte| byte as usize != padding_length)
        {
            return Err(UnpadError);
        }

        Ok(&data[0..padding_begin])
    }
}

/// Sets all bytes in `dst` equal to `value`
#[inline(always)]
fn set(dst: &mut [u8], value: u8) {
    // SAFETY: we overwrite valid memory behind `dst`
    // note: loop is not used here because it produces
    // unnecessary branch which tests for zero-length slices
    unsafe {
        core::ptr::write_bytes(dst.as_mut_ptr(), value, dst.len());
    }
}

#[cfg(test)]
pub mod tests {
    use rand::Rng;

    use super::*;

    #[test]
    fn padding_length_is_amount_of_bytes_excluding_the_padding_length_itself() -> Result<(), ()> {
        for original_length in 0..128 {
            for padding_length in 0..(256 - original_length) {
                let mut block = vec![0; original_length + padding_length + 1];
                rand::thread_rng().fill(&mut block[0..original_length]);
                let original = block[0..original_length].to_vec();
                DtlsPadding::raw_pad(&mut block, original_length);

                for byte in block[original_length..].iter() {
                    assert_eq!(*byte as usize, padding_length);
                }
                assert_eq!(block[0..original_length], original);
            }
        }

        Ok(())
    }

    #[test]
    #[should_panic]
    fn full_block_is_padding_error() {
        for original_length in 0..256 {
            let mut block = vec![0; original_length];
            DtlsPadding::raw_pad(&mut block, original_length);
        }
    }

    #[test]
    #[should_panic]
    fn padding_length_bigger_than_255_is_a_pad_error() {
        let padding_length = 256;
        for original_length in 0..128 {
            let mut block = vec![0; original_length + padding_length + 1];
            DtlsPadding::raw_pad(&mut block, original_length);
        }
    }

    #[test]
    fn empty_block_is_unpadding_error() {
        let r = DtlsPadding::raw_unpad(&[]);
        assert!(r.is_err());
    }

    #[test]
    fn padding_too_big_for_block_is_unpadding_error() {
        let r = DtlsPadding::raw_unpad(&[1]);
        assert!(r.is_err());
    }

    #[test]
    fn one_of_the_padding_bytes_with_value_different_than_padding_length_is_unpadding_error() {
        for padding_length in 0..16 {
            for invalid_byte in 0..padding_length {
                let mut block = vec![0; padding_length + 1];
                DtlsPadding::raw_pad(&mut block, 0);

                assert_eq!(DtlsPadding::raw_unpad(&block).ok(), Some(&[][..]));
                block[invalid_byte] = (padding_length - 1) as u8;
                let r = DtlsPadding::raw_unpad(&block);
                assert!(r.is_err());
            }
        }
    }
}
