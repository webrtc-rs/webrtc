#![warn(rust_2018_idioms)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod buffer;
pub mod error;
pub mod fixed_big_int;
pub mod replay_detector;

pub use crate::buffer::Buffer;
pub use crate::error::Error;

pub fn overlapping_copy(dst: &mut [u8], src: &[u8]) {
    for i in 0..src.len() {
        if i >= dst.len() {
            break;
        }

        dst[i] = src[i];
    }
}
