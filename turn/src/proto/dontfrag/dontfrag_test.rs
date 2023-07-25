use super::*;

#[test]
fn test_dont_fragment_false() -> Result<(), stun::Error> {
    let mut dont_fragment = DontFragmentAttr;

    let mut m = Message::new();
    m.write_header();
    assert!(dont_fragment.get_from(&m).is_err(), "should not be set");

    Ok(())
}

#[test]
fn test_dont_fragment_add_to() -> Result<(), stun::Error> {
    let mut dont_fragment = DontFragmentAttr;

    let mut m = Message::new();
    dont_fragment.add_to(&mut m)?;
    m.write_header();

    let mut decoded = Message::new();
    decoded.write(&m.raw)?;
    assert!(dont_fragment.get_from(&m).is_ok(), "should be set");

    Ok(())
}
