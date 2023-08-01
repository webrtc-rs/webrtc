use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, Criterion};
use util::Marshal;
use webrtc_srtp::{context::Context, protection_profile::ProtectionProfile};

fn benchmark_buffer(c: &mut Criterion) {
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

    let mut count = 1;

    c.bench_function("Benchmark context ", |b| {
        b.iter_batched(
            || {
                let pkt = rtp::packet::Packet {
                    header: rtp::header::Header {
                        sequence_number: count,
                        timestamp: count.into(),
                        extension_profile: 48862,
                        marker: true,
                        padding: false,
                        extension: true,
                        payload_type: 96,
                        ..Default::default()
                    },
                    payload: pld.clone().into(),
                };
                count += 1;
                pkt.marshal().unwrap()
            },
            |pkt_raw| {
                ctx.encrypt_rtp(&pkt_raw).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, benchmark_buffer);
criterion_main!(benches);
