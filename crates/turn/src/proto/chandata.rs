#[cfg(test)]
mod chandata_test;

use super::channum::*;
use crate::error::*;

const PADDING: usize = 4;

fn nearest_padded_value_length(l: usize) -> usize {
    let mut n = PADDING * (l / PADDING);
    if n < l {
        n += PADDING;
    }
    n
}

const CHANNEL_DATA_LENGTH_SIZE: usize = 2;
const CHANNEL_DATA_NUMBER_SIZE: usize = CHANNEL_DATA_LENGTH_SIZE;
const CHANNEL_DATA_HEADER_SIZE: usize = CHANNEL_DATA_LENGTH_SIZE + CHANNEL_DATA_NUMBER_SIZE;

// ChannelData represents The ChannelData Message.
//
// See RFC 5766 Section 11.4
#[derive(Default, Debug)]
pub struct ChannelData {
    pub data: Vec<u8>, // can be subslice of Raw
    pub number: ChannelNumber,
    pub raw: Vec<u8>,
}

impl PartialEq for ChannelData {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.number == other.number
    }
}

impl ChannelData {
    // grow ensures that internal buffer will fit v more bytes and
    // increases it capacity if necessary.
    //
    // Similar to stun.Message.grow method.
    fn grow(&mut self, v: usize) {
        let n = self.raw.len() + v;
        self.raw.extend_from_slice(&vec![0; n - self.raw.len()]);
    }

    // Reset resets Length, Data and Raw length.
    pub fn reset(&mut self) {
        self.raw.clear();
        self.data.clear();
    }

    // Encode encodes ChannelData Message to Raw.
    pub fn encode(&mut self) {
        self.raw.clear();
        self.write_header();
        self.raw.extend_from_slice(&self.data);
        let padded = nearest_padded_value_length(self.raw.len());
        let bytes_to_add = padded - self.raw.len();
        if bytes_to_add > 0 {
            self.raw.extend_from_slice(&vec![0; bytes_to_add]);
        }
    }

    // Decode decodes The ChannelData Message from Raw.
    pub fn decode(&mut self) -> Result<()> {
        let buf = &self.raw;
        if buf.len() < CHANNEL_DATA_HEADER_SIZE {
            return Err(Error::ErrUnexpectedEof);
        }
        let num = u16::from_be_bytes([buf[0], buf[1]]);
        self.number = ChannelNumber(num);
        if !self.number.valid() {
            return Err(Error::ErrInvalidChannelNumber);
        }
        let l = u16::from_be_bytes([
            buf[CHANNEL_DATA_NUMBER_SIZE],
            buf[CHANNEL_DATA_NUMBER_SIZE + 1],
        ]) as usize;
        if l > buf[CHANNEL_DATA_HEADER_SIZE..].len() {
            return Err(Error::ErrBadChannelDataLength);
        }
        self.data = buf[CHANNEL_DATA_HEADER_SIZE..CHANNEL_DATA_HEADER_SIZE + l].to_vec();

        Ok(())
    }

    // WriteHeader writes channel number and length.
    pub fn write_header(&mut self) {
        if self.raw.len() < CHANNEL_DATA_HEADER_SIZE {
            // Making WriteHeader call valid even when c.Raw
            // is nil or len(c.Raw) is less than needed for header.
            self.grow(CHANNEL_DATA_HEADER_SIZE);
        }
        self.raw[..CHANNEL_DATA_NUMBER_SIZE].copy_from_slice(&self.number.0.to_be_bytes());
        self.raw[CHANNEL_DATA_NUMBER_SIZE..CHANNEL_DATA_HEADER_SIZE]
            .copy_from_slice(&(self.data.len() as u16).to_be_bytes());
    }

    // is_channel_data returns true if buf looks like the ChannelData Message.
    pub fn is_channel_data(buf: &[u8]) -> bool {
        if buf.len() < CHANNEL_DATA_HEADER_SIZE {
            return false;
        }

        if u16::from_be_bytes([
            buf[CHANNEL_DATA_NUMBER_SIZE],
            buf[CHANNEL_DATA_NUMBER_SIZE + 1],
        ]) > buf[CHANNEL_DATA_HEADER_SIZE..].len() as u16
        {
            return false;
        }

        // Quick check for channel number.
        let num = ChannelNumber(u16::from_be_bytes([buf[0], buf[1]]));
        num.valid()
    }
}
