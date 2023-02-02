use super::*;
use crate::error::*;

#[test]
fn test_mapped_address() -> Result<()> {
    let mut m = Message::new();
    let addr = MappedAddress {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    assert_eq!(addr.to_string(), "122.12.34.5:5412", "bad string {addr}");

    //"add_to"
    {
        addr.add_to(&mut m)?;

        //"GetFrom"
        {
            let mut got = MappedAddress::default();
            got.get_from(&m)?;
            assert_eq!(got.ip, addr.ip, "got bad IP: {}", got.ip);

            //"Not found"
            {
                let message = Message::new();
                let result = got.get_from(&message);
                if let Err(err) = result {
                    assert_eq!(
                        Error::ErrAttributeNotFound,
                        err,
                        "should be not found: {err}"
                    );
                } else {
                    panic!("expected error, but got ok");
                }
            }
            //"Bad family"
            {
                let (mut v, _) = m.attributes.get(ATTR_MAPPED_ADDRESS);
                v.value[0] = 32;
                got.get_from(&m)?
            }
            //"Bad length"
            {
                let mut message = Message::new();
                message.add(ATTR_MAPPED_ADDRESS, &[1, 2, 3]);
                let result = got.get_from(&message);
                if let Err(err) = result {
                    assert_eq!(
                        Error::ErrUnexpectedEof,
                        err,
                        "<{}> should be <{}>",
                        err,
                        Error::ErrUnexpectedEof
                    );
                } else {
                    panic!("expected error, but got ok");
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_mapped_address_v6() -> Result<()> {
    let mut m = Message::new();
    let addr = MappedAddress {
        ip: "::".parse().unwrap(),
        port: 5412,
    };

    //"add_to"
    {
        addr.add_to(&mut m)?;

        //"GetFrom"
        {
            let mut got = MappedAddress::default();
            got.get_from(&m)?;
            assert_eq!(got.ip, addr.ip, "got bad IP: {}", got.ip);

            //"Not found"
            {
                let message = Message::new();
                let result = got.get_from(&message);
                if let Err(err) = result {
                    assert_eq!(
                        Error::ErrAttributeNotFound,
                        err,
                        "<{}> should be <{}>",
                        err,
                        Error::ErrAttributeNotFound,
                    );
                } else {
                    panic!("expected error, but got ok");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn test_alternate_server() -> Result<()> {
    let mut m = Message::new();
    let addr = MappedAddress {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };

    //"add_to"
    {
        addr.add_to(&mut m)?;

        //"GetFrom"
        {
            let mut got = AlternateServer::default();
            got.get_from(&m)?;
            assert_eq!(got.ip, addr.ip, "got bad IP: {}", got.ip);

            //"Not found"
            {
                let message = Message::new();
                let result = got.get_from(&message);
                if let Err(err) = result {
                    assert_eq!(
                        Error::ErrAttributeNotFound,
                        err,
                        "<{}> should be <{}>",
                        err,
                        Error::ErrAttributeNotFound,
                    );
                } else {
                    panic!("expected error, but got ok");
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_other_address() -> Result<()> {
    let mut m = Message::new();
    let addr = OtherAddress {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };

    //"add_to"
    {
        addr.add_to(&mut m)?;

        //"GetFrom"
        {
            let mut got = OtherAddress::default();
            got.get_from(&m)?;
            assert_eq!(got.ip, addr.ip, "got bad IP: {}", got.ip);

            //"Not found"
            {
                let message = Message::new();
                let result = got.get_from(&message);
                if let Err(err) = result {
                    assert_eq!(
                        Error::ErrAttributeNotFound,
                        err,
                        "<{}> should be <{}>",
                        err,
                        Error::ErrAttributeNotFound,
                    );
                } else {
                    panic!("expected error, but got ok");
                }
            }
        }
    }

    Ok(())
}
