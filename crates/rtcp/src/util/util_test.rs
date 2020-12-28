#[cfg(test)]
mod test {
    use crate::errors::Error;
    use crate::util::{get_padding, set_nbits_of_uint16};

    #[test]
    fn test_get_padding() -> Result<(), Error> {
        let tests = vec![(0, 0), (1, 3), (2, 2), (3, 1), (4, 0), (100, 0), (500, 0)];

        for (n, p) in tests {
            assert_eq!(
                get_padding(n),
                p,
                "Test case returned wrong value for input {}",
                n
            );
        }

        Ok(())
    }

    #[test]
    fn test_set_nbits_of_uint16() -> Result<(), Error> {
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
            if let Some(_) = err {
                assert!(res.is_err(), "setNBitsOfUint16 {} : should be error", name);
            } else if let Ok(got) = res {
                assert_eq!(got, result, "setNBitsOfUint16 {}", name);
            } else {
                assert!(false, "setNBitsOfUint16 {} :unexpected error result", name);
            }
        }

        Ok(())
    }
}
