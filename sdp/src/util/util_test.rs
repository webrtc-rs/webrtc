use super::*;
use crate::description::common::*;
use crate::description::media::*;
use crate::description::session::*;

fn get_test_session_description() -> SessionDescription {
    SessionDescription{
        media_descriptions: vec![
            MediaDescription {
                media_name: MediaName {
                    media: "video".to_string(),
                    port: RangedPort {
                        value: 51372,
                        range: None,
                    },
                    protos: vec!["RTP".to_string(), "AVP".to_string()],
                    formats: vec!["120".to_string(), "121".to_string(), "126".to_string(), "97".to_string()],
                },
                attributes: vec![
                    Attribute::new("fmtp:126 profile-level-id=42e01f;level-asymmetry-allowed=1;packetization-mode=1".to_string(), None),
                    Attribute::new("fmtp:97 profile-level-id=42e01f;level-asymmetry-allowed=1".to_string(), None),
                    Attribute::new("fmtp:120 max-fs=12288;max-fr=60".to_string(), None),
                    Attribute::new("fmtp:121 max-fs=12288;max-fr=60".to_string(), None),
                    Attribute::new("rtpmap:120 VP8/90000".to_string(), None),
                    Attribute::new("rtpmap:121 VP9/90000".to_string(), None),
                    Attribute::new("rtpmap:126 H264/90000".to_string(), None),
                    Attribute::new("rtpmap:97 H264/90000".to_string(), None),
                    Attribute::new("rtcp-fb:97 ccm fir".to_string(), None),
                    Attribute::new("rtcp-fb:97 nack".to_string(), None),
                    Attribute::new("rtcp-fb:97 nack pli".to_string(), None),
                ],
                ..Default::default()
            },
		],
        ..Default::default()
	}
}

#[test]
fn test_get_payload_type_for_vp8() -> Result<()> {
    let tests = vec![
        (
            Codec {
                name: "VP8".to_string(),
                ..Default::default()
            },
            120,
        ),
        (
            Codec {
                name: "VP9".to_string(),
                ..Default::default()
            },
            121,
        ),
        (
            Codec {
                name: "H264".to_string(),
                fmtp: "profile-level-id=42e01f;level-asymmetry-allowed=1".to_string(),
                ..Default::default()
            },
            97,
        ),
        (
            Codec {
                name: "H264".to_string(),
                fmtp: "level-asymmetry-allowed=1;profile-level-id=42e01f".to_string(),
                ..Default::default()
            },
            97,
        ),
        (
            Codec {
                name: "H264".to_string(),
                fmtp: "profile-level-id=42e01f;level-asymmetry-allowed=1;packetization-mode=1"
                    .to_string(),
                ..Default::default()
            },
            126,
        ),
    ];

    for (codec, expected) in tests {
        let sdp = get_test_session_description();
        let actual = sdp.get_payload_type_for_codec(&codec)?;
        assert_eq!(actual, expected);
    }

    Ok(())
}

#[test]
fn test_get_codec_for_payload_type() -> Result<()> {
    let tests: Vec<(u8, Codec)> = vec![
        (
            120,
            Codec {
                payload_type: 120,
                name: "VP8".to_string(),
                clock_rate: 90000,
                fmtp: "max-fs=12288;max-fr=60".to_string(),
                ..Default::default()
            },
        ),
        (
            121,
            Codec {
                payload_type: 121,
                name: "VP9".to_string(),
                clock_rate: 90000,
                fmtp: "max-fs=12288;max-fr=60".to_string(),
                ..Default::default()
            },
        ),
        (
            126,
            Codec {
                payload_type: 126,
                name: "H264".to_string(),
                clock_rate: 90000,
                fmtp: "profile-level-id=42e01f;level-asymmetry-allowed=1;packetization-mode=1"
                    .to_string(),
                ..Default::default()
            },
        ),
        (
            97,
            Codec {
                payload_type: 97,
                name: "H264".to_string(),
                clock_rate: 90000,
                fmtp: "profile-level-id=42e01f;level-asymmetry-allowed=1".to_string(),
                rtcp_feedback: vec![
                    "ccm fir".to_string(),
                    "nack".to_string(),
                    "nack pli".to_string(),
                ],
                ..Default::default()
            },
        ),
    ];

    for (payload_type, expected) in &tests {
        let sdp = get_test_session_description();
        let actual = sdp.get_codec_for_payload_type(*payload_type)?;
        assert_eq!(actual, *expected);
    }

    Ok(())
}

#[test]
fn test_new_session_id() -> Result<()> {
    let mut min = 0x7FFFFFFFFFFFFFFFu64;
    let mut max = 0u64;
    for _ in 0..10000 {
        let r = new_session_id();

        if r > (1 << 63) - 1 {
            panic!("Session ID must be less than 2**64-1, got {r}")
        }
        if r < min {
            min = r
        }
        if r > max {
            max = r
        }
    }
    if min > 0x1000000000000000 {
        panic!("Value around lower boundary was not generated")
    }
    if max < 0x7000000000000000 {
        panic!("Value around upper boundary was not generated")
    }

    Ok(())
}
