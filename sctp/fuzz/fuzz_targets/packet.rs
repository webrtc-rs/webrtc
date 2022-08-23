#![no_main]
use libfuzzer_sys::fuzz_target;

use webrtc_sctp::packet::Packet;
use bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    let bytes = Bytes::from(data.to_vec());
    Packet::unmarshal(&bytes);
});
