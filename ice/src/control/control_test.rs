use super::*;
use crate::error::Result;

#[test]
fn test_controlled_get_from() -> Result<()> {
    let mut m = Message::new();
    let mut c = AttrControlled(4321);
    let result = c.get_from(&m);
    if let Err(err) = result {
        assert_eq!(stun::Error::ErrAttributeNotFound, err, "unexpected error");
    } else {
        panic!("expected error, but got ok");
    }

    m.build(&[Box::new(BINDING_REQUEST), Box::new(c)])?;

    let mut m1 = Message::new();
    m1.write(&m.raw)?;

    let mut c1 = AttrControlled::default();
    c1.get_from(&m1)?;

    assert_eq!(c1, c, "not equal");

    //"IncorrectSize"
    {
        let mut m3 = Message::new();
        m3.add(ATTR_ICE_CONTROLLED, &[0; 100]);
        let mut c2 = AttrControlled::default();
        let result = c2.get_from(&m3);
        if let Err(err) = result {
            assert!(is_attr_size_invalid(&err), "should error");
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_controlling_get_from() -> Result<()> {
    let mut m = Message::new();
    let mut c = AttrControlling(4321);
    let result = c.get_from(&m);
    if let Err(err) = result {
        assert_eq!(stun::Error::ErrAttributeNotFound, err, "unexpected error");
    } else {
        panic!("expected error, but got ok");
    }

    m.build(&[Box::new(BINDING_REQUEST), Box::new(c)])?;

    let mut m1 = Message::new();
    m1.write(&m.raw)?;

    let mut c1 = AttrControlling::default();
    c1.get_from(&m1)?;

    assert_eq!(c1, c, "not equal");

    //"IncorrectSize"
    {
        let mut m3 = Message::new();
        m3.add(ATTR_ICE_CONTROLLING, &[0; 100]);
        let mut c2 = AttrControlling::default();
        let result = c2.get_from(&m3);
        if let Err(err) = result {
            assert!(is_attr_size_invalid(&err), "should error");
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}

#[test]
fn test_control_get_from() -> Result<()> {
    //"Blank"
    {
        let m = Message::new();
        let mut c = AttrControl::default();
        let result = c.get_from(&m);
        if let Err(err) = result {
            assert_eq!(stun::Error::ErrAttributeNotFound, err, "unexpected error");
        } else {
            panic!("expected error, but got ok");
        }
    }
    //"Controlling"
    {
        let mut m = Message::new();
        let mut c = AttrControl::default();
        let result = c.get_from(&m);
        if let Err(err) = result {
            assert_eq!(stun::Error::ErrAttributeNotFound, err, "unexpected error");
        } else {
            panic!("expected error, but got ok");
        }

        c.role = Role::Controlling;
        c.tie_breaker = TieBreaker(4321);

        m.build(&[Box::new(BINDING_REQUEST), Box::new(c)])?;

        let mut m1 = Message::new();
        m1.write(&m.raw)?;

        let mut c1 = AttrControl::default();
        c1.get_from(&m1)?;

        assert_eq!(c1, c, "not equal");

        //"IncorrectSize"
        {
            let mut m3 = Message::new();
            m3.add(ATTR_ICE_CONTROLLING, &[0; 100]);
            let mut c2 = AttrControl::default();
            let result = c2.get_from(&m3);
            if let Err(err) = result {
                assert!(is_attr_size_invalid(&err), "should error");
            } else {
                panic!("expected error, but got ok");
            }
        }
    }

    //"Controlled"
    {
        let mut m = Message::new();
        let mut c = AttrControl::default();
        let result = c.get_from(&m);
        if let Err(err) = result {
            assert_eq!(stun::Error::ErrAttributeNotFound, err, "unexpected error");
        } else {
            panic!("expected error, but got ok");
        }

        c.role = Role::Controlled;
        c.tie_breaker = TieBreaker(1234);

        m.build(&[Box::new(BINDING_REQUEST), Box::new(c)])?;

        let mut m1 = Message::new();
        m1.write(&m.raw)?;

        let mut c1 = AttrControl::default();
        c1.get_from(&m1)?;

        assert_eq!(c1, c, "not equal");

        //"IncorrectSize"
        {
            let mut m3 = Message::new();
            m3.add(ATTR_ICE_CONTROLLING, &[0; 100]);
            let mut c2 = AttrControl::default();
            let result = c2.get_from(&m3);
            if let Err(err) = result {
                assert!(is_attr_size_invalid(&err), "should error");
            } else {
                panic!("expected error, but got ok");
            }
        }
    }

    Ok(())
}
