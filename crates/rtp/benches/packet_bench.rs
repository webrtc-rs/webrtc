use criterion::{criterion_group, criterion_main, Criterion};
use webrtc_rtp::{
    header::{self, ExtensionProfile, Header},
    packet::Packet,
};

fn benchmark_packet(c: &mut Criterion) {
    let mut pkt = Packet {
        header: Header {
            extension: true,
            csrc: vec![1, 2],
            extension_profile: ExtensionProfile::TwoByte as u16,
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
        payload: vec![0xFFu8; 1500], //MTU=1500
        ..Default::default()
    };

    let raw_pkt = pkt.marshal().unwrap();
    let raw_pkt_clone = raw_pkt.clone();
    let mut p = Packet::default();
    p.unmarshal(&raw_pkt_clone).unwrap();

    if pkt != p {
        panic!(
            "marshal or unmarshal not correct: \npkt: {:?} \nvs \np: {:?}",
            pkt, p
        );
    }

    c.bench_function("Marshal Benchmark", |b| b.iter(|| p.marshal().unwrap()));

    let buf = &mut [0u8; 1600];
    c.bench_function("Marshal_To Benchmark", move |b| {
        b.iter(|| p.marshal_to(buf).unwrap())
    });

    c.bench_function("Shared Struct", move |b| {
        b.iter(|| pkt.unmarshal(&raw_pkt).unwrap())
    });

    c.bench_function("New Struct", move |b| {
        b.iter(|| {
            let mut p = Packet::default();
            p.unmarshal(&raw_pkt_clone).unwrap();
        })
    });
}

criterion_group!(benches, benchmark_packet);
criterion_main!(benches);
