use std::io::BufReader;

use super::*;
use crate::checks::*;
use crate::error::*;

#[test]
fn test_software_get_from() -> Result<()> {
    let mut m = Message::new();
    let v = "Client v0.0.1".to_owned();
    m.add(ATTR_SOFTWARE, v.as_bytes());
    m.write_header();

    let mut m2 = Message {
        raw: Vec::with_capacity(256),
        ..Default::default()
    };

    let mut reader = BufReader::new(m.raw.as_slice());
    m2.read_from(&mut reader)?;
    let software = TextAttribute::get_from_as(&m, ATTR_SOFTWARE)?;
    assert_eq!(software.to_string(), v, "Expected {v}, got {software}.");

    let (s_attr, ok) = m.attributes.get(ATTR_SOFTWARE);
    assert!(ok, "sowfware attribute should be found");

    let s = s_attr.to_string();
    assert!(s.starts_with("SOFTWARE:"), "bad string representation {s}");

    Ok(())
}

#[test]
fn test_software_add_to_invalid() -> Result<()> {
    let mut m = Message::new();
    let s = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: String::from_utf8(vec![0; 1024]).unwrap(),
    };
    let result = s.add_to(&mut m);
    if let Err(err) = result {
        assert!(
            is_attr_size_overflow(&err),
            "add_to should return AttrOverflowErr, got: {err}"
        );
    } else {
        panic!("expected error, but got ok");
    }

    let result = TextAttribute::get_from_as(&m, ATTR_SOFTWARE);
    if let Err(err) = result {
        assert_eq!(
            Error::ErrAttributeNotFound,
            err,
            "GetFrom should return {}, got: {}",
            Error::ErrAttributeNotFound,
            err
        );
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_software_add_to_regression() -> Result<()> {
    // s.add_to checked len(m.Raw) instead of len(s.Raw).
    let mut m = Message {
        raw: vec![0u8; 2048],
        ..Default::default()
    };
    let s = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: String::from_utf8(vec![0; 100]).unwrap(),
    };
    s.add_to(&mut m)?;

    Ok(())
}

#[test]
fn test_username() -> Result<()> {
    let username = "username".to_owned();
    let u = TextAttribute {
        attr: ATTR_USERNAME,
        text: username.clone(),
    };
    let mut m = Message::new();
    m.write_header();
    //"Bad length"
    {
        let bad_u = TextAttribute {
            attr: ATTR_USERNAME,
            text: String::from_utf8(vec![0; 600]).unwrap(),
        };
        let result = bad_u.add_to(&mut m);
        if let Err(err) = result {
            assert!(
                is_attr_size_overflow(&err),
                "add_to should return *AttrOverflowErr, got: {err}"
            );
        } else {
            panic!("expected error, but got ok");
        }
    }
    //"add_to"
    {
        u.add_to(&mut m)?;

        //"GetFrom"
        {
            let got = TextAttribute::get_from_as(&m, ATTR_USERNAME)?;
            assert_eq!(
                got.to_string(),
                username,
                "expedted: {username}, got: {got}"
            );
            //"Not found"
            {
                let m = Message::new();
                let result = TextAttribute::get_from_as(&m, ATTR_USERNAME);
                if let Err(err) = result {
                    assert_eq!(Error::ErrAttributeNotFound, err, "Should error");
                } else {
                    panic!("expected error, but got ok");
                }
            }
        }
    }

    //"No allocations"
    {
        let mut m = Message::new();
        m.write_header();
        let u = TextAttribute {
            attr: ATTR_USERNAME,
            text: "username".to_owned(),
        };

        u.add_to(&mut m)?;
        m.reset();
    }

    Ok(())
}

#[test]
fn test_realm_get_from() -> Result<()> {
    let mut m = Message::new();
    let v = "realm".to_owned();
    m.add(ATTR_REALM, v.as_bytes());
    m.write_header();

    let mut m2 = Message {
        raw: Vec::with_capacity(256),
        ..Default::default()
    };

    let result = TextAttribute::get_from_as(&m2, ATTR_REALM);
    if let Err(err) = result {
        assert_eq!(
            Error::ErrAttributeNotFound,
            err,
            "GetFrom should return {}, got: {}",
            Error::ErrAttributeNotFound,
            err
        );
    } else {
        panic!("Expected error, but got ok");
    }

    let mut reader = BufReader::new(m.raw.as_slice());
    m2.read_from(&mut reader)?;

    let r = TextAttribute::get_from_as(&m, ATTR_REALM)?;
    assert_eq!(r.to_string(), v, "Expected {v}, got {r}.");

    let (r_attr, ok) = m.attributes.get(ATTR_REALM);
    assert!(ok, "realm attribute should be found");

    let s = r_attr.to_string();
    assert!(s.starts_with("REALM:"), "bad string representation {s}");

    Ok(())
}

#[test]
fn test_realm_add_to_invalid() -> Result<()> {
    let mut m = Message::new();
    let s = TextAttribute {
        attr: ATTR_REALM,
        text: String::from_utf8(vec![0; 1024]).unwrap(),
    };
    let result = s.add_to(&mut m);
    if let Err(err) = result {
        assert!(
            is_attr_size_overflow(&err),
            "add_to should return AttrOverflowErr, got: {err}"
        );
    } else {
        panic!("expected error, but got ok");
    }

    let result = TextAttribute::get_from_as(&m, ATTR_REALM);
    if let Err(err) = result {
        assert_eq!(
            Error::ErrAttributeNotFound,
            err,
            "GetFrom should return {}, got: {}",
            Error::ErrAttributeNotFound,
            err
        );
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_nonce_get_from() -> Result<()> {
    let mut m = Message::new();
    let v = "example.org".to_owned();
    m.add(ATTR_NONCE, v.as_bytes());
    m.write_header();

    let mut m2 = Message {
        raw: Vec::with_capacity(256),
        ..Default::default()
    };

    let result = TextAttribute::get_from_as(&m2, ATTR_NONCE);
    if let Err(err) = result {
        assert_eq!(
            Error::ErrAttributeNotFound,
            err,
            "GetFrom should return {}, got: {}",
            Error::ErrAttributeNotFound,
            err
        );
    } else {
        panic!("Expected error, but got ok");
    }

    let mut reader = BufReader::new(m.raw.as_slice());
    m2.read_from(&mut reader)?;

    let r = TextAttribute::get_from_as(&m, ATTR_NONCE)?;
    assert_eq!(r.to_string(), v, "Expected {v}, got {r}.");

    let (r_attr, ok) = m.attributes.get(ATTR_NONCE);
    assert!(ok, "realm attribute should be found");

    let s = r_attr.to_string();
    assert!(s.starts_with("NONCE:"), "bad string representation {s}");

    Ok(())
}

#[test]
fn test_nonce_add_to_invalid() -> Result<()> {
    let mut m = Message::new();
    let s = TextAttribute {
        attr: ATTR_NONCE,
        text: String::from_utf8(vec![0; 1024]).unwrap(),
    };
    let result = s.add_to(&mut m);
    if let Err(err) = result {
        assert!(
            is_attr_size_overflow(&err),
            "add_to should return AttrOverflowErr, got: {err}"
        );
    } else {
        panic!("expected error, but got ok");
    }

    let result = TextAttribute::get_from_as(&m, ATTR_NONCE);
    if let Err(err) = result {
        assert_eq!(
            Error::ErrAttributeNotFound,
            err,
            "GetFrom should return {}, got: {}",
            Error::ErrAttributeNotFound,
            err
        );
    } else {
        panic!("expected error, but got ok");
    }

    Ok(())
}

#[test]
fn test_nonce_add_to() -> Result<()> {
    let mut m = Message::new();
    let n = TextAttribute {
        attr: ATTR_NONCE,
        text: "example.org".to_owned(),
    };
    n.add_to(&mut m)?;

    let v = m.get(ATTR_NONCE)?;
    assert_eq!(v.as_slice(), b"example.org", "bad nonce {v:?}");

    Ok(())
}
