use std::io::BufReader;
use std::iter::Iterator;

use super::*;
use crate::lexer::END_LINE;
use crate::util::ATTRIBUTE_KEY;

const EXAMPLE_ATTR_EXTMAP1: &str = "extmap:1 http://example.com/082005/ext.htm#ttime";
const EXAMPLE_ATTR_EXTMAP2: &str =
    "extmap:2/sendrecv http://example.com/082005/ext.htm#xmeta short";
const FAILING_ATTR_EXTMAP1: &str =
    "extmap:257/sendrecv http://example.com/082005/ext.htm#xmeta short";
const FAILING_ATTR_EXTMAP2: &str = "extmap:2/blorg http://example.com/082005/ext.htm#xmeta short";

#[test]
fn test_extmap() -> Result<()> {
    let example_attr_extmap1_line = EXAMPLE_ATTR_EXTMAP1;
    let example_attr_extmap2_line = EXAMPLE_ATTR_EXTMAP2;
    let failing_attr_extmap1_line = format!("{ATTRIBUTE_KEY}{FAILING_ATTR_EXTMAP1}{END_LINE}");
    let failing_attr_extmap2_line = format!("{ATTRIBUTE_KEY}{FAILING_ATTR_EXTMAP2}{END_LINE}");
    let passingtests = [
        (EXAMPLE_ATTR_EXTMAP1, example_attr_extmap1_line),
        (EXAMPLE_ATTR_EXTMAP2, example_attr_extmap2_line),
    ];
    let failingtests = vec![
        (FAILING_ATTR_EXTMAP1, failing_attr_extmap1_line),
        (FAILING_ATTR_EXTMAP2, failing_attr_extmap2_line),
    ];

    for (i, u) in passingtests.iter().enumerate() {
        let mut reader = BufReader::new(u.1.as_bytes());
        let actual = ExtMap::unmarshal(&mut reader)?;
        assert_eq!(
            actual.marshal(),
            u.1,
            "{}: {} vs {}",
            i,
            u.1,
            actual.marshal()
        );
    }

    for u in failingtests {
        let mut reader = BufReader::new(u.1.as_bytes());
        let actual = ExtMap::unmarshal(&mut reader);
        assert!(actual.is_err());
    }

    Ok(())
}

#[test]
fn test_transport_cc_extmap() -> Result<()> {
    // a=extmap:<value>["/"<direction>] <URI> <extensionattributes>
    // a=extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01
    let uri = Some(Url::parse(
        "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01",
    )?);
    let e = ExtMap {
        value: 3,
        uri,
        direction: Direction::Unspecified,
        ext_attr: None,
    };

    let s = e.marshal();
    if s == "3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01" {
        panic!("TestTransportCC failed");
    } else {
        assert_eq!(
            s,
            "extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01"
        )
    }

    Ok(())
}
