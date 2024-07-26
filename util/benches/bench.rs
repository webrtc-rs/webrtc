use criterion::async_executor::FuturesExecutor;
use criterion::measurement::WallTime;
use criterion::{criterion_main, BenchmarkGroup, Criterion};
use webrtc_util::Buffer;

async fn buffer_write_then_read(times: u32) {
    let buffer = Buffer::new(0, 0);
    let mut packet: Vec<u8> = vec![0; 4];
    for _ in 0..times {
        buffer.write(&[0, 1]).await.unwrap();
        buffer.read(&mut packet, None).await.unwrap();
    }
}

fn benchmark_buffer(g: &mut BenchmarkGroup<WallTime>) {
    ///////////////////////////////////////////////////////////////////////////////////////////////

    // Benchmark Buffer WriteThenRead 1
    g.bench_function("Buffer/Write then read x1", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| buffer_write_then_read(1));
    });

    // Benchmark Buffer WriteThenRead 10
    g.bench_function("Buffer/Write then read x10", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| buffer_write_then_read(10));
    });

    // Benchmark Buffer WriteThenRead 100
    g.bench_function("Buffer/Write then read x100", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| buffer_write_then_read(100));
    });
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("Util");

    benchmark_buffer(&mut g);

    g.finish();
}

criterion_main!(benches);
