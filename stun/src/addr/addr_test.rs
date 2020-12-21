use super::*;
use crate::errors::*;

use util::Error;

#[test]
fn test_mapped_address() -> Result<(), Error> {
    let mut m = Message::new();
    let mut addr = MappedAddress {
        ip: "122.12.34.5".parse().unwrap(),
        port: 5412,
    };
    assert_eq!(addr.to_string(), "122.12.34.5:5412", "bad string {}", addr);

    //"add_to"
    {
        addr.add_to(&mut m)?;

        //"GetFrom"
        {
            let mut got = MappedAddress::default();
            got.get_from(&mut m)?;
            assert_eq!(got.ip, addr.ip, "got bad IP: {}", got.ip);

            //"Not found"
            {
                let message = Message::new();
                let result = got.get_from(&message);
                if let Err(err) = result {
                    assert_eq!(
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone(),
                        "should be not found: {}",
                        err
                    );
                } else {
                    assert!(false, "expected error, but got ok");
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
                        err,
                        ERR_UNEXPECTED_EOF.clone(),
                        "<{}> should be <{}>",
                        err,
                        ERR_UNEXPECTED_EOF.clone()
                    );
                } else {
                    assert!(false, "expected error, but got ok");
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_mapped_address_v6() -> Result<(), Error> {
    let mut m = Message::new();
    let mut addr = MappedAddress {
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
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone(),
                        "<{}> should be <{}>",
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone()
                    );
                } else {
                    assert!(false, "expected error, but got ok");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn test_alternate_server() -> Result<(), Error> {
    let mut m = Message::new();
    let mut addr = MappedAddress {
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
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone(),
                        "<{}> should be <{}>",
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone()
                    );
                } else {
                    assert!(false, "expected error, but got ok");
                }
            }
        }
    }

    Ok(())
}

#[test]
fn test_other_address() -> Result<(), Error> {
    let mut m = Message::new();
    let mut addr = OtherAddress {
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
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone(),
                        "<{}> should be <{}>",
                        err,
                        ERR_ATTRIBUTE_NOT_FOUND.clone()
                    );
                } else {
                    assert!(false, "expected error, but got ok");
                }
            }
        }
    }

    Ok(())
}
