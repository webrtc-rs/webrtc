use super::*;
use crate::textattrs::TextAttribute;

#[test]
fn test_raw_attribute_add_to() -> Result<()> {
    let v = vec![1, 2, 3, 4];
    let mut m = Message::new();
    let ra = Box::new(RawAttribute {
        typ: ATTR_DATA,
        value: v.clone(),
        ..Default::default()
    });
    m.build(&[ra])?;
    let got_v = m.get(ATTR_DATA)?;
    assert_eq!(got_v, v, "value mismatch");

    Ok(())
}

#[test]
fn test_message_get_no_allocs() -> Result<()> {
    let mut m = Message::new();
    let a = TextAttribute {
        attr: ATTR_SOFTWARE,
        text: "c".to_owned(),
    };
    a.add_to(&mut m)?;
    m.write_header();

    //"Default"
    {
        m.get(ATTR_SOFTWARE)?;
    }
    //"Not found"
    {
        let result = m.get(ATTR_ORIGIN);
        assert!(result.is_err(), "should error");
    }

    Ok(())
}

#[test]
fn test_padding() -> Result<()> {
    let tt = vec![
        (4, 4),   // 0
        (2, 4),   // 1
        (5, 8),   // 2
        (8, 8),   // 3
        (11, 12), // 4
        (1, 4),   // 5
        (3, 4),   // 6
        (6, 8),   // 7
        (7, 8),   // 8
        (0, 0),   // 9
        (40, 40), // 10
    ];

    for (i, o) in tt {
        let got = nearest_padded_value_length(i);
        assert_eq!(got, o, "padded({i}) {got} (got) != {o} (expected)",);
    }

    Ok(())
}

#[test]
fn test_attr_type_range() -> Result<()> {
    let tests = vec![
        ATTR_PRIORITY,
        ATTR_ERROR_CODE,
        ATTR_USE_CANDIDATE,
        ATTR_EVEN_PORT,
        ATTR_REQUESTED_ADDRESS_FAMILY,
    ];
    for a in tests {
        assert!(!a.optional() && a.required(), "should be required");
    }

    let tests = vec![ATTR_SOFTWARE, ATTR_ICE_CONTROLLED, ATTR_ORIGIN];
    for a in tests {
        assert!(!a.required() && a.optional(), "should be optional");
    }

    Ok(())
}
