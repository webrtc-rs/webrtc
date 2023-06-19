use criterion::{black_box, criterion_group, criterion_main, Criterion};
use webrtc_media::audio::buffer::layout::{Deinterleaved, Interleaved};
use webrtc_media::audio::buffer::Buffer;

fn benchmark_from(c: &mut Criterion) {
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

    c.bench_function("Buffer<T, Interleaved> => Buffer<T, Deinterleaved>", |b| {
        b.iter(|| {
            black_box(Buffer::<Sample, Interleaved>::from(
                deinterleaved_buffer.as_ref(),
            ));
        })
    });

    c.bench_function("Buffer<T, Deinterleaved> => Buffer<T, Interleaved>", |b| {
        b.iter(|| {
            black_box(Buffer::<Sample, Deinterleaved>::from(
                interleaved_buffer.as_ref(),
            ));
        })
    });
}

criterion_group!(benches, benchmark_from);
criterion_main!(benches);
