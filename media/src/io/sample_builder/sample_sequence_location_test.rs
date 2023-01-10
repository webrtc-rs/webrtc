use super::sample_sequence_location::*;

#[test]
fn test_sample_sequence_location_compare() {
    let s1 = SampleSequenceLocation { head: 32, tail: 42 };
    assert_eq!(s1.compare(16), Comparison::Before);
    assert_eq!(s1.compare(32), Comparison::Inside);
    assert_eq!(s1.compare(38), Comparison::Inside);
    assert_eq!(s1.compare(41), Comparison::Inside);
    assert_eq!(s1.compare(42), Comparison::After);
    assert_eq!(s1.compare(0x57), Comparison::After);

    let s2 = SampleSequenceLocation {
        head: 0xffa0,
        tail: 32,
    };
    assert_eq!(s2.compare(0xff00), Comparison::Before);
    assert_eq!(s2.compare(0xffa0), Comparison::Inside);
    assert_eq!(s2.compare(0xffff), Comparison::Inside);
    assert_eq!(s2.compare(0), Comparison::Inside);
    assert_eq!(s2.compare(31), Comparison::Inside);
    assert_eq!(s2.compare(32), Comparison::After);
    assert_eq!(s2.compare(128), Comparison::After);
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
