use bytes::Bytes;
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
    let pkt = rtp::packet::Packet {
        header: rtp::header::Header {
            sequence_number: 322,
            ..Default::default()
        },
        payload: Bytes::from_static(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05]),
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
