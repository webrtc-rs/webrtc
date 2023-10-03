use std::io::{BufReader, BufWriter};

use super::*;
use crate::fingerprint::FINGERPRINT;
use crate::integrity::MessageIntegrity;
use crate::textattrs::TextAttribute;
use crate::xoraddr::*;

#[test]
fn test_message_buffer() -> Result<()> {
    let mut m = Message::new();
    m.typ = MessageType {
        method: METHOD_BINDING,
        class: CLASS_REQUEST,
    };
    m.transaction_id = TransactionId::new();
    m.add(ATTR_ERROR_CODE, &[0xff, 0xfe, 0xfa]);
    m.write_header();

    let mut m_decoded = Message::new();
    let mut reader = BufReader::new(m.raw.as_slice());
    m_decoded.read_from(&mut reader)?;

    assert_eq!(m_decoded, m, "{m_decoded} != {m}");

    Ok(())
}

#[test]
fn test_message_type_value() -> Result<()> {
    let tests = vec![
        (
            MessageType {
                method: METHOD_BINDING,
                class: CLASS_REQUEST,
            },
            0x0001,
        ),
        (
            MessageType {
                method: METHOD_BINDING,
                class: CLASS_SUCCESS_RESPONSE,
            },
            0x0101,
        ),
        (
            MessageType {
                method: METHOD_BINDING,
                class: CLASS_ERROR_RESPONSE,
            },
            0x0111,
        ),
        (
            MessageType {
                method: Method(0xb6d),
                class: MessageClass(0x3),
            },
            0x2ddd,
        ),
    ];

    for (input, output) in tests {
        let b = input.value();
        assert_eq!(b, output, "Value({input}) -> {b}, want {output}");
    }

    Ok(())
}

#[test]
fn test_message_type_read_value() -> Result<()> {
    let tests = vec![
        (
            0x0001,
            MessageType {
                method: METHOD_BINDING,
                class: CLASS_REQUEST,
            },
        ),
        (
            0x0101,
            MessageType {
                method: METHOD_BINDING,
                class: CLASS_SUCCESS_RESPONSE,
            },
        ),
        (
            0x0111,
            MessageType {
                method: METHOD_BINDING,
                class: CLASS_ERROR_RESPONSE,
            },
        ),
    ];

    for (input, output) in tests {
        let mut m = MessageType::default();
        m.read_value(input);
        assert_eq!(m, output, "ReadValue({input}) -> {m}, want {output}");
    }

    Ok(())
}

#[test]
fn test_message_type_read_write_value() -> Result<()> {
    let tests = vec![
        MessageType {
            method: METHOD_BINDING,
            class: CLASS_REQUEST,
        },
        MessageType {
            method: METHOD_BINDING,
            class: CLASS_SUCCESS_RESPONSE,
        },
        MessageType {
            method: METHOD_BINDING,
            class: CLASS_ERROR_RESPONSE,
        },
        MessageType {
            method: Method(0x12),
            class: CLASS_ERROR_RESPONSE,
        },
    ];

    for test in tests {
        let mut m = MessageType::default();
        let v = test.value();
        m.read_value(v);
        assert_eq!(m, test, "ReadValue({test} -> {v}) = {m}, should be {test}");
    }

    Ok(())
}

#[test]
fn test_message_write_to() -> Result<()> {
    let mut m = Message::new();
    m.typ = MessageType {
        method: METHOD_BINDING,
        class: CLASS_REQUEST,
    };
    m.transaction_id = TransactionId::new();
    m.add(ATTR_ERROR_CODE, &[0xff, 0xfe, 0xfa]);
    m.write_header();
    let mut buf = vec![];
    {
        let mut writer = BufWriter::<&mut Vec<u8>>::new(buf.as_mut());
        m.write_to(&mut writer)?;
    }

    let mut m_decoded = Message::new();
    let mut reader = BufReader::new(buf.as_slice());
    m_decoded.read_from(&mut reader)?;
    assert_eq!(m_decoded, m, "{m_decoded} != {m}");

    Ok(())
}

#[test]
fn test_message_cookie() -> Result<()> {
    let buf = vec![0; 20];
    let mut m_decoded = Message::new();
    let mut reader = BufReader::new(buf.as_slice());
    let result = m_decoded.read_from(&mut reader);
    assert!(result.is_err(), "should error");

    Ok(())
}

#[test]
fn test_message_length_less_header_size() -> Result<()> {
    let buf = vec![0; 8];
    let mut m_decoded = Message::new();
    let mut reader = BufReader::new(buf.as_slice());
    let result = m_decoded.read_from(&mut reader);
    assert!(result.is_err(), "should error");

    Ok(())
}

#[test]
fn test_message_bad_length() -> Result<()> {
    let m_type = MessageType {
        method: METHOD_BINDING,
        class: CLASS_REQUEST,
    };
    let mut m = Message {
        typ: m_type,
        length: 4,
        transaction_id: TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
        ..Default::default()
    };
    m.add(AttrType(0x1), &[1, 2]);
    m.write_header();
    m.raw[20 + 3] = 10; // set attr length = 10

    let mut m_decoded = Message::new();
    let result = m_decoded.write(&m.raw);
    assert!(result.is_err(), "should error");

    Ok(())
}

#[test]
fn test_message_attr_length_less_than_header() -> Result<()> {
    let m_type = MessageType {
        method: METHOD_BINDING,
        class: CLASS_REQUEST,
    };
    let message_attribute = RawAttribute {
        length: 2,
        value: vec![1, 2],
        typ: AttrType(0x1),
    };
    let message_attributes = Attributes(vec![message_attribute]);
    let mut m = Message {
        typ: m_type,
        transaction_id: TransactionId::new(),
        attributes: message_attributes,
        ..Default::default()
    };
    m.encode();

    let mut m_decoded = Message::new();
    m.raw[3] = 2; // rewrite to bad length

    let mut reader = BufReader::new(&m.raw[..20 + 2]);
    let result = m_decoded.read_from(&mut reader);
    assert!(result.is_err(), "should be error");

    Ok(())
}

#[test]
fn test_message_attr_size_less_than_length() -> Result<()> {
    let m_type = MessageType {
        method: METHOD_BINDING,
        class: CLASS_REQUEST,
    };
    let message_attribute = RawAttribute {
        length: 4,
        value: vec![1, 2, 3, 4],
        typ: AttrType(0x1),
    };
    let message_attributes = Attributes(vec![message_attribute]);
    let mut m = Message {
        typ: m_type,
        transaction_id: TransactionId::new(),
        attributes: message_attributes,
        ..Default::default()
    };
    m.write_attributes();
    m.write_header();
    m.raw[3] = 5; // rewrite to bad length

    let mut m_decoded = Message::new();
    let mut reader = BufReader::new(&m.raw[..20 + 5]);
    let result = m_decoded.read_from(&mut reader);
    assert!(result.is_err(), "should be error");

    Ok(())
}

#[test]
fn test_message_read_from_error() -> Result<()> {
    let mut m_decoded = Message::new();
    let buf = vec![];
    let mut reader = BufReader::new(buf.as_slice());
    let result = m_decoded.read_from(&mut reader);
    assert!(result.is_err(), "should be error");

    Ok(())
}

#[test]
fn test_message_class_string() -> Result<()> {
    let v = vec![
        CLASS_REQUEST,
        CLASS_ERROR_RESPONSE,
        CLASS_SUCCESS_RESPONSE,
        CLASS_INDICATION,
    ];

    for k in v {
        if k.to_string() == *"unknown message class" {
            panic!("bad stringer {k}");
        }
    }

    // should panic
    let p = MessageClass(0x05).to_string();
    assert_eq!(p, "unknown message class", "should be error {p}");

    Ok(())
}

#[test]
fn test_attr_type_string() -> Result<()> {
    let v = vec![
        ATTR_MAPPED_ADDRESS,
        ATTR_USERNAME,
        ATTR_ERROR_CODE,
        ATTR_MESSAGE_INTEGRITY,
        ATTR_UNKNOWN_ATTRIBUTES,
        ATTR_REALM,
        ATTR_NONCE,
        ATTR_XORMAPPED_ADDRESS,
        ATTR_SOFTWARE,
        ATTR_ALTERNATE_SERVER,
        ATTR_FINGERPRINT,
    ];
    for k in v {
        assert!(!k.to_string().starts_with("0x"), "bad stringer");
    }

    let v_non_standard = AttrType(0x512);
    assert!(
        v_non_standard.to_string().starts_with("0x512"),
        "bad prefix"
    );

    Ok(())
}

#[test]
fn test_method_string() -> Result<()> {
    assert_eq!(
        METHOD_BINDING.to_string(),
        "Binding".to_owned(),
        "binding is not binding!"
    );
    assert_eq!(
        Method(0x616).to_string(),
        "0x616".to_owned(),
        "Bad stringer {}",
        Method(0x616)
    );

    Ok(())
}

#[test]
fn test_attribute_equal() -> Result<()> {
    let a = RawAttribute {
        length: 2,
        value: vec![0x1, 0x2],
        ..Default::default()
    };
    let b = RawAttribute {
        length: 2,
        value: vec![0x1, 0x2],
        ..Default::default()
    };
    assert_eq!(a, b, "should equal");

    assert_ne!(
        a,
        RawAttribute {
            typ: AttrType(0x2),
            ..Default::default()
        },
        "should not equal"
    );
    assert_ne!(
        a,
        RawAttribute {
            length: 0x2,
            ..Default::default()
        },
        "should not equal"
    );
    assert_ne!(
        a,
        RawAttribute {
            length: 0x3,
            ..Default::default()
        },
        "should not equal"
    );
    assert_ne!(
        a,
        RawAttribute {
            length: 0x2,
            value: vec![0x1, 0x3],
            ..Default::default()
        },
        "should not equal"
    );

    Ok(())
}

#[test]
fn test_message_equal() -> Result<()> {
    let attr = RawAttribute {
        length: 2,
        value: vec![0x1, 0x2],
        typ: AttrType(0x1),
    };
    let attrs = Attributes(vec![attr]);
    let a = Message {
        attributes: attrs.clone(),
        length: 4 + 2,
        ..Default::default()
    };
    let b = Message {
        attributes: attrs.clone(),
        length: 4 + 2,
        ..Default::default()
    };
    assert_eq!(a, b, "should equal");
    assert_ne!(
        a,
        Message {
            typ: MessageType {
                class: MessageClass(128),
                ..Default::default()
            },
            ..Default::default()
        },
        "should not equal"
    );

    let t_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);

    assert_ne!(
        a,
        Message {
            transaction_id: t_id,
            ..Default::default()
        },
        "should not equal"
    );
    assert_ne!(
        a,
        Message {
            length: 3,
            ..Default::default()
        },
        "should not equal"
    );

    let t_attrs = Attributes(vec![RawAttribute {
        length: 1,
        value: vec![0x1],
        typ: AttrType(0x1),
    }]);
    assert_ne!(
        a,
        Message {
            attributes: t_attrs,
            length: 4 + 2,
            ..Default::default()
        },
        "should not equal"
    );

    let t_attrs = Attributes(vec![RawAttribute {
        length: 2,
        value: vec![0x1, 0x1],
        typ: AttrType(0x2),
    }]);
    assert_ne!(
        a,
        Message {
            attributes: t_attrs,
            length: 4 + 2,
            ..Default::default()
        },
        "should not equal"
    );

    //"Nil attributes"
    {
        let a = Message {
            length: 4 + 2,
            ..Default::default()
        };
        let mut b = Message {
            attributes: attrs,
            length: 4 + 2,
            ..Default::default()
        };

        assert_ne!(a, b, "should not equal");
        assert_ne!(b, a, "should not equal");
        b.attributes = Attributes::default();
        assert_eq!(a, b, "should equal");
    }

    //"Attributes length"
    {
        let attr = RawAttribute {
            length: 2,
            value: vec![0x1, 0x2],
            typ: AttrType(0x1),
        };
        let attr1 = RawAttribute {
            length: 2,
            value: vec![0x1, 0x2],
            typ: AttrType(0x1),
        };
        let a = Message {
            attributes: Attributes(vec![attr.clone()]),
            length: 4 + 2,
            ..Default::default()
        };
        let b = Message {
            attributes: Attributes(vec![attr, attr1]),
            length: 4 + 2,
            ..Default::default()
        };
        assert_ne!(a, b, "should not equal");
    }

    //"Attributes values"
    {
        let attr = RawAttribute {
            length: 2,
            value: vec![0x1, 0x2],
            typ: AttrType(0x1),
        };
        let attr1 = RawAttribute {
            length: 2,
            value: vec![0x1, 0x1],
            typ: AttrType(0x1),
        };
        let a = Message {
            attributes: Attributes(vec![attr.clone(), attr.clone()]),
            length: 4 + 2,
            ..Default::default()
        };
        let b = Message {
            attributes: Attributes(vec![attr, attr1]),
            length: 4 + 2,
            ..Default::default()
        };
        assert_ne!(a, b, "should not equal");
    }

    Ok(())
}

#[test]
fn test_message_grow() -> Result<()> {
    let mut m = Message::new();
    m.grow(512, false);
    assert_eq!(m.raw.len(), 512, "Bad length {}", m.raw.len());

    Ok(())
}

#[test]
fn test_message_grow_smaller() -> Result<()> {
    let mut m = Message::new();
    m.grow(2, false);
    assert!(m.raw.capacity() >= 20, "Bad capacity {}", m.raw.capacity());

    assert!(m.raw.len() >= 20, "Bad length {}", m.raw.len());

    Ok(())
}

#[test]
fn test_message_string() -> Result<()> {
    let m = Message::new();
    assert_ne!(m.to_string(), "", "bad string");

    Ok(())
}

#[test]
fn test_is_message() -> Result<()> {
    let mut m = Message::new();
    let a = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: "software".to_owned(),
    };
    a.add_to(&mut m)?;
    m.write_header();

    let tests = vec![
        (vec![], false),                           // 0
        (vec![1, 2, 3], false),                    // 1
        (vec![1, 2, 4], false),                    // 2
        (vec![1, 2, 4, 5, 6, 7, 8, 9, 20], false), // 3
        (m.raw.to_vec(), true),                    // 5
        (
            vec![
                0, 0, 0, 0, 33, 18, 164, 66, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            true,
        ), // 6
    ];

    for (input, output) in tests {
        let got = is_message(&input);
        assert_eq!(got, output, "IsMessage({input:?}) {got} != {output}");
    }

    Ok(())
}

#[test]
fn test_message_contains() -> Result<()> {
    let mut m = Message::new();
    m.add(ATTR_SOFTWARE, "value".as_bytes());

    assert!(m.contains(ATTR_SOFTWARE), "message should contain software");
    assert!(!m.contains(ATTR_NONCE), "message should not contain nonce");

    Ok(())
}

#[test]
fn test_message_full_size() -> Result<()> {
    let mut m = Message::new();
    m.build(&[
        Box::new(BINDING_REQUEST),
        Box::new(TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 0])),
        Box::new(TextAttribute::new(ATTR_SOFTWARE, "pion/stun".to_owned())),
        Box::new(MessageIntegrity::new_long_term_integrity(
            "username".to_owned(),
            "realm".to_owned(),
            "password".to_owned(),
        )),
        Box::new(FINGERPRINT),
    ])?;
    let l = m.raw.len();
    m.raw = m.raw[..l - 10].to_vec();

    let mut decoder = Message::new();
    let l = m.raw.len();
    decoder.raw = m.raw[..l - 10].to_vec();
    let result = decoder.decode();
    assert!(result.is_err(), "decode on truncated buffer should error");

    Ok(())
}

#[test]
fn test_message_clone_to() -> Result<()> {
    let mut m = Message::new();
    m.build(&[
        Box::new(BINDING_REQUEST),
        Box::new(TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 0])),
        Box::new(TextAttribute::new(ATTR_SOFTWARE, "pion/stun".to_owned())),
        Box::new(MessageIntegrity::new_long_term_integrity(
            "username".to_owned(),
            "realm".to_owned(),
            "password".to_owned(),
        )),
        Box::new(FINGERPRINT),
    ])?;
    m.encode();

    let mut b = Message::new();
    m.clone_to(&mut b)?;
    assert_eq!(b, m, "not equal");

    //TODO: Corrupting m and checking that b is not corrupted.
    /*let (mut s, ok) = b.attributes.get(ATTR_SOFTWARE);
    assert!(ok, "no software attribute");
    s.value[0] = b'k';
    s.add_to(&mut b)?;
    assert_ne!(b, m, "should not be equal");*/

    Ok(())
}

#[test]
fn test_message_add_to() -> Result<()> {
    let mut m = Message::new();
    m.build(&[
        Box::new(BINDING_REQUEST),
        Box::new(TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 0])),
        Box::new(FINGERPRINT),
    ])?;
    m.encode();

    let mut b = Message::new();
    m.clone_to(&mut b)?;

    m.transaction_id = TransactionId([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 2, 0]);
    assert_ne!(b, m, "should not be equal");

    m.add_to(&mut b)?;
    assert_eq!(b, m, "should be equal");

    Ok(())
}

#[test]
fn test_decode() -> Result<()> {
    let mut m = Message::new();
    m.typ = MessageType {
        method: METHOD_BINDING,
        class: CLASS_REQUEST,
    };
    m.transaction_id = TransactionId::new();
    m.add(ATTR_ERROR_CODE, &[0xff, 0xfe, 0xfa]);
    m.write_header();

    let mut m_decoded = Message::new();
    m_decoded.raw.clear();
    m_decoded.raw.extend_from_slice(&m.raw);
    m_decoded.decode()?;
    assert_eq!(
        m_decoded, m,
        "decoded result is not equal to encoded message"
    );

    Ok(())
}

#[test]
fn test_message_marshal_binary() -> Result<()> {
    let mut m = Message::new();
    m.build(&[
        Box::new(TextAttribute::new(ATTR_SOFTWARE, "software".to_owned())),
        Box::new(XorMappedAddress {
            ip: "213.1.223.5".parse().unwrap(),
            port: 0,
        }),
    ])?;

    let mut data = m.marshal_binary()?;
    // Reset m.Raw to check retention.
    for i in 0..m.raw.len() {
        m.raw[i] = 0;
    }
    m.unmarshal_binary(&data)?;

    // Reset data to check retention.
    #[allow(clippy::needless_range_loop)]
    for i in 0..data.len() {
        data[i] = 0;
    }

    m.decode()?;

    Ok(())
}
