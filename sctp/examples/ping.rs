use proto::{ClientConfig, PayloadProtocolIdentifier, ReliabilityType};
use webrtc_sctp::{Endpoint, NewAssociation};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use clap::{App, AppSettings, Arg};
use std::time::Instant;

// RUST_LOG=trace cargo run --color=always --package webrtc-sctp --example ping -- --server 0.0.0.0:5678

use std::io::Write;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} [{}] {} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.level(),
                chrono::Local::now().format("%H:%M:%S.%6f"),
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Trace)
        .init();

    let mut app = App::new("SCTP Ping")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of SCTP Client")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("server")
                .required_unless("FULLHELP")
                .takes_value(true)
                .long("server")
                .help("SCTP Server name."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let server = matches.value_of("server").unwrap();

    let start = Instant::now();
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
    endpoint.set_default_client_config(ClientConfig::new());

    println!("connecting {}..", server);
    let new_conn = endpoint
        .connect(server.parse().unwrap())?
        .await
        .map_err(|e| anyhow!("failed to connect: {}", e))?;

    println!("connected at {:?}", start.elapsed());

    let NewAssociation {
        association: conn, ..
    } = new_conn;

    let (mut send_stream, mut recv_stream) = conn
        .open_stream(0, PayloadProtocolIdentifier::String)
        .await
        .map_err(|e| anyhow!("failed to open stream: {}", e))?;

    println!("opened a stream");

    // set unordered = true and 10ms treshold for dropping packets
    send_stream.set_reliability_params(true, ReliabilityType::Timed, 10)?;

    let ping_msg = format!("ping {}", 0);
    println!("sent: {}", ping_msg);
    send_stream.write(&Bytes::from(ping_msg)).await?;

    println!("waiting pong...");
    let mut buff = vec![0u8; 1024];
    if let Ok(Some(n)) = recv_stream.read(&mut buff).await {
        let pong_msg = String::from_utf8(buff[..n].to_vec()).unwrap();
        println!("received: {}", pong_msg);
    }

    println!("finished recv pong");

    /*
    let stream_tx = Arc::clone(&stream);
    tokio::spawn(async move {
        let mut ping_seq_num = 0;
        while ping_seq_num < 10 {
            let ping_msg = format!("ping {}", ping_seq_num);
            println!("sent: {}", ping_msg);
            stream_tx.write(&Bytes::from(ping_msg)).await?;

            ping_seq_num += 1;
        }

        println!("finished send ping");
        Result::<(), Error>::Ok(())
    });

    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
    let stream_rx = Arc::clone(&stream);
    tokio::spawn(async move {
        let mut buff = vec![0u8; 1024];
        while let Ok(n) = stream_rx.read(&mut buff).await {
            let pong_msg = String::from_utf8(buff[..n].to_vec()).unwrap();
            println!("received: {}", pong_msg);
        }

        println!("finished recv pong");
        drop(done_tx);
    });
     */

    send_stream.finish()?;

    println!("close association");
    conn.close(0u16.into(), b"done");

    println!("wait until endpoint idle");
    // Give the server a fair chance to receive the close packet
    //endpoint.wait_idle().await;

    Ok(())
}
