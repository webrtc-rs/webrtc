use bytes::BytesMut;
use util::Marshal;
use webrtc_srtp::{context::Context, protection_profile::ProtectionProfile};

fn main() {
    let mut ctx = Context::new(
        &vec![0; 16],
        &vec![0; 14],
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    ).unwrap();
    let mut pld = BytesMut::new();
    for i in 0..1000 {
        pld.extend_from_slice(&[i as u8]);
    }
    let pkt = rtp::packet::Packet {
        header: rtp::header::Header {
            sequence_number: 322,
            ..Default::default()
        },
        payload: pld.into(),
    };
    let pkt_raw = pkt.marshal().unwrap();

    for _ in 0..10000 {
        ctx.encrypt_rtp(&pkt_raw).unwrap();
    }
}