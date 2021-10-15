use criterion::{criterion_group, criterion_main, Criterion};
use std::ops::{Add, Sub};
use std::time::Duration;
use stun::addr::{AlternateServer, MappedAddress};
use stun::agent::{noop_handler, Agent, TransactionId};
use stun::attributes::{ATTR_REALM, ATTR_USERNAME};
use stun::message::{Message, Setter};
use tokio::time::Instant;

// AGENT_COLLECT_CAP is initial capacity for Agent.Collect slices,
// sufficient to make function zero-alloc in most cases.
const AGENT_COLLECT_CAP: usize = 100;

fn benchmark_addr(c: &mut Criterion) {
    let mut m = Message::new();

    let ma_addr = MappedAddress {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    c.bench_function("BenchmarkMappedAddress_AddTo", |b| {
        b.iter(|| {
            ma_addr.add_to(&mut m).unwrap();
            m.reset();
        })
    });

    let as_addr = AlternateServer {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    c.bench_function("BenchmarkAlternateServer_AddTo", |b| {
        b.iter(|| {
            as_addr.add_to(&mut m).unwrap();
            m.reset();
        })
    });
}

fn benchmark_agent(c: &mut Criterion) {
    let deadline = Instant::now().add(Duration::from_secs(60 * 60 * 24));
    let gc_deadline = deadline.sub(Duration::from_secs(1));

    {
        let mut a = Agent::new(noop_handler());
        for _ in 0..AGENT_COLLECT_CAP {
            a.start(TransactionId::new(), deadline).unwrap();
        }

        c.bench_function("BenchmarkAgent_GC", |b| {
            b.iter(|| {
                a.collect(gc_deadline).unwrap();
            })
        });

        a.close().unwrap();
    }

    {
        let mut a = Agent::new(noop_handler());
        for _ in 0..AGENT_COLLECT_CAP {
            a.start(TransactionId::new(), deadline).unwrap();
        }

        let mut m = Message::new();
        m.build(&[Box::new(TransactionId::default())]).unwrap();
        c.bench_function("BenchmarkAgent_Process", |b| {
            b.iter(|| {
                a.process(m.clone()).unwrap();
            })
        });

        a.close().unwrap();
    }
}

fn benchmark_attributes(c: &mut Criterion) {
    {
        let m = Message::new();
        c.bench_function("BenchmarkMessage_GetNotFound", |b| {
            b.iter(|| {
                let _ = m.get(ATTR_REALM);
            })
        });
    }

    {
        let mut m = Message::new();
        m.add(ATTR_USERNAME, &[1, 2, 3, 4, 5, 6, 7]);
        c.bench_function("BenchmarkMessage_Get", |b| {
            b.iter(|| {
                let _ = m.get(ATTR_USERNAME);
            })
        });
    }
}

//TODO: add benchmark_client

criterion_group!(
    benches,
    benchmark_addr,
    benchmark_agent,
    benchmark_attributes,
);
criterion_main!(benches);
