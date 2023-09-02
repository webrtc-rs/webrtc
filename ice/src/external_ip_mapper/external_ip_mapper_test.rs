use super::*;

#[test]
fn test_external_ip_mapper_validate_ip_string() -> Result<()> {
    let ip = validate_ip_string("1.2.3.4")?;
    assert!(ip.is_ipv4(), "should be true");
    assert_eq!("1.2.3.4", ip.to_string(), "should be true");

    let ip = validate_ip_string("2601:4567::5678")?;
    assert!(!ip.is_ipv4(), "should be false");
    assert_eq!("2601:4567::5678", ip.to_string(), "should be true");

    let result = validate_ip_string("bad.6.6.6");
    assert!(result.is_err(), "should fail");

    Ok(())
}

#[test]
fn test_external_ip_mapper_new_external_ip_mapper() -> Result<()> {
    // ips being empty should succeed but mapper will still be nil
    let m = ExternalIpMapper::new(CandidateType::Unspecified, &[])?;
    assert!(m.is_none(), "should be none");

    // IPv4 with no explicit local IP, defaults to CandidateTypeHost
    let m = ExternalIpMapper::new(CandidateType::Unspecified, &["1.2.3.4".to_owned()])?.unwrap();
    assert_eq!(m.candidate_type, CandidateType::Host, "should match");
    assert!(m.ipv4_mapping.ip_sole.is_some());
    assert!(m.ipv6_mapping.ip_sole.is_none());
    assert_eq!(m.ipv4_mapping.ip_map.len(), 0, "should match");
    assert_eq!(m.ipv6_mapping.ip_map.len(), 0, "should match");

    // IPv4 with no explicit local IP, using CandidateTypeServerReflexive
    let m =
        ExternalIpMapper::new(CandidateType::ServerReflexive, &["1.2.3.4".to_owned()])?.unwrap();
    assert_eq!(
        CandidateType::ServerReflexive,
        m.candidate_type,
        "should match"
    );
    assert!(m.ipv4_mapping.ip_sole.is_some());
    assert!(m.ipv6_mapping.ip_sole.is_none());
    assert_eq!(m.ipv4_mapping.ip_map.len(), 0, "should match");
    assert_eq!(m.ipv6_mapping.ip_map.len(), 0, "should match");

    // IPv4 with no explicit local IP, defaults to CandidateTypeHost
    let m = ExternalIpMapper::new(CandidateType::Unspecified, &["2601:4567::5678".to_owned()])?
        .unwrap();
    assert_eq!(m.candidate_type, CandidateType::Host, "should match");
    assert!(m.ipv4_mapping.ip_sole.is_none());
    assert!(m.ipv6_mapping.ip_sole.is_some());
    assert_eq!(m.ipv4_mapping.ip_map.len(), 0, "should match");
    assert_eq!(m.ipv6_mapping.ip_map.len(), 0, "should match");

    // IPv4 and IPv6 in the mix
    let m = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &["1.2.3.4".to_owned(), "2601:4567::5678".to_owned()],
    )?
    .unwrap();
    assert_eq!(m.candidate_type, CandidateType::Host, "should match");
    assert!(m.ipv4_mapping.ip_sole.is_some());
    assert!(m.ipv6_mapping.ip_sole.is_some());
    assert_eq!(m.ipv4_mapping.ip_map.len(), 0, "should match");
    assert_eq!(m.ipv6_mapping.ip_map.len(), 0, "should match");

    // Unsupported candidate type - CandidateTypePeerReflexive
    let result = ExternalIpMapper::new(CandidateType::PeerReflexive, &["1.2.3.4".to_owned()]);
    assert!(result.is_err(), "should fail");

    // Unsupported candidate type - CandidateTypeRelay
    let result = ExternalIpMapper::new(CandidateType::PeerReflexive, &["1.2.3.4".to_owned()]);
    assert!(result.is_err(), "should fail");

    // Cannot duplicate mapping IPv4 family
    let result = ExternalIpMapper::new(
        CandidateType::ServerReflexive,
        &["1.2.3.4".to_owned(), "5.6.7.8".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    // Cannot duplicate mapping IPv6 family
    let result = ExternalIpMapper::new(
        CandidateType::ServerReflexive,
        &["2201::1".to_owned(), "2201::0002".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    // Invalid external IP string
    let result = ExternalIpMapper::new(CandidateType::ServerReflexive, &["bad.2.3.4".to_owned()]);
    assert!(result.is_err(), "should fail");

    // Invalid local IP string
    let result = ExternalIpMapper::new(
        CandidateType::ServerReflexive,
        &["1.2.3.4/10.0.0.bad".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    Ok(())
}

#[test]
fn test_external_ip_mapper_new_external_ip_mapper_with_explicit_local_ip() -> Result<()> {
    // IPv4 with  explicit local IP, defaults to CandidateTypeHost
    let m = ExternalIpMapper::new(CandidateType::Unspecified, &["1.2.3.4/10.0.0.1".to_owned()])?
        .unwrap();
    assert_eq!(m.candidate_type, CandidateType::Host, "should match");
    assert!(m.ipv4_mapping.ip_sole.is_none());
    assert!(m.ipv6_mapping.ip_sole.is_none());
    assert_eq!(m.ipv4_mapping.ip_map.len(), 1, "should match");
    assert_eq!(m.ipv6_mapping.ip_map.len(), 0, "should match");

    // Cannot assign two ext IPs for one local IPv4
    let result = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &["1.2.3.4/10.0.0.1".to_owned(), "1.2.3.5/10.0.0.1".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    // Cannot assign two ext IPs for one local IPv6
    let result = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &[
            "2200::1/fe80::1".to_owned(),
            "2200::0002/fe80::1".to_owned(),
        ],
    );
    assert!(result.is_err(), "should fail");

    // Cannot mix different IP family in a pair (1)
    let result =
        ExternalIpMapper::new(CandidateType::Unspecified, &["2200::1/10.0.0.1".to_owned()]);
    assert!(result.is_err(), "should fail");

    // Cannot mix different IP family in a pair (2)
    let result = ExternalIpMapper::new(CandidateType::Unspecified, &["1.2.3.4/fe80::1".to_owned()]);
    assert!(result.is_err(), "should fail");

    // Invalid pair
    let result = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &["1.2.3.4/192.168.0.2/10.0.0.1".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    Ok(())
}

#[test]
fn test_external_ip_mapper_new_external_ip_mapper_with_implicit_local_ip() -> Result<()> {
    // Mixing inpicit and explicit local IPs not allowed
    let result = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &["1.2.3.4".to_owned(), "1.2.3.5/10.0.0.1".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    // Mixing inpicit and explicit local IPs not allowed
    let result = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &["1.2.3.5/10.0.0.1".to_owned(), "1.2.3.4".to_owned()],
    );
    assert!(result.is_err(), "should fail");

    Ok(())
}

#[test]
fn test_external_ip_mapper_find_external_ip_without_explicit_local_ip() -> Result<()> {
    // IPv4 with  explicit local IP, defaults to CandidateTypeHost
    let m = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &["1.2.3.4".to_owned(), "2200::1".to_owned()],
    )?
    .unwrap();
    assert!(m.ipv4_mapping.ip_sole.is_some());
    assert!(m.ipv6_mapping.ip_sole.is_some());

    // find external IPv4
    let ext_ip = m.find_external_ip("10.0.0.1")?;
    assert_eq!(ext_ip.to_string(), "1.2.3.4", "should match");

    // find external IPv6
    let ext_ip = m.find_external_ip("fe80::0001")?; // use '0001' instead of '1' on purpose
    assert_eq!(ext_ip.to_string(), "2200::1", "should match");

    // Bad local IP string
    let result = m.find_external_ip("really.bad");
    assert!(result.is_err(), "should fail");

    Ok(())
}

#[test]
fn test_external_ip_mapper_find_external_ip_with_explicit_local_ip() -> Result<()> {
    // IPv4 with  explicit local IP, defaults to CandidateTypeHost
    let m = ExternalIpMapper::new(
        CandidateType::Unspecified,
        &[
            "1.2.3.4/10.0.0.1".to_owned(),
            "1.2.3.5/10.0.0.2".to_owned(),
            "2200::1/fe80::1".to_owned(),
            "2200::2/fe80::2".to_owned(),
        ],
    )?
    .unwrap();

    // find external IPv4
    let ext_ip = m.find_external_ip("10.0.0.1")?;
    assert_eq!(ext_ip.to_string(), "1.2.3.4", "should match");

    let ext_ip = m.find_external_ip("10.0.0.2")?;
    assert_eq!(ext_ip.to_string(), "1.2.3.5", "should match");

    let result = m.find_external_ip("10.0.0.3");
    assert!(result.is_err(), "should fail");

    // find external IPv6
    let ext_ip = m.find_external_ip("fe80::0001")?; // use '0001' instead of '1' on purpose
    assert_eq!(ext_ip.to_string(), "2200::1", "should match");

    let ext_ip = m.find_external_ip("fe80::0002")?; // use '0002' instead of '2' on purpose
    assert_eq!(ext_ip.to_string(), "2200::2", "should match");

    let result = m.find_external_ip("fe80::3");
    assert!(result.is_err(), "should fail");

    // Bad local IP string
    let result = m.find_external_ip("really.bad");
    assert!(result.is_err(), "should fail");

    Ok(())
}

#[test]
fn test_external_ip_mapper_find_external_ip_with_empty_map() -> Result<()> {
    let m = ExternalIpMapper::new(CandidateType::Unspecified, &["1.2.3.4".to_owned()])?.unwrap();

    // attempt to find IPv6 that does not exist in the map
    let result = m.find_external_ip("fe80::1");
    assert!(result.is_err(), "should fail");

    let m = ExternalIpMapper::new(CandidateType::Unspecified, &["2200::1".to_owned()])?.unwrap();

    // attempt to find IPv4 that does not exist in the map
    let result = m.find_external_ip("10.0.0.1");
    assert!(result.is_err(), "should fail");

    Ok(())
}
