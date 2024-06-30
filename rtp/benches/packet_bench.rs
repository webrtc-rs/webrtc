// Silence warning on `..Default::default()` with no effect:
#![allow(clippy::needless_update)]

use bytes::{Bytes, BytesMut};
use criterion::measurement::WallTime;
use criterion::{criterion_main, BenchmarkGroup, Criterion};
use rtp::header::*;
use rtp::packet::*;
use util::marshal::{Marshal, MarshalSize, Unmarshal};

fn benchmark_packet(g: &mut BenchmarkGroup<WallTime>) {
    let pkt = Packet {
        header: Header {
            extension: true,
            csrc: vec![1, 2],
            extension_profile: EXTENSION_PROFILE_TWO_BYTE,
            extensions: vec![
                Extension {
                    id: 1,
                    payload: Bytes::from_static(&[3, 4]),
                },
                Extension {
                    id: 2,
                    payload: Bytes::from_static(&[5, 6]),
                },
            ],
            ..Default::default()
        },
        payload: Bytes::from_static(&[0xFFu8; 15]), //vec![0x07, 0x08, 0x09, 0x0a], //MTU=1500
        ..Default::default()
    };
    let raw = pkt.marshal().unwrap();
    let buf = &mut raw.clone();
    let p = Packet::unmarshal(buf).unwrap();
    if pkt != p {
        panic!("marshal or unmarshal not correct: \npkt: {pkt:?} \nvs \np: {p:?}");
    }

    ///////////////////////////////////////////////////////////////////////////////////////////////
    let mut buf = BytesMut::with_capacity(pkt.marshal_size());
    buf.resize(pkt.marshal_size(), 0);
    // BenchmarkMarshalTo
    g.bench_function("marshal_to", |b| {
        b.iter(|| {
            let _ = pkt.marshal_to(&mut buf).unwrap();
        })
    });

    // BenchmarkMarshal
    g.bench_function("marshal", |b| {
        b.iter(|| {
            let _ = pkt.marshal().unwrap();
        })
    });

    // BenchmarkUnmarshal
    g.bench_function("unmarshal", |b| {
        b.iter(|| {
            let buf = &mut raw.clone();
            let _ = Packet::unmarshal(buf).unwrap();
        })
    });
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("RTP");

    benchmark_packet(&mut g);

    g.finish();
}

criterion_main!(benches);
