use webrtc_rs_stun as stun;

use stun::agent::*;
use stun::client::*;
use stun::message::*;
use stun::xoraddr::*;

use clap::{App, Arg};
use std::sync::Arc;
use tokio::net::UdpSocket;
use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut app = App::new("STUN Client")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of STUN Client")
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("server")
                .required_unless("FULLHELP")
                .takes_value(true)
                .default_value("stun.l.google.com:19302")
                .long("server")
                .help("STUN Server"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let server = matches.value_of("server").unwrap();

    println!("Connecting {}...", server);

    let (handler_tx, mut handler_rx) = tokio::sync::mpsc::unbounded_channel();

    let conn = UdpSocket::bind("::0:0").await?;
    println!("local address: {}", conn.local_addr()?);

    conn.connect(server).await?;

    let mut client = ClientBuilder::new().with_conn(Arc::new(conn)).build()?;

    let mut msg = Message::new();
    msg.build(&[
        Box::new(TransactionId::default()),
        Box::new(BINDING_REQUEST),
    ])?;

    client.send(&msg, Some(Arc::new(handler_tx))).await?;

    if let Some(event) = handler_rx.recv().await {
        match event.event_body {
            Ok(msg) => {
                let mut xor_addr = XORMappedAddress::default();
                xor_addr.get_from(&msg)?;
                println!("{}", xor_addr);
            }
            Err(err) => println!("{:?}", err),
        };
    }

    client.close().await?;

    Ok(())
}
