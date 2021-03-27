use criterion::{criterion_group, criterion_main, Criterion};
use webrtc_rs_rtp::{
    header::{self, ExtensionProfile, Header},
    packet::Packet,
};

fn benchmark_marshal(c: &mut Criterion) {
    let mut raw_pkt = vec![
        0x90u8, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];

    let mut p = Packet::default();
    p.unmarshal(raw_pkt.as_mut_slice()).unwrap();

    c.bench_function("Marshal Benchmark", |b| b.iter(|| p.marshal().unwrap()));
}

fn benchmark_marshal_to(c: &mut Criterion) {
    let mut raw_pkt = vec![
        0x90, 0x60, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0x00, 0x01, 0x00,
        0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x98, 0x36, 0xbe, 0x88, 0x9e,
    ];

    let mut p = Packet::default();
    p.unmarshal(&mut raw_pkt).unwrap();

    let buf = &mut [0u8; 100];

    c.bench_function("Marshal_To Benchmark", move |b| {
        b.iter(|| p.marshal_to(buf).unwrap())
    });
}

fn benchmark_unmarshal(c: &mut Criterion) {
    let mut pkt = Packet {
        header: Header {
            extension: true,
            csrc: vec![1, 2],
            extension_profile: ExtensionProfile::TwoByte.into(),
            extensions: vec![
                header::Extension {
                    id: 1,
                    payload: vec![3, 4],
                },
                header::Extension {
                    id: 2,
                    payload: vec![5, 6],
                },
            ],
            ..Default::default()
        },
        payload: vec![0x07, 0x08, 0x09, 0x0a],
        ..Default::default()
    };

    let mut raw_pkt = pkt.marshal().unwrap();
    let mut raw_pkt_clone = raw_pkt.clone();

    c.bench_function("Shared Struct", move |b| {
        b.iter(|| pkt.unmarshal(&mut raw_pkt).unwrap())
    });

    c.bench_function("New Struct", move |b| {
        b.iter(|| {
            let mut p = Packet::default();
            p.unmarshal(&mut raw_pkt_clone).unwrap();
        })
    });
}

criterion_group!(
    benches,
    benchmark_marshal,
    benchmark_marshal_to,
    benchmark_unmarshal
);
criterion_main!(benches);
