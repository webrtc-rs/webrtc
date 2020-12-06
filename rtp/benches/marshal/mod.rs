use bytes::BytesMut;
use criterion::{criterion_group, Criterion};

fn marshal_benchmark(c: &mut Criterion) {
    let mut raw_pkt = BytesMut::from(
        vec![
            0x90, 0xe0, 0x69, 0x8f, 0xd9, 0xc2, 0x93, 0xda, 0x1c, 0x64, 0x27, 0x82, 0xBE, 0xDE,
            0x00, 0x01, 0x10, 0xAA, 0x20, 0xBB, // Payload
            0x98, 0x36, 0xbe, 0x88, 0x9e,
        ]
        .as_slice(),
    );

    c.bench_function("rtp::marshal", |b| {
        b.iter(|| {
            let mut p = rtp::packet::Packet::default();
            {
                p.unmarshal(&mut raw_pkt)
                    .expect("Error unmarshalling packet");
            }

            p.marshal().expect("Error marshalling data");
        })
    });
}

criterion_group!(marshal, marshal_benchmark);
