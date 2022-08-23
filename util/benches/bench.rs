use criterion::async_executor::FuturesExecutor;
use criterion::{criterion_group, criterion_main, Criterion};

use webrtc_util::Buffer;

async fn buffer_write_then_read(times: u32) {
    let buffer = Buffer::new(0, 0);
    let mut packet: Vec<u8> = vec![0; 4];
    for _ in 0..times {
        buffer.write(&[0, 1]).await.unwrap();
        buffer.read(&mut packet, None).await.unwrap();
    }
}

fn benchmark_buffer(c: &mut Criterion) {
    ///////////////////////////////////////////////////////////////////////////////////////////////
    c.bench_function("Benchmark Buffer WriteThenRead 1", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| buffer_write_then_read(1));
    });

    c.bench_function("Benchmark Buffer WriteThenRead 10", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| buffer_write_then_read(10));
    });

    c.bench_function("Benchmark Buffer WriteThenRead 100", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| buffer_write_then_read(100));
    });
}

criterion_group!(benches, benchmark_buffer);
criterion_main!(benches);
