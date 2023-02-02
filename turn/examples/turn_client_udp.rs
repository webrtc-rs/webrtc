use turn::client::*;
use turn::Error;

use clap::{App, AppSettings, Arg};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::Duration;
use util::Conn;

// RUST_LOG=trace cargo run --color=always --package turn --example turn_client_udp -- --host 0.0.0.0 --user user=pass --ping

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let mut app = App::new("TURN Client UDP")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of TURN Client UDP")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("host")
                .required_unless("FULLHELP")
                .takes_value(true)
                .long("host")
                .help("TURN Server name."),
        )
        .arg(
            Arg::with_name("user")
                .required_unless("FULLHELP")
                .takes_value(true)
                .long("user")
                .help("A pair of username and password (e.g. \"user=pass\")"),
        )
        .arg(
            Arg::with_name("realm")
                .default_value("webrtc.rs")
                .takes_value(true)
                .long("realm")
                .help("Realm (defaults to \"webrtc.rs\")"),
        )
        .arg(
            Arg::with_name("port")
                .takes_value(true)
                .default_value("3478")
                .long("port")
                .help("Listening port."),
        )
        .arg(
            Arg::with_name("ping")
                .long("ping")
                .takes_value(false)
                .help("Run ping test"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let host = matches.value_of("host").unwrap();
    let port = matches.value_of("port").unwrap();
    let user = matches.value_of("user").unwrap();
    let cred: Vec<&str> = user.splitn(2, '=').collect();
    let ping = matches.is_present("ping");
    let realm = matches.value_of("realm").unwrap();

    // TURN client won't create a local listening socket by itself.
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let turn_server_addr = format!("{host}:{port}");

    let cfg = ClientConfig {
        stun_serv_addr: turn_server_addr.clone(),
        turn_serv_addr: turn_server_addr,
        username: cred[0].to_string(),
        password: cred[1].to_string(),
        realm: realm.to_string(),
        software: String::new(),
        rto_in_ms: 0,
        conn: Arc::new(conn),
        vnet: None,
    };

    let client = Client::new(cfg).await?;

    // Start listening on the conn provided.
    client.listen().await?;

    // Allocate a relay socket on the TURN server. On success, it
    // will return a net.PacketConn which represents the remote
    // socket.
    let relay_conn = client.allocate().await?;

    // The relayConn's local address is actually the transport
    // address assigned on the TURN server.
    println!("relayed-address={}", relay_conn.local_addr()?);

    // If you provided `-ping`, perform a ping test agaist the
    // relayConn we have just allocated.
    if ping {
        do_ping_test(&client, relay_conn).await?;
    }

    client.close().await?;

    Ok(())
}

async fn do_ping_test(
    client: &Client,
    relay_conn: impl Conn + std::marker::Send + std::marker::Sync + 'static,
) -> Result<(), Error> {
    // Send BindingRequest to learn our external IP
    let mapped_addr = client.send_binding_request().await?;

    // Set up pinger socket (pingerConn)
    //println!("bind...");
    let pinger_conn_tx = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    // Punch a UDP hole for the relay_conn by sending a data to the mapped_addr.
    // This will trigger a TURN client to generate a permission request to the
    // TURN server. After this, packets from the IP address will be accepted by
    // the TURN server.
    //println!("relay_conn send hello to mapped_addr {}", mapped_addr);
    relay_conn.send_to("Hello".as_bytes(), mapped_addr).await?;
    let relay_addr = relay_conn.local_addr()?;

    let pinger_conn_rx = Arc::clone(&pinger_conn_tx);

    // Start read-loop on pingerConn
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            let (n, from) = match pinger_conn_rx.recv_from(&mut buf).await {
                Ok((n, from)) => (n, from),
                Err(_) => break,
            };

            let msg = match String::from_utf8(buf[..n].to_vec()) {
                Ok(msg) => msg,
                Err(_) => break,
            };

            println!("pingerConn read-loop: {msg} from {from}");
            /*if sentAt, pingerErr := time.Parse(time.RFC3339Nano, msg); pingerErr == nil {
                rtt := time.Since(sentAt)
                log.Printf("%d bytes from from %s time=%d ms\n", n, from.String(), int(rtt.Seconds()*1000))
            }*/
        }
    });

    // Start read-loop on relay_conn
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        loop {
            let (n, from) = match relay_conn.recv_from(&mut buf).await {
                Err(_) => break,
                Ok((n, from)) => (n, from),
            };

            println!("relay_conn read-loop: {:?} from {}", &buf[..n], from);

            // Echo back
            if relay_conn.send_to(&buf[..n], from).await.is_err() {
                break;
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    /*println!(
        "pinger_conn_tx send 10 packets to relay addr {}...",
        relay_addr
    );*/
    // Send 10 packets from relay_conn to the echo server
    for _ in 0..2 {
        let msg = "12345678910".to_owned(); //format!("{:?}", tokio::time::Instant::now());
        println!("sending msg={} with size={}", msg, msg.as_bytes().len());
        pinger_conn_tx.send_to(msg.as_bytes(), relay_addr).await?;

        // For simplicity, this example does not wait for the pong (reply).
        // Instead, sleep 1 second.
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}
