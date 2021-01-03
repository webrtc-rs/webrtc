use super::builder::*;
use super::header::*;
use super::name::*;
use super::parser::*;
use super::question::*;
use super::resource::{
    a::*, aaaa::*, cname::*, mx::*, ns::*, opt::*, ptr::*, soa::*, srv::*, txt::*, *,
};
use super::*;
use crate::errors::*;

use std::collections::HashMap;
use util::Error;

fn small_test_msg() -> Result<Message, Error> {
    let name = Name::new("example.com.")?;
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
            body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
        }],
        authorities: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DNSType::A,
                class: DNSClass::INET,
                ..Default::default()
            },
            body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
        }],
        additionals: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DNSType::A,
                class: DNSClass::INET,
                ..Default::default()
            },
            body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
        }],
    })
}

fn large_test_msg() -> Result<Message, Error> {
    let name = Name::new("foo.bar.example.com.")?;
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
                body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::A,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(AResource { a: [127, 0, 0, 2] })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::AAAA,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(AAAAResource {
                    aaaa: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::CNAME,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(CNAMEResource {
                    cname: Name::new("alias.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::SOA,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(SOAResource {
                    ns: Name::new("ns1.example.com.")?,
                    mbox: Name::new("mb.example.com.")?,
                    serial: 1,
                    refresh: 2,
                    retry: 3,
                    expire: 4,
                    min_ttl: 5,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::PTR,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(PTRResource {
                    ptr: Name::new("ptr.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::MX,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(MXResource {
                    pref: 7,
                    mx: Name::new("mx.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::SRV,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(SRVResource {
                    priority: 8,
                    weight: 9,
                    port: 11,
                    target: Name::new("srv.example.com.")?,
                })),
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
                body: Some(Box::new(NSResource {
                    ns: Name::new("ns1.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::NS,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(NSResource {
                    ns: Name::new("ns2.example.com.")?,
                })),
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
                body: Some(Box::new(TXTResource {
                    txt: vec!["So Long, and Thanks for All the Fish".to_owned()],
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DNSType::TXT,
                    class: DNSClass::INET,
                    ..Default::default()
                },
                body: Some(Box::new(TXTResource {
                    txt: vec!["Hamster Huey and the Gooey Kablooie".to_owned()],
                })),
            },
            Resource {
                header: must_edns0_resource_header(4096, 0xfe0 | (RCode::Success as u32), false)?,
                body: Some(Box::new(OPTResource {
                    options: vec![DNSOption {
                        code: 10, // see RFC 7873
                        data: vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
                    }],
                })),
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
fn test_name_string() -> Result<(), Error> {
    let want = "foo";
    let name = Name::new(want)?;
    assert_eq!(name.to_string(), want);

    Ok(())
}

#[test]
fn test_question_pack_unpack() -> Result<(), Error> {
    let want = Question {
        name: Name::new(".")?,
        typ: DNSType::A,
        class: DNSClass::INET,
    };
    let buf = want.pack(vec![0; 1], &mut Some(HashMap::new()), 1)?;
    let mut p = Parser {
        msg: &buf,
        header: HeaderInternal {
            questions: 1,
            ..Default::default()
        },
        section: Section::Questions,
        off: 1,
        ..Default::default()
    };

    let got = p.question()?;
    assert_eq!(
        p.off,
        buf.len(),
        "unpacked different amount than packed: got = {}, want = {}",
        p.off,
        buf.len(),
    );
    assert_eq!(
        got, want,
        "got from Parser.Question() = {}, want = {}",
        got, want
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
        let name = Name::new(test)?;
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
        let input = Name::new(input)?;
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

        let want = Name::new(want)?;

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
    let name = Name::new("example.com.")?;
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

#[test]
fn test_header_unpack_error() -> Result<(), Error> {
    let wants = vec![
        "id",
        "bits",
        "questions",
        "answers",
        "authorities",
        "additionals",
    ];

    let mut buf = vec![];
    for want in wants {
        let mut h = HeaderInternal::default();
        let result = h.unpack(&buf, 0);
        assert!(result.is_err(), "{}", want);
        buf.extend_from_slice(&[0, 0]);
    }

    Ok(())
}

#[test]
fn test_parser_start() -> Result<(), Error> {
    let mut p = Parser::default();
    let result = p.start(&vec![]);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_resource_not_started() -> Result<(), Error> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Parser<'_>) -> Result<(), Error>>)> = vec![
        (
            "CNAMEResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "MXResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "NSResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "PTRResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "SOAResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "TXTResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "SRVResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "AResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
        (
            "AAAAResource",
            Box::new(|p: &mut Parser<'_>| -> Result<(), Error> {
                if let Err(err) = p.resource_body() {
                    Err(err)
                } else {
                    Ok(())
                }
            }),
        ),
    ];

    for (name, test_fn) in tests {
        let mut p = Parser::default();
        if let Err(err) = test_fn(&mut p) {
            assert_eq!(err, ERR_NOT_STARTED.to_owned(), "{}", name);
        }
    }

    Ok(())
}

#[test]
fn test_srv_pack_unpack() -> Result<(), Error> {
    let want = Box::new(SRVResource {
        priority: 8,
        weight: 9,
        port: 11,
        target: Name::new("srv.example.com.")?,
    });

    let b = want.pack(vec![], &mut None, 0)?;
    let mut got = SRVResource::default();
    got.unpack(&b, 0, 0)?;
    assert_eq!(got.to_string(), want.to_string(),);

    Ok(())
}

#[test]
fn test_dns_pack_unpack() -> Result<(), Error> {
    let wants = vec![
        Message {
            header: Header::default(),
            questions: vec![Question {
                name: Name::new(".")?,
                typ: DNSType::AAAA,
                class: DNSClass::INET,
            }],
            answers: vec![],
            authorities: vec![],
            additionals: vec![],
        },
        large_test_msg()?,
    ];

    for mut want in wants {
        let b = want.pack()?;
        let mut got = Message::default();
        got.unpack(&b)?;
        assert_eq!(got.to_string(), want.to_string(),);
    }

    Ok(())
}

#[test]
fn test_start_error() -> Result<(), Error> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Builder) -> Result<(), Error>>)> = vec![
        (
            "Questions",
            Box::new(|b: &mut Builder| -> Result<(), Error> { b.start_questions() }),
        ),
        (
            "Answers",
            Box::new(|b: &mut Builder| -> Result<(), Error> { b.start_answers() }),
        ),
        (
            "Authorities",
            Box::new(|b: &mut Builder| -> Result<(), Error> { b.start_authorities() }),
        ),
        (
            "Additionals",
            Box::new(|b: &mut Builder| -> Result<(), Error> { b.start_additionals() }),
        ),
    ];

    let envs: Vec<(&str, Box<dyn Fn() -> Builder>, Error)> = vec![
        (
            "sectionNotStarted",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::NotStarted,
                    ..Default::default()
                }
            }),
            ERR_NOT_STARTED.to_owned(),
        ),
        (
            "sectionDone",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Done,
                    ..Default::default()
                }
            }),
            ERR_SECTION_DONE.to_owned(),
        ),
    ];

    for (env_name, env_fn, env_err) in &envs {
        for (test_name, test_fn) in &tests {
            let mut b = env_fn();
            if let Err(got_err) = test_fn(&mut b) {
                assert_eq!(
                    got_err, *env_err,
                    "got Builder{}.{} = {}, want = {}",
                    env_name, test_name, got_err, *env_err
                );
            } else {
                assert!(
                    false,
                    "{}.{}expected error, but got ok",
                    env_name, test_name
                );
            }
        }
    }

    Ok(())
}

#[test]
fn test_builder_resource_error() -> Result<(), Error> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Builder) -> Result<(), Error>>)> = vec![
        (
            "CNAMEResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(CNAMEResource::default())),
                })
            }),
        ),
        (
            "MXResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(MXResource::default())),
                })
            }),
        ),
        (
            "NSResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(NSResource::default())),
                })
            }),
        ),
        (
            "PTRResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(PTRResource::default())),
                })
            }),
        ),
        (
            "SOAResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(SOAResource::default())),
                })
            }),
        ),
        (
            "TXTResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(TXTResource::default())),
                })
            }),
        ),
        (
            "SRVResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(SRVResource::default())),
                })
            }),
        ),
        (
            "AResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(AResource::default())),
                })
            }),
        ),
        (
            "AAAAResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(AAAAResource::default())),
                })
            }),
        ),
        (
            "OPTResource",
            Box::new(|b: &mut Builder| -> Result<(), Error> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::new(OPTResource::default())),
                })
            }),
        ),
    ];

    let envs: Vec<(&str, Box<dyn Fn() -> Builder>, Error)> = vec![
        (
            "sectionNotStarted",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::NotStarted,
                    ..Default::default()
                }
            }),
            ERR_NOT_STARTED.to_owned(),
        ),
        (
            "sectionHeader",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Header,
                    ..Default::default()
                }
            }),
            ERR_NOT_STARTED.to_owned(),
        ),
        (
            "sectionQuestions",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Questions,
                    ..Default::default()
                }
            }),
            ERR_NOT_STARTED.to_owned(),
        ),
        (
            "sectionDone",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Done,
                    ..Default::default()
                }
            }),
            ERR_SECTION_DONE.to_owned(),
        ),
    ];

    for (env_name, env_fn, env_err) in &envs {
        for (test_name, test_fn) in &tests {
            let mut b = env_fn();
            if let Err(got_err) = test_fn(&mut b) {
                assert_eq!(
                    got_err, *env_err,
                    "got Builder{}.{} = {}, want = {}",
                    env_name, test_name, got_err, *env_err
                );
            } else {
                assert!(
                    false,
                    "{}.{}expected error, but got ok",
                    env_name, test_name
                );
            }
        }
    }

    Ok(())
}

#[test]
fn test_finish_error() -> Result<(), Error> {
    let mut b = Builder::default();
    let want = ERR_NOT_STARTED.to_owned();
    if let Err(got) = b.finish() {
        assert_eq!(got, want, "got Builder.Finish() = {}, want = {}", got, want);
    } else {
        assert!(false, "expected error, but got ok");
    }

    Ok(())
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
