use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use stun::attributes::ATTR_DATA;
use stun::message::{Getter, Message, Setter};
use turn::proto::chandata::ChannelData;
use turn::proto::channum::{ChannelNumber, MIN_CHANNEL_NUMBER};
use turn::proto::data::Data;
use turn::proto::lifetime::Lifetime;

fn benchmark_chan_data(c: &mut Criterion) {
    {
        let buf = [64, 0, 0, 0, 0, 4, 0, 0, 1, 2, 3];
        c.bench_function("BenchmarkIsChannelData", |b| {
            b.iter(|| {
                assert!(ChannelData::is_channel_data(&buf));
            })
        });
    }

    {
        let mut d = ChannelData {
            data: vec![1, 2, 3, 4],
            number: ChannelNumber(MIN_CHANNEL_NUMBER + 1),
            raw: vec![],
        };
        c.bench_function("BenchmarkChannelData_Encode", |b| {
            b.iter(|| {
                d.encode();
                d.reset();
            })
        });
    }

    {
        let mut d = ChannelData {
            data: vec![1, 2, 3, 4],
            number: ChannelNumber(MIN_CHANNEL_NUMBER + 1),
            raw: vec![],
        };
        d.encode();
        let mut buf = vec![0u8; d.raw.len()];
        buf.copy_from_slice(&d.raw);
        c.bench_function("BenchmarkChannelData_Decode", |b| {
            b.iter(|| {
                d.reset();
                d.raw = buf.clone();
                d.decode().unwrap();
            })
        });
    }
}

fn benchmark_chan(c: &mut Criterion) {
    {
        let mut m = Message::new();
        c.bench_function("BenchmarkChannelNumber/AddTo", |b| {
            b.iter(|| {
                let n = ChannelNumber(12);
                n.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let expected = ChannelNumber(12);
        expected.add_to(&mut m).unwrap();
        let mut n = ChannelNumber::default();
        c.bench_function("BenchmarkChannelNumber/GetFrom", |b| {
            b.iter(|| {
                n.get_from(&m).unwrap();
                assert_eq!(n, expected);
            })
        });
    }
}

fn benchmark_data(c: &mut Criterion) {
    {
        let mut m = Message::new();
        let d = Data(vec![0u8; 10]);
        c.bench_function("BenchmarkData/AddTo", |b| {
            b.iter(|| {
                d.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let d = Data(vec![0u8; 10]);
        c.bench_function("BenchmarkData/AddToRaw", |b| {
            b.iter(|| {
                m.add(ATTR_DATA, &d.0);
                m.reset();
            })
        });
    }
}

fn benchmark_lifetime(c: &mut Criterion) {
    {
        let mut m = Message::new();
        let l = Lifetime(Duration::from_secs(1));
        c.bench_function("BenchmarkLifetime/AddTo", |b| {
            b.iter(|| {
                l.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let expected = Lifetime(Duration::from_secs(60));
        expected.add_to(&mut m).unwrap();
        let mut l = Lifetime::default();
        c.bench_function("BenchmarkLifetime/GetFrom", |b| {
            b.iter(|| {
                l.get_from(&m).unwrap();
                assert_eq!(l, expected);
            })
        });
    }
}

criterion_group!(
    benches,
    benchmark_chan_data,
    benchmark_chan,
    benchmark_data,
    benchmark_lifetime
);
criterion_main!(benches);
