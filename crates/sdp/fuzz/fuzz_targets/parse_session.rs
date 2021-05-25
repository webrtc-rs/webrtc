#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut cursor = std::io::Cursor::new(data);
    let _session = sdp::session_description::SessionDescription::unmarshal(&mut cursor);
});
