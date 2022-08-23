use super::sample_sequence_location::*;

#[test]
fn test_sample_sequence_location_compare() {
    let s1 = SampleSequenceLocation { head: 32, tail: 42 };
    assert_eq!(Comparison::Before, s1.compare(16));
    assert_eq!(Comparison::Inside, s1.compare(32));
    assert_eq!(Comparison::Inside, s1.compare(38));
    assert_eq!(Comparison::Inside, s1.compare(41));
    assert_eq!(Comparison::After, s1.compare(42));
    assert_eq!(Comparison::After, s1.compare(0x57));

    let s2 = SampleSequenceLocation {
        head: 0xffa0,
        tail: 32,
    };
    assert_eq!(Comparison::Before, s2.compare(0xff00));
    assert_eq!(Comparison::Inside, s2.compare(0xffa0));
    assert_eq!(Comparison::Inside, s2.compare(0xffff));
    assert_eq!(Comparison::Inside, s2.compare(0));
    assert_eq!(Comparison::Inside, s2.compare(31));
    assert_eq!(Comparison::After, s2.compare(32));
    assert_eq!(Comparison::After, s2.compare(128));
}
