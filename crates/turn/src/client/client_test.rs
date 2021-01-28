use super::*;

use util::Error;

async fn create_listening_test_client(rto_in_ms: u16) -> Result<Client, Error> {
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let c = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: "TEST SOFTWARE".to_owned(),
        rto_in_ms,
        conn: Arc::new(conn),
    })
    .await?;

    c.listen().await?;

    Ok(c)
}

async fn create_listening_test_client_with_stun_serv() -> Result<Client, Error> {
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let c = Client::new(ClientConfig {
        stun_serv_addr: "stun1.l.google.com:19302".to_owned(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: "TEST SOFTWARE".to_owned(),
        rto_in_ms: 0,
        conn: Arc::new(conn),
    })
    .await?;

    c.listen().await?;

    Ok(c)
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request() -> Result<(), Error> {
    //env_logger::init();

    let c = create_listening_test_client_with_stun_serv().await?;

    let resp = c.send_binding_request().await?;
    log::debug!("mapped-addr: {}", resp);
    {
        let ci = c.client_internal.lock().await;
        let tm = ci.tr_map.lock().await;
        assert_eq!(0, tm.size(), "should be no transaction left");
    }

    c.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request_to_parallel() -> Result<(), Error> {
    //env_logger::init();

    let c1 = create_listening_test_client(0).await?;
    let c2 = c1.clone();

    let (stared_tx, mut started_rx) = mpsc::channel::<()>(1);
    let (finished_tx, mut finished_rx) = mpsc::channel::<()>(1);

    let to = lookup_host(true, "stun1.l.google.com:19302").await?;

    tokio::spawn(async move {
        drop(stared_tx);
        if let Ok(resp) = c2.send_binding_request_to(&to.to_string()).await {
            log::debug!("mapped-addr: {}", resp);
        }
        drop(finished_tx);
    });

    let _ = started_rx.recv().await;

    let resp = c1.send_binding_request_to(&to.to_string()).await?;
    log::debug!("mapped-addr: {}", resp);

    let _ = finished_rx.recv().await;

    c1.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request_to_timeout() -> Result<(), Error> {
    //env_logger::init();

    let c = create_listening_test_client(10).await?;

    let to = lookup_host(true, "127.0.0.1:9").await?;

    let result = c.send_binding_request_to(&to.to_string()).await;
    assert!(result.is_err(), "expected error, but got ok");

    c.close().await?;

    Ok(())
}

// Create an allocation, and then delete all nonces
// The subsequent Write on the allocation will cause a CreatePermission
// which will be forced to handle a stale nonce response
#[tokio::test]
async fn test_client_nonce_expiration() -> Result<(), Error> {
    /*TODO: env_logger::init();

    let udpListener = UdpSocket::bind("0.0.0.0:3478").await?;

    server, err := NewServer(ServerConfig{
        auth_handler: func(username, realm string, srcAddr net.Addr) (key []byte, ok bool) {
            return GenerateAuthKey(username, realm, "pass"), true
        },
        PacketConnConfigs: []PacketConnConfig{
            {
                PacketConn: udpListener,
                RelayAddressGenerator: &RelayAddressGeneratorStatic{
                    RelayAddress: net.ParseIP("127.0.0.1"),
                    Address:      "0.0.0.0",
                },
            },
        },
        realm: "pion.ly",
    })
    assert.NoError(t, err)

    conn, err := net.ListenPacket("udp4", "0.0.0.0:0")
    assert.NoError(t, err)

    client, err := NewClient(&ClientConfig{
        Conn:           conn,
        STUNServerAddr: "127.0.0.1:3478",
        TURNServerAddr: "127.0.0.1:3478",
        Username:       "foo",
        Password:       "pass",
    })
    assert.NoError(t, err)
    assert.NoError(t, client.Listen())

    allocation, err := client.Allocate()
    assert.NoError(t, err)

    server.nonces.Range(func(key, value interface{}) bool {
        server.nonces.Delete(key)
        return true
    })

    _, err = allocation.WriteTo([]byte{0x00}, &net.UDPAddr{IP: net.ParseIP("127.0.0.1"), Port: 8080})
    assert.NoError(t, err)

    // Shutdown
    assert.NoError(t, allocation.Close())
    assert.NoError(t, conn.Close())
    assert.NoError(t, server.Close())*/

    Ok(())
}
