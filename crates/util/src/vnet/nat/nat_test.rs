use super::*;
use crate::vnet::chunk::ChunkUDP;
use std::net::SocketAddr;
use std::str::FromStr;

// oic: outbound internal chunk
// oec: outbound external chunk
// iic: inbound internal chunk
// iec: inbound external chunk

const DEMO_IP: &str = "1.2.3.4";

#[test]
fn test_nat_type_default() -> Result<(), Error> {
    let nat = NetworkAddressTranslator::new(NatConfig {
        mapped_ips: vec![IpAddr::from_str(DEMO_IP)?],
        ..Default::default()
    })?;

    assert_eq!(
        EndpointDependencyType::EndpointIndependent,
        nat.nat_type.mapping_behavior,
        "should match"
    );
    assert_eq!(
        EndpointDependencyType::EndpointIndependent,
        nat.nat_type.filtering_behavior,
        "should match"
    );
    assert!(!nat.nat_type.hair_pining, "should be false");
    assert!(!nat.nat_type.port_preservation, "should be false");
    assert_eq!(
        DEFAULT_NAT_MAPPING_LIFE_TIME, nat.nat_type.mapping_life_time,
        "should be false"
    );

    Ok(())
}

#[test]
fn test_nat_mapping_behavior_full_cone_nat() -> Result<(), Error> {
    let mut nat = NetworkAddressTranslator::new(NatConfig {
        nat_type: NATType {
            mapping_behavior: EndpointDependencyType::EndpointIndependent,
            filtering_behavior: EndpointDependencyType::EndpointIndependent,
            hair_pining: false,
            mapping_life_time: Duration::from_secs(30),
            ..Default::default()
        },
        mapped_ips: vec![IpAddr::from_str(DEMO_IP)?],
        ..Default::default()
    })?;

    let src = SocketAddr::from_str("192.168.0.2:1234")?;
    let dst = SocketAddr::from_str("5.6.7.8:5678")?;

    let oic = ChunkUDP::new(src, dst);

    let oec = nat.translate_outbound(&oic)?.unwrap();
    assert_eq!(1, nat.outbound_map.len(), "should match");
    assert_eq!(1, nat.inbound_map.len(), "should match");

    log::debug!("o-original  : {}", oic);
    log::debug!("o-translated: {}", oec);

    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), dst.port()),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    log::debug!("i-original  : {}", iec);

    let iic = nat.translate_inbound(&iec)?.unwrap();

    log::debug!("i-translated: {}", iic);

    assert_eq!(oic.source_addr(), iic.destination_addr(), "should match");

    // packet with dest addr that does not exist in the mapping table
    // will be dropped
    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), dst.port()),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port() + 1),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_err(), "should fail (dropped)");

    // packet from any addr will be accepted (full-cone)
    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), 7777),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_ok(), "should succeed");

    Ok(())
}

#[test]
fn test_nat_mapping_behavior_addr_restricted_cone_nat() -> Result<(), Error> {
    let mut nat = NetworkAddressTranslator::new(NatConfig {
        nat_type: NATType {
            mapping_behavior: EndpointDependencyType::EndpointIndependent,
            filtering_behavior: EndpointDependencyType::EndpointAddrDependent,
            hair_pining: false,
            mapping_life_time: Duration::from_secs(30),
            ..Default::default()
        },
        mapped_ips: vec![IpAddr::from_str(DEMO_IP)?],
        ..Default::default()
    })?;

    let src = SocketAddr::from_str("192.168.0.2:1234")?;
    let dst = SocketAddr::from_str("5.6.7.8:5678")?;

    let oic = ChunkUDP::new(src, dst);
    log::debug!("o-original  : {}", oic);

    let oec = nat.translate_outbound(&oic)?.unwrap();
    assert_eq!(1, nat.outbound_map.len(), "should match");
    assert_eq!(1, nat.inbound_map.len(), "should match");
    log::debug!("o-translated: {}", oec);

    // sending different (IP: 5.6.7.9) won't create a new mapping
    let oic2 = ChunkUDP::new(
        SocketAddr::from_str("192.168.0.2:1234")?,
        SocketAddr::from_str("5.6.7.9:9000")?,
    );
    let oec2 = nat.translate_outbound(&oic2)?.unwrap();
    assert_eq!(1, nat.outbound_map.len(), "should match");
    assert_eq!(1, nat.inbound_map.len(), "should match");
    log::debug!("o-translated: {}", oec2);

    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), dst.port()),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    log::debug!("i-original  : {}", iec);

    let iic = nat.translate_inbound(&iec)?.unwrap();

    log::debug!("i-translated: {}", iic);

    assert_eq!(oic.source_addr(), iic.destination_addr(), "should match");

    // packet with dest addr that does not exist in the mapping table
    // will be dropped
    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), dst.port()),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port() + 1),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_err(), "should fail (dropped)");

    // packet from any port will be accepted (restricted-cone)
    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), 7777),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_ok(), "should succeed");

    // packet from different addr will be droped (restricted-cone)
    let iec = ChunkUDP::new(
        SocketAddr::from_str(&format!("{}:{}", "6.6.6.6", dst.port()))?,
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_err(), "should fail (dropped)");

    Ok(())
}

#[test]
fn test_nat_mapping_behavior_port_restricted_cone_nat() -> Result<(), Error> {
    let mut nat = NetworkAddressTranslator::new(NatConfig {
        nat_type: NATType {
            mapping_behavior: EndpointDependencyType::EndpointIndependent,
            filtering_behavior: EndpointDependencyType::EndpointAddrPortDependent,
            hair_pining: false,
            mapping_life_time: Duration::from_secs(30),
            ..Default::default()
        },
        mapped_ips: vec![IpAddr::from_str(DEMO_IP)?],
        ..Default::default()
    })?;

    let src = SocketAddr::from_str("192.168.0.2:1234")?;
    let dst = SocketAddr::from_str("5.6.7.8:5678")?;

    let oic = ChunkUDP::new(src, dst);
    log::debug!("o-original  : {}", oic);

    let oec = nat.translate_outbound(&oic)?.unwrap();
    assert_eq!(1, nat.outbound_map.len(), "should match");
    assert_eq!(1, nat.inbound_map.len(), "should match");
    log::debug!("o-translated: {}", oec);

    // sending different (IP: 5.6.7.9) won't create a new mapping
    let oic2 = ChunkUDP::new(
        SocketAddr::from_str("192.168.0.2:1234")?,
        SocketAddr::from_str("5.6.7.9:9000")?,
    );
    let oec2 = nat.translate_outbound(&oic2)?.unwrap();
    assert_eq!(1, nat.outbound_map.len(), "should match");
    assert_eq!(1, nat.inbound_map.len(), "should match");
    log::debug!("o-translated: {}", oec2);

    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), dst.port()),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    log::debug!("i-original  : {}", iec);

    let iic = nat.translate_inbound(&iec)?.unwrap();

    log::debug!("i-translated: {}", iic);

    assert_eq!(oic.source_addr(), iic.destination_addr(), "should match");

    // packet with dest addr that does not exist in the mapping table
    // will be dropped
    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), dst.port()),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port() + 1),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_err(), "should fail (dropped)");

    // packet from different port will be dropped (port-restricted-cone)
    let iec = ChunkUDP::new(
        SocketAddr::new(dst.ip(), 7777),
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_err(), "should fail (dropped)");

    // packet from different addr will be droped (restricted-cone)
    let iec = ChunkUDP::new(
        SocketAddr::from_str(&format!("{}:{}", "6.6.6.6", dst.port()))?,
        SocketAddr::new(oec.source_addr().ip(), oec.source_addr().port()),
    );

    let result = nat.translate_inbound(&iec);
    assert!(result.is_err(), "should fail (dropped)");

    Ok(())
}

#[test]
fn test_nat_mapping_behavior_symmetric_nat_addr_dependent_mapping() -> Result<(), Error> {
    let mut nat = NetworkAddressTranslator::new(NatConfig {
        nat_type: NATType {
            mapping_behavior: EndpointDependencyType::EndpointAddrDependent,
            filtering_behavior: EndpointDependencyType::EndpointAddrDependent,
            hair_pining: false,
            mapping_life_time: Duration::from_secs(30),
            ..Default::default()
        },
        mapped_ips: vec![IpAddr::from_str(DEMO_IP)?],
        ..Default::default()
    })?;

    let src = SocketAddr::from_str("192.168.0.2:1234")?;
    let dst1 = SocketAddr::from_str("5.6.7.8:5678")?;
    let dst2 = SocketAddr::from_str("5.6.7.100:5678")?;
    let dst3 = SocketAddr::from_str("5.6.7.8:6000")?;

    let oic1 = ChunkUDP::new(src, dst1);
    let oic2 = ChunkUDP::new(src, dst2);
    let oic3 = ChunkUDP::new(src, dst3);

    log::debug!("o-original  : {}", oic1);
    log::debug!("o-original  : {}", oic2);
    log::debug!("o-original  : {}", oic3);

    let oec1 = nat.translate_outbound(&oic1)?.unwrap();
    let oec2 = nat.translate_outbound(&oic2)?.unwrap();
    let oec3 = nat.translate_outbound(&oic3)?.unwrap();

    assert_eq!(2, nat.outbound_map.len(), "should match");
    assert_eq!(2, nat.inbound_map.len(), "should match");

    log::debug!("o-translated: {}", oec1);
    log::debug!("o-translated: {}", oec2);
    log::debug!("o-translated: {}", oec3);

    assert_ne!(
        oec1.source_addr().port(),
        oec2.source_addr().port(),
        "should not match"
    );
    assert_eq!(
        oec1.source_addr().port(),
        oec3.source_addr().port(),
        "should match"
    );

    Ok(())
}

#[test]
fn test_nat_mapping_behavior_symmetric_nat_port_dependent_mapping() -> Result<(), Error> {
    let mut nat = NetworkAddressTranslator::new(NatConfig {
        nat_type: NATType {
            mapping_behavior: EndpointDependencyType::EndpointAddrPortDependent,
            filtering_behavior: EndpointDependencyType::EndpointAddrPortDependent,
            hair_pining: false,
            mapping_life_time: Duration::from_secs(30),
            ..Default::default()
        },
        mapped_ips: vec![IpAddr::from_str(DEMO_IP)?],
        ..Default::default()
    })?;

    let src = SocketAddr::from_str("192.168.0.2:1234")?;
    let dst1 = SocketAddr::from_str("5.6.7.8:5678")?;
    let dst2 = SocketAddr::from_str("5.6.7.100:5678")?;
    let dst3 = SocketAddr::from_str("5.6.7.8:6000")?;

    let oic1 = ChunkUDP::new(src, dst1);
    let oic2 = ChunkUDP::new(src, dst2);
    let oic3 = ChunkUDP::new(src, dst3);

    log::debug!("o-original  : {}", oic1);
    log::debug!("o-original  : {}", oic2);
    log::debug!("o-original  : {}", oic3);

    let oec1 = nat.translate_outbound(&oic1)?.unwrap();
    let oec2 = nat.translate_outbound(&oic2)?.unwrap();
    let oec3 = nat.translate_outbound(&oic3)?.unwrap();

    assert_eq!(3, nat.outbound_map.len(), "should match");
    assert_eq!(3, nat.inbound_map.len(), "should match");

    log::debug!("o-translated: {}", oec1);
    log::debug!("o-translated: {}", oec2);
    log::debug!("o-translated: {}", oec3);

    assert_ne!(
        oec1.source_addr().port(),
        oec2.source_addr().port(),
        "should not match"
    );
    assert_ne!(
        oec1.source_addr().port(),
        oec3.source_addr().port(),
        "should match"
    );

    Ok(())
}
