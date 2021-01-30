use super::*;

use util::Error;

#[test]
fn test_lt_cred() -> Result<(), Error> {
    let username = "1599491771";
    let shared_secret = "foobar";

    let expected_password = "Tpz/nKkyvX/vMSLKvL4sbtBt8Vs=";
    let actual_password = long_term_credentials(username, shared_secret)?;
    assert_eq!(
        expected_password, actual_password,
        "Expected {}, got {}",
        expected_password, actual_password
    );

    Ok(())
}

/*TODO:
func TestNewLongTermAuthHandler(t *testing.T) {
    const sharedSecret = "HELLO_WORLD"

    serverListener, err := net.ListenPacket("udp4", "0.0.0.0:3478")
    assert.NoError(t, err)

    server, err := NewServer(ServerConfig{
        AuthHandler: NewLongTermAuthHandler(sharedSecret, nil),
        PacketConnConfigs: []PacketConnConfig{
            {
                PacketConn: serverListener,
                RelayAddressGenerator: &RelayAddressGeneratorStatic{
                    RelayAddress: net.ParseIP("127.0.0.1"),
                    Address:      "0.0.0.0",
                },
            },
        },
        Realm:         "pion.ly",
        LoggerFactory: logging.NewDefaultLoggerFactory(),
    })
    assert.NoError(t, err)

    conn, err := net.ListenPacket("udp4", "0.0.0.0:0")
    assert.NoError(t, err)

    username, password, err := GenerateLongTermCredentials(sharedSecret, time.Minute)
    assert.NoError(t, err)

    client, err := NewClient(&ClientConfig{
        STUNServerAddr: "0.0.0.0:3478",
        TURNServerAddr: "0.0.0.0:3478",
        Conn:           conn,
        Username:       username,
        Password:       password,
        LoggerFactory:  logging.NewDefaultLoggerFactory(),
    })
    assert.NoError(t, err)
    assert.NoError(t, client.Listen())

    relayConn, err := client.Allocate()
    assert.NoError(t, err)

    client.Close()
    assert.NoError(t, relayConn.Close())
    assert.NoError(t, conn.Close())
    assert.NoError(t, server.Close())
}
 */
