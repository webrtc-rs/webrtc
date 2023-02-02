// Silence warning on complex types:
#![allow(clippy::type_complexity)]

use super::builder::*;
use super::header::*;
use super::name::*;
use super::parser::*;
use super::question::*;
use super::resource::{
    a::*, aaaa::*, cname::*, mx::*, ns::*, opt::*, ptr::*, soa::*, srv::*, txt::*, *,
};
use super::*;
use crate::error::*;

use std::collections::HashMap;

fn small_test_msg() -> Result<Message> {
    let name = Name::new("example.com.")?;
    Ok(Message {
        header: Header {
            response: true,
            authoritative: true,
            ..Default::default()
        },
        questions: vec![Question {
            name: name.clone(),
            typ: DnsType::A,
            class: DNSCLASS_INET,
        }],
        answers: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DnsType::A,
                class: DNSCLASS_INET,
                ..Default::default()
            },
            body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
        }],
        authorities: vec![Resource {
            header: ResourceHeader {
                name: name.clone(),
                typ: DnsType::A,
                class: DNSCLASS_INET,
                ..Default::default()
            },
            body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
        }],
        additionals: vec![Resource {
            header: ResourceHeader {
                name,
                typ: DnsType::A,
                class: DNSCLASS_INET,
                ..Default::default()
            },
            body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
        }],
    })
}

fn large_test_msg() -> Result<Message> {
    let name = Name::new("foo.bar.example.com.")?;
    Ok(Message {
        header: Header {
            response: true,
            authoritative: true,
            ..Default::default()
        },
        questions: vec![Question {
            name: name.clone(),
            typ: DnsType::A,
            class: DNSCLASS_INET,
        }],
        answers: vec![
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::A,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(AResource { a: [127, 0, 0, 1] })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::A,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(AResource { a: [127, 0, 0, 2] })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Aaaa,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(AaaaResource {
                    aaaa: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Cname,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(CnameResource {
                    cname: Name::new("alias.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Soa,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(SoaResource {
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
                    typ: DnsType::Ptr,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(PtrResource {
                    ptr: Name::new("ptr.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Mx,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(MxResource {
                    pref: 7,
                    mx: Name::new("mx.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Srv,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(SrvResource {
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
                    typ: DnsType::Ns,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(NsResource {
                    ns: Name::new("ns1.example.com.")?,
                })),
            },
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Ns,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(NsResource {
                    ns: Name::new("ns2.example.com.")?,
                })),
            },
        ],
        additionals: vec![
            Resource {
                header: ResourceHeader {
                    name: name.clone(),
                    typ: DnsType::Txt,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(TxtResource {
                    txt: vec!["So Long, and Thanks for All the Fish".into()],
                })),
            },
            Resource {
                header: ResourceHeader {
                    name,
                    typ: DnsType::Txt,
                    class: DNSCLASS_INET,
                    ..Default::default()
                },
                body: Some(Box::new(TxtResource {
                    txt: vec!["Hamster Huey and the Gooey Kablooie".into()],
                })),
            },
            Resource {
                header: must_edns0_resource_header(4096, 0xfe0 | (RCode::Success as u32), false)?,
                body: Some(Box::new(OptResource {
                    options: vec![DnsOption {
                        code: 10, // see RFC 7873
                        data: vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
                    }],
                })),
            },
        ],
    })
}

fn must_edns0_resource_header(l: u16, extrc: u32, d: bool) -> Result<ResourceHeader> {
    let mut h = ResourceHeader {
        class: DNSCLASS_INET,
        ..Default::default()
    };
    h.set_edns0(l, extrc, d)?;
    Ok(h)
}

#[test]
fn test_name_string() -> Result<()> {
    let want = "foo";
    let name = Name::new(want)?;
    assert_eq!(name.to_string(), want);

    Ok(())
}

#[test]
fn test_question_pack_unpack() -> Result<()> {
    let want = Question {
        name: Name::new(".")?,
        typ: DnsType::A,
        class: DNSCLASS_INET,
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
        "got from Parser.Question() = {got}, want = {want}"
    );

    Ok(())
}

#[test]
fn test_name() -> Result<()> {
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
        assert_eq!(ns, test, "got {name} = {ns}, want = {test}");
    }

    Ok(())
}

#[test]
fn test_name_pack_unpack() -> Result<()> {
    let tests: Vec<(&str, &str, Option<Error>)> = vec![
        ("", "", Some(Error::ErrNonCanonicalName)),
        (".", ".", None),
        ("google..com", "", Some(Error::ErrNonCanonicalName)),
        ("google.com", "", Some(Error::ErrNonCanonicalName)),
        ("google..com.", "", Some(Error::ErrZeroSegLen)),
        ("google.com.", "google.com.", None),
        (".google.com.", "", Some(Error::ErrZeroSegLen)),
        ("www..google.com.", "", Some(Error::ErrZeroSegLen)),
        ("www.google.com.", "www.google.com.", None),
    ];

    for (input, want, want_err) in tests {
        let input = Name::new(input)?;
        let result = input.pack(vec![], &mut Some(HashMap::new()), 0);
        if let Some(want_err) = want_err {
            if let Err(actual_err) = result {
                assert_eq!(actual_err, want_err);
            } else {
                panic!();
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
            "unpacking packing of {input}: got = {got}, want = {want}"
        );
    }

    Ok(())
}

#[test]
fn test_incompressible_name() -> Result<()> {
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
            Error::ErrCompressedSrv,
            err,
            "unpacking compressed incompressible name with pointers: got {}, want = {}",
            err,
            Error::ErrCompressedSrv
        );
    } else {
        panic!();
    }

    Ok(())
}

#[test]
fn test_header_unpack_error() -> Result<()> {
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
fn test_parser_start() -> Result<()> {
    let mut p = Parser::default();
    let result = p.start(&[]);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_resource_not_started() -> Result<()> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Parser<'_>) -> Result<()>>)> = vec![
        (
            "CNAMEResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "MXResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "NSResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "PTRResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "SOAResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "TXTResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "SRVResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "AResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
        (
            "AAAAResource",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.resource_body().map(|_| ()) }),
        ),
    ];

    for (name, test_fn) in tests {
        let mut p = Parser::default();
        if let Err(err) = test_fn(&mut p) {
            assert_eq!(err, Error::ErrNotStarted, "{name}");
        }
    }

    Ok(())
}

#[test]
fn test_srv_pack_unpack() -> Result<()> {
    let want = Box::new(SrvResource {
        priority: 8,
        weight: 9,
        port: 11,
        target: Name::new("srv.example.com.")?,
    });

    let b = want.pack(vec![], &mut None, 0)?;
    let mut got = SrvResource::default();
    got.unpack(&b, 0, 0)?;
    assert_eq!(got.to_string(), want.to_string(),);

    Ok(())
}

#[test]
fn test_dns_pack_unpack() -> Result<()> {
    let wants = vec![
        Message {
            header: Header::default(),
            questions: vec![Question {
                name: Name::new(".")?,
                typ: DnsType::Aaaa,
                class: DNSCLASS_INET,
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
        assert_eq!(got.to_string(), want.to_string());
    }

    Ok(())
}

#[test]
fn test_dns_append_pack_unpack() -> Result<()> {
    let wants = vec![
        Message {
            header: Header::default(),
            questions: vec![Question {
                name: Name::new(".")?,
                typ: DnsType::Aaaa,
                class: DNSCLASS_INET,
            }],
            answers: vec![],
            authorities: vec![],
            additionals: vec![],
        },
        large_test_msg()?,
    ];

    for mut want in wants {
        let mut b = vec![0; 2];
        b = want.append_pack(b)?;
        let mut got = Message::default();
        got.unpack(&b[2..])?;
        assert_eq!(got.to_string(), want.to_string());
    }

    Ok(())
}

#[test]
fn test_skip_all() -> Result<()> {
    let mut msg = large_test_msg()?;
    let buf = msg.pack()?;
    let mut p = Parser::default();
    p.start(&buf)?;

    for _ in 1..=3 {
        p.skip_all_questions()?;
    }
    for _ in 1..=3 {
        p.skip_all_answers()?;
    }
    for _ in 1..=3 {
        p.skip_all_authorities()?;
    }
    for _ in 1..=3 {
        p.skip_all_additionals()?;
    }

    Ok(())
}

#[test]
fn test_skip_each() -> Result<()> {
    let mut msg = small_test_msg()?;
    let buf = msg.pack()?;
    let mut p = Parser::default();
    p.start(&buf)?;

    //	{"SkipQuestion", p.SkipQuestion},
    //	{"SkipAnswer", p.SkipAnswer},
    //	{"SkipAuthority", p.SkipAuthority},
    //  {"SkipAdditional", p.SkipAdditional},

    p.skip_question()?;
    if let Err(err) = p.skip_question() {
        assert_eq!(err, Error::ErrSectionDone);
    } else {
        panic!("expected error, but got ok");
    }

    p.skip_answer()?;
    if let Err(err) = p.skip_answer() {
        assert_eq!(err, Error::ErrSectionDone);
    } else {
        panic!("expected error, but got ok");
    }

    p.skip_authority()?;
    if let Err(err) = p.skip_authority() {
        assert_eq!(err, Error::ErrSectionDone);
    } else {
        panic!("expected error, but got ok");
    }

    p.skip_additional()?;
    if let Err(err) = p.skip_additional() {
        assert_eq!(err, Error::ErrSectionDone);
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_skip_after_read() -> Result<()> {
    let mut msg = small_test_msg()?;
    let buf = msg.pack()?;
    let mut p = Parser::default();
    p.start(&buf)?;

    let tests: Vec<(&str, Box<dyn Fn(&mut Parser<'_>) -> Result<()>>)> = vec![
        (
            "Question",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.question().map(|_| ()) }),
        ),
        (
            "Answer",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.answer().map(|_| ()) }),
        ),
        (
            "Authority",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.authority().map(|_| ()) }),
        ),
        (
            "Additional",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.additional().map(|_| ()) }),
        ),
    ];

    for (name, read_fn) in tests {
        read_fn(&mut p)?;

        let result = match name {
            "Question" => p.skip_question(),
            "Answer" => p.skip_answer(),
            "Authority" => p.skip_authority(),
            _ => p.skip_additional(),
        };

        if let Err(err) = result {
            assert_eq!(err, Error::ErrSectionDone);
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_skip_not_started() -> Result<()> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Parser<'_>) -> Result<()>>)> = vec![
        (
            "SkipAllQuestions",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.skip_all_questions() }),
        ),
        (
            "SkipAllAnswers",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.skip_all_answers() }),
        ),
        (
            "SkipAllAuthorities",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.skip_all_authorities() }),
        ),
        (
            "SkipAllAdditionals",
            Box::new(|p: &mut Parser<'_>| -> Result<()> { p.skip_all_additionals() }),
        ),
    ];

    let mut p = Parser::default();
    for (name, test_fn) in tests {
        if let Err(err) = test_fn(&mut p) {
            assert_eq!(err, Error::ErrNotStarted);
        } else {
            panic!("{name} expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_too_many_records() -> Result<()> {
    let recs: usize = u16::MAX as usize + 1;
    let tests = vec![
        (
            "Questions",
            Message {
                questions: vec![Question::default(); recs],
                ..Default::default()
            },
            Error::ErrTooManyQuestions,
        ),
        (
            "Answers",
            Message {
                answers: {
                    let mut a = vec![];
                    for _ in 0..recs {
                        a.push(Resource::default());
                    }
                    a
                },
                ..Default::default()
            },
            Error::ErrTooManyAnswers,
        ),
        (
            "Authorities",
            Message {
                authorities: {
                    let mut a = vec![];
                    for _ in 0..recs {
                        a.push(Resource::default());
                    }
                    a
                },
                ..Default::default()
            },
            Error::ErrTooManyAuthorities,
        ),
        (
            "Additionals",
            Message {
                additionals: {
                    let mut a = vec![];
                    for _ in 0..recs {
                        a.push(Resource::default());
                    }
                    a
                },
                ..Default::default()
            },
            Error::ErrTooManyAdditionals,
        ),
    ];

    for (name, mut msg, want) in tests {
        if let Err(got) = msg.pack() {
            assert_eq!(
                got, want,
                "got Message.Pack() for {name} = {got}, want = {want}"
            )
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_very_long_txt() -> Result<()> {
    let mut str255 = String::new();
    for _ in 0..255 {
        str255.push('.');
    }

    let mut want = Resource {
        header: ResourceHeader {
            name: Name::new("foo.bar.example.com.")?,
            typ: DnsType::Txt,
            class: DNSCLASS_INET,
            ..Default::default()
        },
        body: Some(Box::new(TxtResource {
            txt: vec![
                "".to_owned(),
                "".to_owned(),
                "foo bar".to_owned(),
                "".to_owned(),
                "www.example.com".to_owned(),
                "www.example.com.".to_owned(),
                str255,
            ],
        })),
    };

    let buf = want.pack(vec![], &mut Some(HashMap::new()), 0)?;
    let mut got = Resource::default();
    let off = got.header.unpack(&buf, 0, 0)?;
    let (body, n) = unpack_resource_body(got.header.typ, &buf, off, got.header.length as usize)?;
    got.body = Some(body);
    assert_eq!(
        n,
        buf.len(),
        "unpacked different amount than packed: got = {}, want = {}",
        n,
        buf.len(),
    );
    assert_eq!(got.to_string(), want.to_string());

    Ok(())
}

#[test]
fn test_too_long_txt() -> Result<()> {
    let mut str256 = String::new();
    for _ in 0..256 {
        str256.push('.');
    }
    let rb = TxtResource { txt: vec![str256] };
    if let Err(err) = rb.pack(vec![], &mut Some(HashMap::new()), 0) {
        assert_eq!(err, Error::ErrStringTooLong);
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_start_error() -> Result<()> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Builder) -> Result<()>>)> = vec![
        (
            "Questions",
            Box::new(|b: &mut Builder| -> Result<()> { b.start_questions() }),
        ),
        (
            "Answers",
            Box::new(|b: &mut Builder| -> Result<()> { b.start_answers() }),
        ),
        (
            "Authorities",
            Box::new(|b: &mut Builder| -> Result<()> { b.start_authorities() }),
        ),
        (
            "Additionals",
            Box::new(|b: &mut Builder| -> Result<()> { b.start_additionals() }),
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
            Error::ErrNotStarted,
        ),
        (
            "sectionDone",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Done,
                    ..Default::default()
                }
            }),
            Error::ErrSectionDone,
        ),
    ];

    for (env_name, env_fn, env_err) in &envs {
        for (test_name, test_fn) in &tests {
            let mut b = env_fn();
            if let Err(got_err) = test_fn(&mut b) {
                assert_eq!(
                    got_err, *env_err,
                    "got Builder{env_name}.{test_name} = {got_err}, want = {env_err}"
                );
            } else {
                panic!("{env_name}.{test_name}expected error, but got ok");
            }
        }
    }

    Ok(())
}

#[test]
fn test_builder_resource_error() -> Result<()> {
    let tests: Vec<(&str, Box<dyn Fn(&mut Builder) -> Result<()>>)> = vec![
        (
            "CNAMEResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<CnameResource>::default()),
                })
            }),
        ),
        (
            "MXResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<MxResource>::default()),
                })
            }),
        ),
        (
            "NSResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<NsResource>::default()),
                })
            }),
        ),
        (
            "PTRResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<PtrResource>::default()),
                })
            }),
        ),
        (
            "SOAResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<SoaResource>::default()),
                })
            }),
        ),
        (
            "TXTResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<TxtResource>::default()),
                })
            }),
        ),
        (
            "SRVResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<SrvResource>::default()),
                })
            }),
        ),
        (
            "AResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<AResource>::default()),
                })
            }),
        ),
        (
            "AAAAResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<AaaaResource>::default()),
                })
            }),
        ),
        (
            "OPTResource",
            Box::new(|b: &mut Builder| -> Result<()> {
                b.add_resource(&mut Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<OptResource>::default()),
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
            Error::ErrNotStarted,
        ),
        (
            "sectionHeader",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Header,
                    ..Default::default()
                }
            }),
            Error::ErrNotStarted,
        ),
        (
            "sectionQuestions",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Questions,
                    ..Default::default()
                }
            }),
            Error::ErrNotStarted,
        ),
        (
            "sectionDone",
            Box::new(|| -> Builder {
                Builder {
                    section: Section::Done,
                    ..Default::default()
                }
            }),
            Error::ErrSectionDone,
        ),
    ];

    for (env_name, env_fn, env_err) in &envs {
        for (test_name, test_fn) in &tests {
            let mut b = env_fn();
            if let Err(got_err) = test_fn(&mut b) {
                assert_eq!(
                    got_err, *env_err,
                    "got Builder{env_name}.{test_name} = {got_err}, want = {env_err}"
                );
            } else {
                panic!("{env_name}.{test_name}expected error, but got ok");
            }
        }
    }

    Ok(())
}

#[test]
fn test_finish_error() -> Result<()> {
    let mut b = Builder::default();
    let want = Error::ErrNotStarted;
    if let Err(got) = b.finish() {
        assert_eq!(got, want, "got Builder.Finish() = {got}, want = {want}");
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_builder() -> Result<()> {
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
fn test_resource_pack() -> Result<()> {
    let tests = vec![
        (
            Message {
                questions: vec![Question {
                    name: Name::new(".")?,
                    typ: DnsType::Aaaa,
                    class: DNSCLASS_INET,
                }],
                answers: vec![Resource {
                    header: ResourceHeader::default(),
                    body: None,
                }],
                ..Default::default()
            },
            Error::ErrNilResourceBody,
        ),
        (
            Message {
                questions: vec![Question {
                    name: Name::new(".")?,
                    typ: DnsType::Aaaa,
                    class: DNSCLASS_INET,
                }],
                authorities: vec![Resource {
                    header: ResourceHeader::default(),
                    body: Some(Box::<NsResource>::default()),
                }],
                ..Default::default()
            },
            Error::ErrNonCanonicalName,
        ),
        (
            Message {
                questions: vec![Question {
                    name: Name::new(".")?,
                    typ: DnsType::A,
                    class: DNSCLASS_INET,
                }],
                additionals: vec![Resource {
                    header: ResourceHeader::default(),
                    body: None,
                }],
                ..Default::default()
            },
            Error::ErrNilResourceBody,
        ),
    ];

    for (mut m, want_err) in tests {
        if let Err(err) = m.pack() {
            assert_eq!(err, want_err);
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_resource_pack_length() -> Result<()> {
    let mut r = Resource {
        header: ResourceHeader {
            name: Name::new(".")?,
            typ: DnsType::A,
            class: DNSCLASS_INET,
            ..Default::default()
        },
        body: Some(Box::new(AResource { a: [127, 0, 0, 2] })),
    };

    let (hb, _) = r.header.pack(vec![], &mut None, 0)?;
    let buf = r.pack(vec![], &mut None, 0)?;

    let mut hdr = ResourceHeader::default();
    hdr.unpack(&buf, 0, 0)?;

    let (got, want) = (hdr.length as usize, buf.len() - hb.len());
    assert_eq!(got, want, "got hdr.Length = {got}, want = {want}");

    Ok(())
}

#[test]
fn test_option_pack_unpack() -> Result<()> {
    let tests = vec![
        (
            "without EDNS(0) options",
            vec![
                0x00, 0x00, 0x29, 0x10, 0x00, 0xfe, 0x00, 0x80, 0x00, 0x00, 0x00,
            ],
            Message {
                header: Header {
                    rcode: RCode::FormatError,
                    ..Default::default()
                },
                questions: vec![Question {
                    name: Name::new(".")?,
                    typ: DnsType::A,
                    class: DNSCLASS_INET,
                }],
                additionals: vec![Resource {
                    header: must_edns0_resource_header(
                        4096,
                        0xfe0 | RCode::FormatError as u32,
                        true,
                    )?,
                    body: Some(Box::<OptResource>::default()),
                }],
                ..Default::default()
            },
            //true,
            //0xfe0 | RCode::FormatError as u32,
        ),
        (
            "with EDNS(0) options",
            vec![
                0x00, 0x00, 0x29, 0x10, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x0c, 0x00,
                0x02, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x02, 0x12, 0x34,
            ],
            Message {
                header: Header {
                    rcode: RCode::ServerFailure,
                    ..Default::default()
                },
                questions: vec![Question {
                    name: Name::new(".")?,
                    typ: DnsType::Aaaa,
                    class: DNSCLASS_INET,
                }],
                additionals: vec![Resource {
                    header: must_edns0_resource_header(
                        4096,
                        0xff0 | RCode::ServerFailure as u32,
                        false,
                    )?,
                    body: Some(Box::new(OptResource {
                        options: vec![
                            DnsOption {
                                code: 12, // see RFC 7828
                                data: vec![0x00, 0x00],
                            },
                            DnsOption {
                                code: 11, // see RFC 7830
                                data: vec![0x12, 0x34],
                            },
                        ],
                    })),
                }],
                ..Default::default()
            },
            //dnssecOK: false,
            //extRCode: 0xff0 | RCodeServerFailure,
        ),
        (
            // Containing multiple OPT resources in a
            // message is invalid, but it's necessary for
            // protocol conformance testing.
            "with multiple OPT resources",
            vec![
                0x00, 0x00, 0x29, 0x10, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x0b, 0x00,
                0x02, 0x12, 0x34, 0x00, 0x00, 0x29, 0x10, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x06,
                0x00, 0x0c, 0x00, 0x02, 0x00, 0x00,
            ],
            Message {
                header: Header {
                    rcode: RCode::NameError,
                    ..Default::default()
                },
                questions: vec![Question {
                    name: Name::new(".")?,
                    typ: DnsType::Aaaa,
                    class: DNSCLASS_INET,
                }],
                additionals: vec![
                    Resource {
                        header: must_edns0_resource_header(
                            4096,
                            0xff0 | RCode::NameError as u32,
                            false,
                        )?,
                        body: Some(Box::new(OptResource {
                            options: vec![DnsOption {
                                code: 11, // see RFC 7830
                                data: vec![0x12, 0x34],
                            }],
                        })),
                    },
                    Resource {
                        header: must_edns0_resource_header(
                            4096,
                            0xff0 | RCode::NameError as u32,
                            false,
                        )?,
                        body: Some(Box::new(OptResource {
                            options: vec![DnsOption {
                                code: 12, // see RFC 7828
                                data: vec![0x00, 0x00],
                            }],
                        })),
                    },
                ],
                ..Default::default()
            },
        ),
    ];

    for (_tt_name, tt_w, mut tt_m) in tests {
        let w = tt_m.pack()?;

        assert_eq!(&w[w.len() - tt_w.len()..], &tt_w[..]);

        let mut m = Message::default();
        m.unpack(&w)?;

        let ms: Vec<String> = m.additionals.iter().map(|s| s.to_string()).collect();
        let tt_ms: Vec<String> = tt_m.additionals.iter().map(|s| s.to_string()).collect();
        assert_eq!(ms, tt_ms);
    }

    Ok(())
}
