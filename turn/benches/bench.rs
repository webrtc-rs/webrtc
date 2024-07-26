use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{criterion_main, BenchmarkGroup, Criterion};
use stun::attributes::ATTR_DATA;
use stun::message::{Getter, Message, Setter};
use turn::proto::chandata::ChannelData;
use turn::proto::channum::{ChannelNumber, MIN_CHANNEL_NUMBER};
use turn::proto::data::Data;
use turn::proto::lifetime::Lifetime;

fn benchmark_chan_data(g: &mut BenchmarkGroup<WallTime>) {
    {
        let buf = [64, 0, 0, 0, 0, 4, 0, 0, 1, 2, 3];
        // BenchmarkIsChannelData
        g.bench_function("ChannelData/is_channel_data", |b| {
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
        // BenchmarkChannelData_Encode
        g.bench_function("ChannelData/encode", |b| {
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
        // BenchmarkChannelData_Decode
        g.bench_function("ChannelData/decode", |b| {
            b.iter(|| {
                d.reset();
                d.raw.clone_from(&buf);
                d.decode().unwrap();
            })
        });
    }
}

// BenchmarkChannelNumber
fn benchmark_chan(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        g.bench_function("ChannelNumber/add_to", |b| {
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
        g.bench_function("ChannelNumber/get_from", |b| {
            b.iter(|| {
                n.get_from(&m).unwrap();
                assert_eq!(n, expected);
            })
        });
    }
}

// BenchmarkData
fn benchmark_data(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        let d = Data(vec![0u8; 10]);
        g.bench_function("Data/add_to", |b| {
            b.iter(|| {
                d.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let d = Data(vec![0u8; 10]);
        g.bench_function("Data/add_to Raw", |b| {
            b.iter(|| {
                m.add(ATTR_DATA, &d.0);
                m.reset();
            })
        });
    }
}

// BenchmarkLifetime
fn benchmark_lifetime(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        let l = Lifetime(Duration::from_secs(1));
        g.bench_function("Lifetime/add_to", |b| {
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
        g.bench_function("Lifetime/get_from", |b| {
            b.iter(|| {
                l.get_from(&m).unwrap();
                assert_eq!(l, expected);
            })
        });
    }
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("TURN");

    benchmark_chan_data(&mut g);
    benchmark_chan(&mut g);
    benchmark_data(&mut g);
    benchmark_lifetime(&mut g);

    g.finish();
}

criterion_main!(benches);
