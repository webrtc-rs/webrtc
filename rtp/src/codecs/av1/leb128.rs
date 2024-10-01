use bytes::{BufMut, Bytes, BytesMut};

pub fn encode_leb128(mut val: u32) -> u32 {
    let mut b = 0;
    loop {
        b |= val & 0b_0111_1111;
        val >>= 7;
        if val != 0 {
            b |= 0b_1000_0000;
            b <<= 8;
        } else {
            return b;
        }
    }
}

pub fn decode_leb128(mut val: u64) -> u32 {
    let mut b = 0;
    loop {
        b |= val & 0b_0111_1111;
        val >>= 8;
        if val == 0 {
            return b as u32;
        }
        b <<= 7;
    }
}

pub fn read_leb128(bytes: &Bytes) -> (u32, usize) {
    let mut encoded = 0;
    for i in 0..bytes.len() {
        encoded |= bytes[i] as u64;
        if bytes[i] & 0b_1000_0000 == 0 {
            return (decode_leb128(encoded), i + 1);
        }
        encoded <<= 8;
    }
    (0, 0)
}

pub fn leb128_size(value: u32) -> usize {
    let mut size = 0;
    let mut value = value;
    while value >= 0b_1000_0000 {
        size += 1;
        value >>= 7;
    }
    size + 1
}

pub trait BytesMutExt {
    fn put_leb128(&mut self, n: u32);
}

impl BytesMutExt for BytesMut {
    fn put_leb128(&mut self, n: u32) {
        let mut encoded = encode_leb128(n);
        while encoded >= 0b_1000_0000 {
            self.put_u8(0b_1000_0000 | (encoded & 0b_0111_1111) as u8);
            encoded >>= 7;
        }
        self.put_u8(encoded as u8);
    }
}
