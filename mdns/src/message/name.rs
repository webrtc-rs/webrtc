use std::collections::HashMap;
use std::fmt;

use crate::error::*;

const NAME_LEN: usize = 255;

// A Name is a non-encoded domain name. It is used instead of strings to avoid
// allocations.
#[derive(Default, PartialEq, Eq, Debug, Clone)]
pub struct Name {
    pub data: String,
}

// String implements fmt.Stringer.String.
impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data)
    }
}

impl Name {
    pub fn new(data: &str) -> Result<Self> {
        if data.len() > NAME_LEN {
            Err(Error::ErrCalcLen)
        } else {
            Ok(Name {
                data: data.to_owned(),
            })
        }
    }

    // pack appends the wire format of the Name to msg.
    //
    // Domain names are a sequence of counted strings split at the dots. They end
    // with a zero-length string. Compression can be used to reuse domain suffixes.
    //
    // The compression map will be updated with new domain suffixes. If compression
    // is nil, compression will not be used.
    pub fn pack(
        &self,
        mut msg: Vec<u8>,
        compression: &mut Option<HashMap<String, usize>>,
        compression_off: usize,
    ) -> Result<Vec<u8>> {
        let data = self.data.as_bytes();

        // Add a trailing dot to canonicalize name.
        if data.is_empty() || data[data.len() - 1] != b'.' {
            return Err(Error::ErrNonCanonicalName);
        }

        // Allow root domain.
        if data.len() == 1 && data[0] == b'.' {
            msg.push(0);
            return Ok(msg);
        }

        // Emit sequence of counted strings, chopping at dots.
        let mut begin = 0;
        for i in 0..data.len() {
            // Check for the end of the segment.
            if data[i] == b'.' {
                // The two most significant bits have special meaning.
                // It isn't allowed for segments to be long enough to
                // need them.
                if i - begin >= (1 << 6) {
                    return Err(Error::ErrSegTooLong);
                }

                // Segments must have a non-zero length.
                if i - begin == 0 {
                    return Err(Error::ErrZeroSegLen);
                }

                msg.push((i - begin) as u8);
                msg.extend_from_slice(&data[begin..i]);

                begin = i + 1;
                continue;
            }

            // We can only compress domain suffixes starting with a new
            // segment. A pointer is two bytes with the two most significant
            // bits set to 1 to indicate that it is a pointer.
            if i == 0 || data[i - 1] == b'.' {
                if let Some(compression) = compression {
                    let key: String = self.data.chars().skip(i).collect();
                    if let Some(ptr) = compression.get(&key) {
                        // Hit. Emit a pointer instead of the rest of
                        // the domain.
                        msg.push(((ptr >> 8) | 0xC0) as u8);
                        msg.push((ptr & 0xFF) as u8);
                        return Ok(msg);
                    }

                    // Miss. Add the suffix to the compression table if the
                    // offset can be stored in the available 14 bytes.
                    if msg.len() <= 0x3FFF {
                        compression.insert(key, msg.len() - compression_off);
                    }
                }
            }
        }

        msg.push(0);
        Ok(msg)
    }

    // unpack unpacks a domain name.
    pub fn unpack(&mut self, msg: &[u8], off: usize) -> Result<usize> {
        self.unpack_compressed(msg, off, true /* allowCompression */)
    }

    pub fn unpack_compressed(
        &mut self,
        msg: &[u8],
        off: usize,
        allow_compression: bool,
    ) -> Result<usize> {
        // curr_off is the current working offset.
        let mut curr_off = off;

        // new_off is the offset where the next record will start. Pointers lead
        // to data that belongs to other names and thus doesn't count towards to
        // the usage of this name.
        let mut new_off = off;

        // ptr is the number of pointers followed.
        let mut ptr = 0;

        // Name is a slice representation of the name data.
        let mut name = String::new(); //n.Data[:0]

        loop {
            if curr_off >= msg.len() {
                return Err(Error::ErrBaseLen);
            }
            let c = msg[curr_off];
            curr_off += 1;
            match c & 0xC0 {
                0x00 => {
                    // String segment
                    if c == 0x00 {
                        // A zero length signals the end of the name.
                        break;
                    }
                    let end_off = curr_off + c as usize;
                    if end_off > msg.len() {
                        return Err(Error::ErrCalcLen);
                    }
                    name.push_str(String::from_utf8(msg[curr_off..end_off].to_vec())?.as_str());
                    name.push('.');
                    curr_off = end_off;
                }
                0xC0 => {
                    // Pointer
                    if !allow_compression {
                        return Err(Error::ErrCompressedSrv);
                    }
                    if curr_off >= msg.len() {
                        return Err(Error::ErrInvalidPtr);
                    }
                    let c1 = msg[curr_off];
                    curr_off += 1;
                    if ptr == 0 {
                        new_off = curr_off;
                    }
                    // Don't follow too many pointers, maybe there's a loop.
                    ptr += 1;
                    if ptr > 10 {
                        return Err(Error::ErrTooManyPtr);
                    }
                    curr_off = ((c ^ 0xC0) as usize) << 8 | (c1 as usize);
                }
                _ => {
                    // Prefixes 0x80 and 0x40 are reserved.
                    return Err(Error::ErrReserved);
                }
            }
        }
        if name.is_empty() {
            name.push('.');
        }
        if name.len() > NAME_LEN {
            return Err(Error::ErrCalcLen);
        }
        self.data = name;
        if ptr == 0 {
            new_off = curr_off;
        }
        Ok(new_off)
    }

    pub(crate) fn skip(msg: &[u8], off: usize) -> Result<usize> {
        // new_off is the offset where the next record will start. Pointers lead
        // to data that belongs to other names and thus doesn't count towards to
        // the usage of this name.
        let mut new_off = off;

        loop {
            if new_off >= msg.len() {
                return Err(Error::ErrBaseLen);
            }
            let c = msg[new_off];
            new_off += 1;
            match c & 0xC0 {
                0x00 => {
                    if c == 0x00 {
                        // A zero length signals the end of the name.
                        break;
                    }
                    // literal string
                    new_off += c as usize;
                    if new_off > msg.len() {
                        return Err(Error::ErrCalcLen);
                    }
                }
                0xC0 => {
                    // Pointer to somewhere else in msg.

                    // Pointers are two bytes.
                    new_off += 1;

                    // Don't follow the pointer as the data here has ended.
                    break;
                }
                _ => {
                    // Prefixes 0x80 and 0x40 are reserved.
                    return Err(Error::ErrReserved);
                }
            }
        }

        Ok(new_off)
    }
}
