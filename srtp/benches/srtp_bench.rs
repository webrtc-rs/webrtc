use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, Criterion};
use util::Marshal;
use webrtc_srtp::{context::Context, protection_profile::ProtectionProfile};

fn benchmark_buffer(c: &mut Criterion) {
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

    c.bench_function("Benchmark context ", |b| {
        b.iter(|| {
            ctx.encrypt_rtp(&pkt_raw).unwrap();
        });
    });
}

criterion_group!(benches, benchmark_buffer);
criterion_main!(benches);
