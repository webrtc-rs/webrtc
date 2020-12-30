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
    let matches = App::new("STUN Client")
        .version("0.1.0")
        .author("Rain Liu <yuliu@outlook.com>")
        .about("An example of STUN Client")
        .arg(Arg::with_name("server").required(true).help("STUN Server"))
        .get_matches();

    let server = matches
        .value_of("server")
        .unwrap_or("stun.l.google.com:19302");

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
