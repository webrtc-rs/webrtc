use super::*;

#[test]
fn test_local_interfaces() -> Result<(), Error> {
    let interfaces = match ifaces::Interface::get_all() {
        Ok(interfaces) => interfaces,
        Err(err) => return Err(Error::new(err.to_string())),
    };
    let ips = local_interfaces(&None, &[NetworkType::UDP4, NetworkType::UDP6])?;
    log::info!("interfaces: {:?}, ips: {:?}", interfaces, ips);
    Ok(())
}
