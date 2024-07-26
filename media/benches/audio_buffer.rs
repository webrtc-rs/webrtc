use criterion::measurement::WallTime;
use criterion::{black_box, criterion_main, BenchmarkGroup, Criterion};
use webrtc_media::audio::buffer::layout::{Deinterleaved, Interleaved};
use webrtc_media::audio::buffer::Buffer;

fn benchmark_from(g: &mut BenchmarkGroup<WallTime>) {
    type Sample = i32;
    let channels = 4;
    let frames = 100_000;
    let deinterleaved_buffer: Buffer<Sample, Deinterleaved> = {
        let samples = (0..(channels * frames)).map(|i| i as i32).collect();
        Buffer::new(samples, channels)
    };
    let interleaved_buffer: Buffer<Sample, Interleaved> = {
        let samples = (0..(channels * frames)).map(|i| i as i32).collect();
        Buffer::new(samples, channels)
    };

    g.bench_function("Buffer/Interleaved to Deinterleaved", |b| {
        b.iter(|| {
            black_box(Buffer::<Sample, Interleaved>::from(
                deinterleaved_buffer.as_ref(),
            ));
        })
    });

    g.bench_function("Buffer/Deinterleaved to Interleaved", |b| {
        b.iter(|| {
            black_box(Buffer::<Sample, Deinterleaved>::from(
                interleaved_buffer.as_ref(),
            ));
        })
    });
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("Media");

    benchmark_from(&mut g);

    g.finish();
}

criterion_main!(benches);
