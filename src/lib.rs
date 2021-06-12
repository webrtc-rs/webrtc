#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//#[macro_use]
//extern crate lazy_static;

pub mod api;
pub mod data;
pub mod dtls;
pub mod error;
pub mod ice;
pub mod media;
pub mod peer;
pub mod policy;
pub mod rtp;
pub mod sctp;
pub mod sdp;
pub mod stats;
pub mod track;

pub(crate) const UNSPECIFIED_STR: &str = "Unspecified";
