use super::builder::*;
use super::header::*;
use super::name::*;
use super::question::*;
use super::resource::{
    a::*, aaaa::*, cname::*, mx::*, ns::*, opt::*, ptr::*, soa::*, srv::*, txt::*, *,
};
use super::*;
use crate::errors::*;

use std::collections::HashMap;
use util::Error;

fn small_test_msg() -> Result<Message, Error> {
    let name = Name::new("example.com.".to_owned())?;
    Ok(Message {
        header: Header {
            response: true,
            authoritative: true,
            ..Default::default()
        },
        questions: vec![Question {
            name: name.clone(),
            typ: DNSType::A,
            class: DNSClass::INET,
        }],
        answers: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DNSType::A,
                class: DNSClass::INET,
                ..Default::default()
            },
            body: Box::new(AResource { a: [127, 0, 0, 1] }),
        }],
        authorities: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DNSType::A,
                class: DNSClass::INET,
                ..Default::default()
            },
            body: Box::new(AResource { a: [127, 0, 0, 1] }),
        }],
        additionals: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DNSType::A,
                class: DNSClass::INET,
                ..Default::default()
            },
            body: Box::new(AResource { a: [127, 0, 0, 1] }),
        }],
    })
}

fn large_test_msg() -> Result<Message, Error> {
    let name = Name::new("foo.bar.example.com.".to_owned())?;
    Ok(Message {
        header: Header {
            response: true,
            authoritative: true,
            ..Default::default()
        },
        questions: vec![Question {
            name: name.clone(),
            typ: DNSType::A,
            class: DNSClass::INET,
        }],
        answers: vec![
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::A,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(AResource { a: [127, 0, 0, 1] }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::A,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(AResource { a: [127, 0, 0, 2] }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::AAAA,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(AAAAResource {
                    aaaa: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::CNAME,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(CNAMEResource {
                    cname: Name::new("alias.example.com.".to_owned())?,
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::SOA,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(SOAResource {
                    ns: Name::new("ns1.example.com.".to_owned())?,
                    mbox: Name::new("mb.example.com.".to_owned())?,
                    serial: 1,
                    refresh: 2,
                    retry: 3,
                    expire: 4,
                    min_ttl: 5,
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::PTR,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(PTRResource {
                    ptr: Name::new("ptr.example.com.".to_owned())?,
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::MX,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(MXResource {
                    pref: 7,
                    mx: Name::new("mx.example.com.".to_owned())?,
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::SRV,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(SRVResource {
                    priority: 8,
                    weight: 9,
                    port: 11,
                    target: Name::new("srv.example.com.".to_owned())?,
                }),
            },
        ],
        authorities: vec![
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::NS,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(NSResource {
                    ns: Name::new("ns1.example.com.".to_owned())?,
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::NS,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(NSResource {
                    ns: Name::new("ns2.example.com.".to_owned())?,
                }),
            },
        ],
        additionals: vec![
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::TXT,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(TXTResource {
                    txt: vec!["So Long, and Thanks for All the Fish".to_owned()],
                }),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::TXT,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Box::new(TXTResource {
                    txt: vec!["Hamster Huey and the Gooey Kablooie".to_owned()],
                }),
            },
            Resource {
                header: must_edns0_resource_header(4096, 0xfe0 | (RCode::Success as u32), false)?,
                body: Box::new(OPTResource {
                    options: vec![DNSOption {
                        code: 10, // see RFC 7873
                        data: vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
                    }],
                }),
            },
        ],
    })
}

fn must_edns0_resource_header(l: u16, extrc: u32, d: bool) -> Result<ResourceHeader, Error> {
    let mut h = ResourceHeader {
        class: DNSClass::INET,
        ..Default::default()
    };
    h.set_edns0(l, extrc, d)?;
    Ok(h)
}

#[test]
fn test_builder() -> Result<(), Error> {
    let mut msg = large_test_msg()?;
    let want = msg.pack()?;

    let mut b = Builder::new(&msg.header);
    b.enable_compression();

    b.start_questions()?;
    for q in &msg.questions {
        b.add_question(q)?;
    }

    b.start_answers()?;
    for r in &mut msg.answers {
        b.add_resource(r)?;
    }

    b.start_authorities()?;
    for r in &mut msg.authorities {
        b.add_resource(r)?;
    }

    b.start_additionals()?;
    for r in &mut msg.additionals {
        b.add_resource(r)?;
    }

    let got = b.finish()?;
    assert_eq!(
        got,
        want,
        "got.len()={}, want.len()={}",
        got.len(),
        want.len()
    );

    Ok(())
}

#[test]
fn test_name() -> Result<(), Error> {
    let tests = vec![
        "",
        ".",
        "google..com",
        "google.com",
        "google..com.",
        "google.com.",
        ".google.com.",
        "www..google.com.",
        "www.google.com.",
    ];

    for test in tests {
        let name = Name::new(test.to_owned())?;
        let ns = name.to_string();
        assert_eq!(ns, test, "got {} = {}, want = {}", name, ns, test);
    }

    Ok(())
}

#[test]
fn test_name_pack_unpack() -> Result<(), Error> {
    let tests = vec![
        ("", "", Some(ERR_NON_CANONICAL_NAME.to_owned())),
        (".", ".", None),
        ("google..com", "", Some(ERR_NON_CANONICAL_NAME.to_owned())),
        ("google.com", "", Some(ERR_NON_CANONICAL_NAME.to_owned())),
        ("google..com.", "", Some(ERR_ZERO_SEG_LEN.to_owned())),
        ("google.com.", "google.com.", None),
        (".google.com.", "", Some(ERR_ZERO_SEG_LEN.to_owned())),
        ("www..google.com.", "", Some(ERR_ZERO_SEG_LEN.to_owned())),
        ("www.google.com.", "www.google.com.", None),
    ];

    for (input, want, want_err) in tests {
        let input = Name::new(input.to_owned())?;
        let result = input.pack(vec![], &mut Some(HashMap::new()), 0);
        if let Some(want_err) = want_err {
            if let Err(actual_err) = result {
                assert_eq!(want_err, actual_err);
            } else {
                assert!(false);
            }
            continue;
        } else {
            assert!(result.is_ok());
        }

        let buf = result.unwrap();

        let want = Name::new(want.to_owned())?;

        let mut got = Name::default();
        let n = got.unpack(&buf, 0)?;
        assert_eq!(
            n,
            buf.len(),
            "unpacked different amount than packed for {}: got = {}, want = {}",
            input,
            n,
            buf.len(),
        );

        assert_eq!(
            got, want,
            "unpacking packing of {}: got = {}, want = {}",
            input, got, want
        );
    }

    Ok(())
}

#[test]
fn test_incompressible_name() -> Result<(), Error> {
    let name = Name::new("example.com.".to_owned())?;
    let mut compression = Some(HashMap::new());
    let buf = name.pack(vec![], &mut compression, 0)?;
    let buf = name.pack(buf, &mut compression, 0)?;
    let mut n1 = Name::default();
    let off = n1.unpack_compressed(&buf, 0, false /* allowCompression */)?;
    let mut n2 = Name::default();
    let result = n2.unpack_compressed(&buf, off, false /* allowCompression */);
    if let Err(err) = result {
        assert_eq!(
            err,
            ERR_COMPRESSED_SRV.to_owned(),
            "unpacking compressed incompressible name with pointers: got {}, want = {}",
            err,
            ERR_COMPRESSED_SRV.to_owned()
        );
    } else {
        assert!(false);
    }

    Ok(())
}
