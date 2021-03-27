use super::*;

#[tokio::test]
async fn test_local_interfaces() -> Result<(), Error> {
    let vnet = Arc::new(Net::new(None));
    let interfaces = vnet.get_interfaces().await;
    let ips = local_interfaces(&vnet, &None, &[NetworkType::UDP4, NetworkType::UDP6]).await;
    log::info!("interfaces: {:?}, ips: {:?}", interfaces, ips);
    Ok(())
}
