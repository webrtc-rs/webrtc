use bytes::BytesMut;
use criterion::measurement::WallTime;
use criterion::{criterion_main, BenchmarkGroup, Criterion};
use util::Marshal;
use webrtc_srtp::{context::Context, protection_profile::ProtectionProfile};

const MASTER_KEY: &[u8] = &[
    96, 180, 31, 4, 119, 137, 128, 252, 75, 194, 252, 44, 63, 56, 61, 55,
];
const MASTER_SALT: &[u8] = &[247, 26, 49, 94, 99, 29, 79, 94, 5, 111, 252, 216, 62, 195];
const RAW_RTCP: &[u8] = &[
    0x81, 0xc8, 0x00, 0x0b, 0xca, 0xfe, 0xba, 0xbe, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
    0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
];

fn benchmark_encrypt_rtp_aes_128_cm_hmac_sha1(g: &mut BenchmarkGroup<WallTime>) {
    let mut ctx = Context::new(
        MASTER_KEY,
        MASTER_SALT,
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap();

    let mut pld = BytesMut::new();
    for i in 0..1200 {
        pld.extend_from_slice(&[i as u8]);
    }

    g.bench_function("Encrypt/RTP", |b| {
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

fn benchmark_decrypt_rtp_aes_128_cm_hmac_sha1(g: &mut BenchmarkGroup<WallTime>) {
    let mut setup_ctx = Context::new(
        MASTER_KEY,
        MASTER_SALT,
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap();

    let mut ctx = Context::new(
        MASTER_KEY,
        MASTER_SALT,
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap();

    let mut pld = BytesMut::new();
    for i in 0..1200 {
        pld.extend_from_slice(&[i as u8]);
    }

    g.bench_function("Decrypt/RTP", |b| {
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
                setup_ctx.encrypt_rtp(&pkt.marshal().unwrap()).unwrap()
            },
            |encrypted| ctx.decrypt_rtp(&encrypted).unwrap(),
            criterion::BatchSize::LargeInput,
        );
    });
}

fn benchmark_encrypt_rtcp_aes_128_cm_hmac_sha1(g: &mut BenchmarkGroup<WallTime>) {
    let mut ctx = Context::new(
        MASTER_KEY,
        MASTER_SALT,
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap();

    g.bench_function("Encrypt/RTCP", |b| {
        b.iter(|| {
            ctx.encrypt_rtcp(RAW_RTCP).unwrap();
        });
    });
}

fn benchmark_decrypt_rtcp_aes_128_cm_hmac_sha1(g: &mut BenchmarkGroup<WallTime>) {
    let encrypted = Context::new(
        MASTER_KEY,
        MASTER_SALT,
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap()
    .encrypt_rtcp(RAW_RTCP)
    .unwrap();

    let mut ctx = Context::new(
        MASTER_KEY,
        MASTER_SALT,
        ProtectionProfile::Aes128CmHmacSha1_80,
        None,
        None,
    )
    .unwrap();

    g.bench_function("Decrypt/RTCP", |b| {
        b.iter(|| ctx.decrypt_rtcp(&encrypted).unwrap());
    });
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("SRTP");

    benchmark_encrypt_rtp_aes_128_cm_hmac_sha1(&mut g);
    benchmark_decrypt_rtp_aes_128_cm_hmac_sha1(&mut g);
    benchmark_encrypt_rtcp_aes_128_cm_hmac_sha1(&mut g);
    benchmark_decrypt_rtcp_aes_128_cm_hmac_sha1(&mut g);

    g.finish();
}

criterion_main!(benches);
