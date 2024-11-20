#![no_main]
use libfuzzer_sys::fuzz_target;

use bytes::Bytes;
use webrtc_sctp::param::build_param;

fuzz_target!(|data: &[u8]| {
    let bytes = Bytes::from(data.to_vec());
    build_param(&bytes);
});
