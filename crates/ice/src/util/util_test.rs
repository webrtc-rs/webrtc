use super::*;

#[tokio::test]
async fn test_local_interfaces() -> Result<(), Error> {
    let vnet = Arc::new(Mutex::new(Net::new(None)));
    let interfaces = {
        let n = vnet.lock().await;
        n.get_interfaces().to_vec()
    };
    let ips = local_interfaces(&vnet, &None, &[NetworkType::UDP4, NetworkType::UDP6]).await;
    log::info!("interfaces: {:?}, ips: {:?}", interfaces, ips);
    Ok(())
}
