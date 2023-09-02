//! # What are these tests?
//!
//! These tests ensure that regressions in the unmarshalling code are caught.
//!
//! They check all artifacts of the fuzzer that crashed this lib, and make sure they no longer crash the library.
//!
//! The content of the files is mostly garbage, but it triggers "interesting" behaviour in the unmarshalling code.
//! So if your change fails one of these tests you probably made an error somewhere.
//!
//! Sadly these tests cannot really tell you where your error is specifically outside the standard backtrace rust will provide to you. Sorry.

use bytes::Bytes;

#[test]
fn param_crash_artifacts() {
    for artifact in std::fs::read_dir("fuzz/artifacts/param").unwrap() {
        let artifact = artifact.unwrap();
        if artifact
            .file_name()
            .into_string()
            .unwrap()
            .starts_with("crash-")
        {
            let artifact = std::fs::read(artifact.path()).unwrap();
            crate::param::build_param(&Bytes::from(artifact)).ok();
        }
    }
}

#[test]
fn packet_crash_artifacts() {
    for artifact in std::fs::read_dir("fuzz/artifacts/packet").unwrap() {
        let artifact = artifact.unwrap();
        if artifact
            .file_name()
            .into_string()
            .unwrap()
            .starts_with("crash-")
        {
            let artifact = std::fs::read(artifact.path()).unwrap();
            crate::packet::Packet::unmarshal(&Bytes::from(artifact)).ok();
        }
    }
}
