use std::io::Cursor;
use std::net::Ipv4Addr;
use std::ops::{Add, Sub};
use std::time::Duration;

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use criterion::measurement::WallTime;
use criterion::{criterion_main, BenchmarkGroup, Criterion};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use stun::addr::{AlternateServer, MappedAddress};
use stun::agent::{noop_handler, Agent, TransactionId};
use stun::attributes::{
    ATTR_CHANNEL_NUMBER, ATTR_DONT_FRAGMENT, ATTR_ERROR_CODE, ATTR_MESSAGE_INTEGRITY, ATTR_NONCE,
    ATTR_REALM, ATTR_SOFTWARE, ATTR_USERNAME, ATTR_XORMAPPED_ADDRESS,
};
use stun::error_code::{ErrorCode, ErrorCodeAttribute, CODE_STALE_NONCE};
use stun::fingerprint::{FINGERPRINT, FINGERPRINT_SIZE};
use stun::integrity::MessageIntegrity;
use stun::message::{
    is_message, Getter, Message, MessageType, Setter, ATTRIBUTE_HEADER_SIZE, BINDING_REQUEST,
    CLASS_REQUEST, MESSAGE_HEADER_SIZE, METHOD_BINDING,
};
use stun::textattrs::{Nonce, Realm, Software, Username};
use stun::uattrs::UnknownAttributes;
use stun::xoraddr::{xor_bytes, XorMappedAddress};
use tokio::time::Instant;

// AGENT_COLLECT_CAP is initial capacity for Agent.Collect slices,
// sufficient to make function zero-alloc in most cases.
const AGENT_COLLECT_CAP: usize = 100;

fn benchmark_addr(g: &mut BenchmarkGroup<WallTime>) {
    let mut m = Message::new();

    let ma_addr = MappedAddress {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    // BenchmarkMappedAddress_AddTo
    g.bench_function("MappedAddress/add_to", |b| {
        b.iter(|| {
            ma_addr.add_to(&mut m).unwrap();
            m.reset();
        })
    });

    let as_addr = AlternateServer {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    // BenchmarkAlternateServer_AddTo
    g.bench_function("AlternateServer/add_to", |b| {
        b.iter(|| {
            as_addr.add_to(&mut m).unwrap();
            m.reset();
        })
    });
}

fn benchmark_agent(g: &mut BenchmarkGroup<WallTime>) {
    let deadline = Instant::now().add(Duration::from_secs(60 * 60 * 24));
    let gc_deadline = deadline.sub(Duration::from_secs(1));

    {
        let mut a = Agent::new(noop_handler());
        for _ in 0..AGENT_COLLECT_CAP {
            a.start(TransactionId::new(), deadline).unwrap();
        }

        // BenchmarkAgent_GC
        g.bench_function("Agent/GC", |b| {
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
        m.build(&[Box::<TransactionId>::default()]).unwrap();
        // BenchmarkAgent_Process
        g.bench_function("Agent/process", |b| {
            b.iter(|| {
                a.process(m.clone()).unwrap();
            })
        });

        a.close().unwrap();
    }
}

fn benchmark_attributes(g: &mut BenchmarkGroup<WallTime>) {
    {
        let m = Message::new();
        // BenchmarkMessage_GetNotFound
        g.bench_function("Message/get (Not Found)", |b| {
            b.iter(|| {
                let _ = m.get(ATTR_REALM);
            })
        });
    }

    {
        let mut m = Message::new();
        m.add(ATTR_USERNAME, &[1, 2, 3, 4, 5, 6, 7]);
        // BenchmarkMessage_Get
        g.bench_function("Message/get", |b| {
            b.iter(|| {
                let _ = m.get(ATTR_USERNAME);
            })
        });
    }
}

//TODO: add benchmark_client

fn benchmark_error_code(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        // BenchmarkErrorCode_AddTo
        g.bench_function("ErrorCode/add_to", |b| {
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
        // BenchmarkErrorCodeAttribute_AddTo
        g.bench_function("ErrorCodeAttribute/add_to", |b| {
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
        // BenchmarkErrorCodeAttribute_GetFrom
        g.bench_function("ErrorCodeAttribute/get_from", |b| {
            b.iter(|| {
                a.get_from(&m).unwrap();
            })
        });
    }
}

fn benchmark_fingerprint(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        let s = Software::new(ATTR_SOFTWARE, "software".to_owned());
        let addr = XorMappedAddress {
            ip: Ipv4Addr::new(213, 1, 223, 5).into(),
            port: 0,
        };
        let _ = addr.add_to(&mut m);
        let _ = s.add_to(&mut m);
        // BenchmarkFingerprint_AddTo
        g.bench_function("Fingerprint/add_to", |b| {
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
        // BenchmarkFingerprint_Check
        g.bench_function("Fingerprint/check", |b| {
            b.iter(|| {
                FINGERPRINT.check(&m).unwrap();
            })
        });
    }
}

// BenchmarkBuildOverhead
fn benchmark_message_build_overhead(g: &mut BenchmarkGroup<WallTime>) {
    let t = BINDING_REQUEST;
    let username = Username::new(ATTR_USERNAME, "username".to_owned());
    let nonce = Nonce::new(ATTR_NONCE, "nonce".to_owned());
    let realm = Realm::new(ATTR_REALM, "example.org".to_owned());

    {
        let mut m = Message::new();
        g.bench_function("BuildOverhead/Build", |b| {
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
        g.bench_function("BuildOverhead/Raw", |b| {
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

fn benchmark_message_integrity(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        let integrity = MessageIntegrity::new_short_term_integrity("password".to_owned());
        m.write_header();
        // BenchmarkMessageIntegrity_AddTo
        g.bench_function("MessageIntegrity/add_to", |b| {
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
        // BenchmarkMessageIntegrity_Check
        g.bench_function("MessageIntegrity/check", |b| {
            b.iter(|| {
                integrity.check(&mut m).unwrap();
            })
        });
    }
}

fn benchmark_message(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        // BenchmarkMessage_Write
        g.bench_function("Message/write", |b| {
            b.iter(|| {
                m.add(ATTR_ERROR_CODE, &[0xff, 0x11, 0x12, 0x34]);
                m.transaction_id = TransactionId::new();
                m.typ = MessageType {
                    method: METHOD_BINDING,
                    class: CLASS_REQUEST,
                };
                m.write_header();
                m.reset();
            })
        });
    }

    {
        let m = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        // BenchmarkMessageType_Value
        g.bench_function("MessageType/value", |b| {
            b.iter(|| {
                let _ = m.value();
            })
        });
    }

    {
        let typ = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        let mut m = Message {
            typ,
            length: 0,
            transaction_id: TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
            ..Default::default()
        };
        m.write_header();
        let mut buf = vec![];
        // BenchmarkMessage_WriteTo
        g.bench_function("Message/write_to", |b| {
            b.iter(|| {
                {
                    let mut writer = Cursor::new(&mut buf);
                    m.write_to(&mut writer).unwrap();
                }
                buf.clear();
            })
        });
    }

    {
        let typ = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        let mut m = Message {
            typ,
            length: 0,
            transaction_id: TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
            ..Default::default()
        };
        m.write_header();
        let mut mrec = Message::new();
        // BenchmarkMessage_ReadFrom
        g.bench_function("Message/read_from", |b| {
            b.iter(|| {
                let mut reader = Cursor::new(&m.raw);
                mrec.read_from(&mut reader).unwrap();
                mrec.reset();
            })
        });
    }

    {
        let typ = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        let mut m = Message {
            typ,
            length: 0,
            transaction_id: TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
            ..Default::default()
        };
        m.write_header();
        let mut mrec = Message::new();
        // BenchmarkMessage_ReadBytes
        g.bench_function("Message/read_bytes", |b| {
            b.iter(|| {
                mrec.write(&m.raw).unwrap();
                mrec.reset();
            })
        });
    }

    {
        let typ = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        let mut m = Message {
            typ,
            transaction_id: TransactionId::new(),
            ..Default::default()
        };
        let software = Software::new(ATTR_SOFTWARE, "cydev/stun test".to_owned());
        software.add_to(&mut m).unwrap();
        m.write_header();
        // BenchmarkIsMessage
        g.bench_function("Message/is_message", |b| {
            b.iter(|| {
                assert!(is_message(&m.raw), "Should be message");
            })
        });
    }

    {
        let mut m = Message::new();
        m.write_header();
        // BenchmarkMessage_NewTransactionID
        g.bench_function("Message/new_transaction_id", |b| {
            b.iter(|| {
                m.new_transaction_id().unwrap();
            })
        });
    }

    {
        let mut m = Message::new();
        let s = Software::new(ATTR_SOFTWARE, "software".to_owned());
        let addr = XorMappedAddress {
            ip: Ipv4Addr::new(213, 1, 223, 5).into(),
            ..Default::default()
        };
        // BenchmarkMessageFull
        g.bench_function("Message/Full", |b| {
            b.iter(|| {
                addr.add_to(&mut m).unwrap();
                s.add_to(&mut m).unwrap();
                m.write_attributes();
                m.write_header();
                FINGERPRINT.add_to(&mut m).unwrap();
                m.write_header();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let s = Software::new(ATTR_SOFTWARE, "software".to_owned());
        let addr = XorMappedAddress {
            ip: Ipv4Addr::new(213, 1, 223, 5).into(),
            ..Default::default()
        };
        // BenchmarkMessageFullHardcore
        g.bench_function("Message/Full (Hardcore)", |b| {
            b.iter(|| {
                addr.add_to(&mut m).unwrap();
                s.add_to(&mut m).unwrap();
                m.write_header();
                m.reset();
            })
        });
    }

    {
        let typ = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        let mut m = Message {
            typ,
            transaction_id: TransactionId::new(),
            raw: vec![0u8; 128],
            ..Default::default()
        };
        // BenchmarkMessage_WriteHeader
        g.bench_function("Message/write_header", |b| {
            b.iter(|| {
                m.write_header();
            })
        });
    }

    {
        let mut m = Message::new();
        m.build(&[
            Box::new(BINDING_REQUEST),
            Box::new(TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2])),
            Box::new(Software::new(ATTR_SOFTWARE, "webrtc-rs/stun".to_owned())),
            Box::new(MessageIntegrity::new_long_term_integrity(
                "username".to_owned(),
                "realm".to_owned(),
                "password".to_owned(),
            )),
            Box::new(FINGERPRINT),
        ])
        .unwrap();
        let mut a = Message::new();
        m.clone_to(&mut a).unwrap();
        // BenchmarkMessage_CloneTo
        g.bench_function("Message/clone_to", |b| {
            b.iter(|| {
                m.clone_to(&mut a).unwrap();
            })
        });
    }

    {
        let mut m = Message::new();
        m.build(&[
            Box::new(BINDING_REQUEST),
            Box::new(TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2])),
            Box::new(FINGERPRINT),
        ])
        .unwrap();
        let mut a = Message::new();
        m.clone_to(&mut a).unwrap();
        // BenchmarkMessage_AddTo
        g.bench_function("Message/add_to", |b| {
            b.iter(|| {
                m.add_to(&mut a).unwrap();
            })
        });
    }

    {
        let typ = MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        };
        let mut m = Message {
            typ,
            transaction_id: TransactionId::new(),
            ..Default::default()
        };
        m.add(ATTR_ERROR_CODE, &[0xff, 0xfe, 0xfa]);
        m.write_header();
        let mut mdecoded = Message::new();
        // BenchmarkDecode
        g.bench_function("Message/decode", |b| {
            b.iter(|| {
                mdecoded.reset();
                mdecoded.raw.clone_from(&m.raw);
                mdecoded.decode().unwrap();
            })
        });
    }
}

fn benchmark_text_attributes(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        let u = Username::new(ATTR_USERNAME, "test".to_owned());
        // BenchmarkUsername_AddTo
        g.bench_function("Username/add_to", |b| {
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
        // BenchmarkUsername_GetFrom
        g.bench_function("Username/get_from", |b| {
            b.iter(|| {
                u.get_from(&m).unwrap();
                u.text.clear();
            })
        });
    }

    {
        let mut m = Message::new();
        let n = Nonce::new(ATTR_NONCE, "nonce".to_owned());
        // BenchmarkNonce_AddTo
        g.bench_function("Nonce/add_to", |b| {
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
        // BenchmarkNonce_AddTo_BadLength
        g.bench_function("Nonce/add_to (Bad Length)", |b| {
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
        // BenchmarkNonce_GetFrom
        g.bench_function("Nonce/get_from", |b| {
            b.iter(|| {
                n.get_from(&m).unwrap();
            })
        });
    }
}

// BenchmarkUnknownAttributes
fn benchmark_unknown_attributes(g: &mut BenchmarkGroup<WallTime>) {
    let mut m = Message::new();
    let a = UnknownAttributes(vec![
        ATTR_DONT_FRAGMENT,
        ATTR_CHANNEL_NUMBER,
        ATTR_REALM,
        ATTR_MESSAGE_INTEGRITY,
    ]);

    {
        g.bench_function("UnknownAttributes/add_to", |b| {
            b.iter(|| {
                a.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        a.add_to(&mut m).unwrap();
        let mut attrs = UnknownAttributes(Vec::with_capacity(10));
        g.bench_function("UnknownAttributes/get_from", |b| {
            b.iter(|| {
                attrs.get_from(&m).unwrap();
                attrs.0.clear();
            })
        });
    }
}

fn benchmark_xor(g: &mut BenchmarkGroup<WallTime>) {
    let mut r = StdRng::seed_from_u64(666);
    let mut a = [0u8; 1024];
    let mut d = [0u8; 1024];
    r.fill(&mut a);
    r.fill(&mut d);
    let mut dst = [0u8; 1024];
    // BenchmarkXOR
    g.bench_function("XOR", |b| {
        b.iter(|| {
            let _ = xor_bytes(&mut dst, &a, &d);
        })
    });
}

fn benchmark_xoraddr(g: &mut BenchmarkGroup<WallTime>) {
    {
        let mut m = Message::new();
        let ip = "192.168.1.32".parse().unwrap();
        // BenchmarkXORMappedAddress_AddTo
        g.bench_function("XorMappedAddress/add_to", |b| {
            b.iter(|| {
                let addr = XorMappedAddress { ip, port: 3654 };
                addr.add_to(&mut m).unwrap();
                m.reset();
            })
        });
    }

    {
        let mut m = Message::new();
        let transaction_id = BASE64_STANDARD.decode("jxhBARZwX+rsC6er").unwrap();

        m.transaction_id.0.copy_from_slice(&transaction_id);
        let addr_value = [0, 1, 156, 213, 244, 159, 56, 174]; //hex.DecodeString("00019cd5f49f38ae")
        m.add(ATTR_XORMAPPED_ADDRESS, &addr_value);
        let mut addr = XorMappedAddress::default();
        // BenchmarkXORMappedAddress_GetFrom
        g.bench_function("XorMappedAddress/get_from", |b| {
            b.iter(|| {
                addr.get_from(&m).unwrap();
            })
        });
    }
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("STUN");

    benchmark_addr(&mut g);
    benchmark_agent(&mut g);
    benchmark_attributes(&mut g);
    // TODO: benchmark_client(&mut g);
    benchmark_error_code(&mut g);
    benchmark_fingerprint(&mut g);
    benchmark_message_build_overhead(&mut g);
    benchmark_message_integrity(&mut g);
    benchmark_message(&mut g);
    benchmark_text_attributes(&mut g);
    benchmark_unknown_attributes(&mut g);
    benchmark_xor(&mut g);
    benchmark_xoraddr(&mut g);

    g.finish();
}

criterion_main!(benches);
