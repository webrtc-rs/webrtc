use bytes::BytesMut;
use util::Marshal;
use webrtc_srtp::{context::Context, protection_profile::ProtectionProfile};

fn main() {
    let mut ctx = Context::new(
        &vec![
            96, 180, 31, 4, 119, 137, 128, 252, 75, 194, 252, 44, 63, 56, 61, 55,
        ],
        &vec![247, 26, 49, 94, 99, 29, 79, 94, 5, 111, 252, 216, 62, 195],
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap();
    let mut pld = BytesMut::new();
    for i in 0..1000 {
        pld.extend_from_slice(&[i as u8]);
    }

    for i in 1..=100000u32 {
        let pkt = rtp::packet::Packet {
            header: rtp::header::Header {
                sequence_number: i as u16,
                timestamp: i,
                extension_profile: 48862,
                marker: true,
                padding: false,
                extension: true,
                payload_type: 96,
                ..Default::default()
            },
            payload: pld.clone().into(),
        };

        ctx.encrypt_rtp(&pkt.marshal().unwrap()).unwrap();
    }
}
