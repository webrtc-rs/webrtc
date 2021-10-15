use criterion::{criterion_group, criterion_main, Criterion};
use stun::message::{Message, Setter};
use stun::addr::{AlternateServer, MappedAddress};

fn benchmark_addr(c: &mut Criterion) {
    let mut m  = Message::new();

    let ma_addr = MappedAddress{
        ip:   "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    c.bench_function("BenchmarkMappedAddress_AddTo", |b| {
        b.iter(|| {
            ma_addr.add_to(&mut m).unwrap();
            m.reset();
        })
    });

    let as_addr = AlternateServer{
        ip:   "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    c.bench_function("BenchmarkAlternateServer_AddTo", |b| {
        b.iter(|| {
            as_addr.add_to(&mut m).unwrap();
            m.reset();
        })
    });
}

criterion_group!(benches, benchmark_addr);
criterion_main!(benches);
