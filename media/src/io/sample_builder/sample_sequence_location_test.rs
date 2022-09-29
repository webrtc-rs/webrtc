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

#[test]
fn test_sample_sequence_location_range() {
    let mut data: Vec<Option<u16>> = vec![None; u16::MAX as usize + 1];

    data[65533] = Some(65533);
    data[65535] = Some(65535);
    data[0] = Some(0);
    data[2] = Some(2);

    let s = SampleSequenceLocation {
        head: 65533,
        tail: 3,
    };
    let reconstructed: Vec<_> = s.range(&data).map(|x| x.cloned()).collect();

    assert_eq!(
        reconstructed,
        [Some(65533), None, Some(65535), Some(0), None, Some(2)]
    );
}
