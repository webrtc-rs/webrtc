#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//#[macro_use]
//extern crate lazy_static;

pub mod api;
pub mod data;
mod dtls;
mod ice;
pub mod media;
pub mod peer;
pub mod policy;
mod rtcp;
mod rtp;
mod sctp;
mod sdp;
mod stats;
mod track;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
