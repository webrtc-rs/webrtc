use bytes::BytesMut;
use criterion::{criterion_group, criterion_main, Criterion};
use util::Marshal;
use webrtc_srtp::{context::Context, protection_profile::ProtectionProfile};

fn benchmark_encrypt_rtp_aes_128_cm_hmac_sha1(c: &mut Criterion) {
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
    for i in 0..1200 {
        pld.extend_from_slice(&[i as u8]);
    }

    c.bench_function("Benchmark context ", |b| {
        let mut seq = 1;
        b.iter_batched(
            || {
                let pkt = rtp::packet::Packet {
                    header: rtp::header::Header {
                        sequence_number: seq,
                        timestamp: seq.into(),
                        extension_profile: 48862,
                        marker: true,
                        padding: false,
                        extension: true,
                        payload_type: 96,
                        ..Default::default()
                    },
                    payload: pld.clone().into(),
                };
                seq += 1;
                pkt.marshal().unwrap()
            },
            |pkt_raw| {
                ctx.encrypt_rtp(&pkt_raw).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, benchmark_encrypt_rtp_aes_128_cm_hmac_sha1);
criterion_main!(benches);
