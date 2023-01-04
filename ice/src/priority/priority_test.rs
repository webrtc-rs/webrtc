use super::*;
use crate::error::Result;

#[test]
fn test_priority_get_from() -> Result<()> {
    let mut m = Message::new();
    let mut p = PriorityAttr::default();
    let result = p.get_from(&m);
    if let Err(err) = result {
        assert_eq!(err, stun::Error::ErrAttributeNotFound, "unexpected error");
    } else {
        panic!("expected error, but got ok");
    }

    m.build(&[Box::new(BINDING_REQUEST), Box::new(p)])?;

    let mut m1 = Message::new();
    m1.write(&m.raw)?;

    let mut p1 = PriorityAttr::default();
    p1.get_from(&m1)?;

    assert_eq!(p1, p, "not equal");

    //"IncorrectSize"
    {
        let mut m3 = Message::new();
        m3.add(ATTR_PRIORITY, &[0; 100]);
        let mut p2 = PriorityAttr::default();
        let result = p2.get_from(&m3);
        if let Err(err) = result {
            assert!(is_attr_size_invalid(&err), "should error");
        } else {
            panic!("expected error, but got ok");
        }
    }

    Ok(())
}
