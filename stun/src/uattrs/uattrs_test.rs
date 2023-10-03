use super::*;

#[test]
fn test_unknown_attributes() -> Result<()> {
    let mut m = Message::new();
    let a = UnknownAttributes(vec![ATTR_DONT_FRAGMENT, ATTR_CHANNEL_NUMBER]);
    assert_eq!(
        a.to_string(),
        "DONT-FRAGMENT, CHANNEL-NUMBER",
        "bad String:{a}"
    );
    assert_eq!(
        UnknownAttributes(vec![]).to_string(),
        "<nil>",
        "bad blank string"
    );

    a.add_to(&mut m)?;

    //"GetFrom"
    {
        let mut attrs = UnknownAttributes(Vec::with_capacity(10));
        attrs.get_from(&m)?;
        for i in 0..a.0.len() {
            assert_eq!(a.0[i], attrs.0[i], "expected {} != {}", a.0[i], attrs.0[i]);
        }
        let mut m_blank = Message::new();
        let result = attrs.get_from(&m_blank);
        assert!(result.is_err(), "should error");

        m_blank.add(ATTR_UNKNOWN_ATTRIBUTES, &[1, 2, 3]);
        let result = attrs.get_from(&m_blank);
        assert!(result.is_err(), "should error");
    }

    Ok(())
}
