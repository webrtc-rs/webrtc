use criterion::{criterion_group, criterion_main, Criterion};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::net::Ipv4Addr;
use std::ops::{Add, Sub};
use std::time::Duration;
use stun::addr::{AlternateServer, MappedAddress};
use stun::agent::{noop_handler, Agent, TransactionId};
use stun::attributes::{
    ATTR_CHANNEL_NUMBER, ATTR_DONT_FRAGMENT, ATTR_MESSAGE_INTEGRITY, ATTR_NONCE, ATTR_REALM,
    ATTR_SOFTWARE, ATTR_USERNAME, ATTR_XORMAPPED_ADDRESS,
};
use stun::error_code::{ErrorCode, ErrorCodeAttribute, CODE_STALE_NONCE};
use stun::fingerprint::{FINGERPRINT, FINGERPRINT_SIZE};
use stun::integrity::MessageIntegrity;
use stun::message::{
    Getter, Message, Setter, ATTRIBUTE_HEADER_SIZE, BINDING_REQUEST, MESSAGE_HEADER_SIZE,
};
use stun::textattrs::{Nonce, Realm, Software, Username};
use stun::uattrs::UnknownAttributes;
use stun::xoraddr::{xor_bytes, XorMappedAddress};
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

fn benchmark_error_code(c: &mut Criterion) {
    {
        let mut m = Message::new();
        c.bench_function("BenchmarkErrorCode_AddTo", |b| {
            b.iter(|| {
                let _ = CODE_STALE_NONCE.add_to(&mut m);
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let a = ErrorCodeAttribute {
            code: ErrorCode(404),
            reason: b"not found!".to_vec(),
        };
        c.bench_function("BenchmarkErrorCodeAttribute_AddTo", |b| {
            b.iter(|| {
                let _ = a.add_to(&mut m);
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let mut a = ErrorCodeAttribute {
            code: ErrorCode(404),
            reason: b"not found!".to_vec(),
        };
        let _ = a.add_to(&mut m);
        c.bench_function("BenchmarkErrorCodeAttribute_GetFrom", |b| {
            b.iter(|| {
                a.get_from(&m).unwrap();
            })
        });
    }
}

fn benchmark_fingerprint(c: &mut Criterion) {
    {
        let mut m = Message::new();
        let s = Software::new(ATTR_SOFTWARE, "software".to_owned());
        let addr = XorMappedAddress {
            ip: Ipv4Addr::new(213, 1, 223, 5).into(),
            port: 0,
        };
        let _ = addr.add_to(&mut m);
        let _ = s.add_to(&mut m);
        c.bench_function("BenchmarkFingerprint_AddTo", |b| {
            b.iter(|| {
                let _ = FINGERPRINT.add_to(&mut m);
                m.write_length();
                m.length -= (ATTRIBUTE_HEADER_SIZE + FINGERPRINT_SIZE) as u32;
                m.raw.drain(m.length as usize + MESSAGE_HEADER_SIZE..);
                m.attributes.0.drain(m.attributes.0.len() - 1..);
            })
        });
    }

    {
        let mut m = Message::new();
        let s = Software::new(ATTR_SOFTWARE, "software".to_owned());
        let addr = XorMappedAddress {
            ip: Ipv4Addr::new(213, 1, 223, 5).into(),
            port: 0,
        };
        let _ = addr.add_to(&mut m);
        let _ = s.add_to(&mut m);
        m.write_header();
        FINGERPRINT.add_to(&mut m).unwrap();
        m.write_header();
        c.bench_function("BenchmarkFingerprint_Check", |b| {
            b.iter(|| {
                FINGERPRINT.check(&m).unwrap();
            })
        });
    }
}

fn benchmark_message_build_overhead(c: &mut Criterion) {
    let t = BINDING_REQUEST;
    let username = Username::new(ATTR_USERNAME, "username".to_owned());
    let nonce = Nonce::new(ATTR_NONCE, "nonce".to_owned());
    let realm = Realm::new(ATTR_REALM, "example.org".to_owned());

    {
        let mut m = Message::new();
        c.bench_function("BenchmarkBuildOverhead/Build", |b| {
            b.iter(|| {
                let _ = m.build(&[
                    Box::new(username.clone()),
                    Box::new(nonce.clone()),
                    Box::new(realm.clone()),
                    Box::new(FINGERPRINT),
                ]);
            })
        });
    }

    {
        let mut m = Message::new();
        c.bench_function("BenchmarkBuildOverhead/Raw", |b| {
            b.iter(|| {
                m.reset();
                m.write_header();
                m.set_type(t);
                let _ = username.add_to(&mut m);
                let _ = nonce.add_to(&mut m);
                let _ = realm.add_to(&mut m);
                let _ = FINGERPRINT.add_to(&mut m);
            })
        });
    }
}

fn benchmark_message_integrity(c: &mut Criterion) {
    {
        let mut m = Message::new();
        let integrity = MessageIntegrity::new_short_term_integrity("password".to_owned());
        m.write_header();
        c.bench_function("BenchmarkMessageIntegrity_AddTo", |b| {
            b.iter(|| {
                m.write_header();
                integrity.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        m.raw = Vec::with_capacity(1024);
        let software = Software::new(ATTR_SOFTWARE, "software".to_owned());
        let _ = software.add_to(&mut m);
        let integrity = MessageIntegrity::new_short_term_integrity("password".to_owned());
        m.write_header();
        integrity.add_to(&mut m).unwrap();
        m.write_header();
        c.bench_function("BenchmarkMessageIntegrity_Check", |b| {
            b.iter(|| {
                integrity.check(&mut m).unwrap();
            })
        });
    }
}

fn benchmark_message(c: &mut Criterion) {
    {
        c.bench_function("BenchmarkMessage_Write", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessageType_Value", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_WriteTo", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_ReadFrom", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_ReadBytes", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkIsMessage", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_NewTransactionID", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessageFull", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessageFullHardcore", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_WriteHeader", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_CloneTo", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkMessage_AddTo", |b| b.iter(|| {}));
    }

    {
        c.bench_function("BenchmarkDecode", |b| b.iter(|| {}));
    }
}

fn benchmark_text_attributes(c: &mut Criterion) {
    {
        let mut m = Message::new();
        let u = Username::new(ATTR_USERNAME, "test".to_owned());
        c.bench_function("BenchmarkUsername_AddTo", |b| {
            b.iter(|| {
                u.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let mut u = Username::new(ATTR_USERNAME, "test".to_owned());
        u.add_to(&mut m).unwrap();
        c.bench_function("BenchmarkUsername_GetFrom", |b| {
            b.iter(|| {
                u.get_from(&m).unwrap();
                u.text.clear();
            })
        });
    }

    {
        let mut m = Message::new();
        let n = Nonce::new(ATTR_NONCE, "nonce".to_owned());
        c.bench_function("BenchmarkNonce_AddTo", |b| {
            b.iter(|| {
                n.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let nonce = String::from_utf8(vec![b'a'; 2048]).unwrap();
        let n = Nonce::new(ATTR_NONCE, nonce);
        c.bench_function("BenchmarkNonce_AddTo_BadLength", |b| {
            b.iter(|| {
                assert!(n.add_to(&mut m).is_err());
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let mut n = Nonce::new(ATTR_NONCE, "nonce".to_owned());
        n.add_to(&mut m).unwrap();
        c.bench_function("BenchmarkNonce_GetFrom", |b| {
            b.iter(|| {
                n.get_from(&m).unwrap();
            })
        });
    }
}

fn benchmark_unknown_attributes(c: &mut Criterion) {
    let mut m = Message::new();
    let a = UnknownAttributes(vec![
        ATTR_DONT_FRAGMENT,
        ATTR_CHANNEL_NUMBER,
        ATTR_REALM,
        ATTR_MESSAGE_INTEGRITY,
    ]);

    {
        c.bench_function("BenchmarkUnknownAttributes/AddTo", |b| {
            b.iter(|| {
                a.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        a.add_to(&mut m).unwrap();
        let mut attrs = UnknownAttributes(Vec::with_capacity(10));
        c.bench_function("BenchmarkUnknownAttributes/GetFrom", |b| {
            b.iter(|| {
                attrs.get_from(&m).unwrap();
                attrs.0.clear();
            })
        });
    }
}

fn benchmark_xor(c: &mut Criterion) {
    let mut r = StdRng::seed_from_u64(666);
    let mut a = [0u8; 1024];
    let mut d = [0u8; 1024];
    r.fill(&mut a);
    r.fill(&mut d);
    let mut dst = [0u8; 1024];
    c.bench_function("BenchmarkXOR", |b| {
        b.iter(|| {
            let _ = xor_bytes(&mut dst, &a, &d);
        })
    });
}

fn benchmark_xoraddr(c: &mut Criterion) {
    {
        let mut m = Message::new();
        let ip = "192.168.1.32".parse().unwrap();
        c.bench_function("BenchmarkXORMappedAddress_AddTo", |b| {
            b.iter(|| {
                let addr = XorMappedAddress { ip, port: 3654 };
                addr.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let transaction_id = base64::decode("jxhBARZwX+rsC6er").unwrap();

        m.transaction_id.0.copy_from_slice(&transaction_id);
        let addr_value = base64::encode("00019cd5f49f38ae").into_bytes();
        m.add(ATTR_XORMAPPED_ADDRESS, &addr_value);
        let mut addr = XorMappedAddress::default();
        c.bench_function("BenchmarkXORMappedAddress_GetFrom", |b| {
            b.iter(|| {
                addr.get_from(&m).unwrap();
            })
        });
    }
}

criterion_group!(
    benches,
    benchmark_addr,
    benchmark_agent,
    benchmark_attributes,
    //TODO: benchmark_client,
    benchmark_error_code,
    benchmark_fingerprint,
    benchmark_message_build_overhead,
    benchmark_message_integrity,
    benchmark_message,
    benchmark_text_attributes,
    benchmark_unknown_attributes,
    benchmark_xor,
    benchmark_xoraddr,
);
criterion_main!(benches);
