use std::net::Shutdown;
use std::sync::Arc;

use bytes::Bytes;
use clap::{App, AppSettings, Arg};
use tokio::net::UdpSocket;
use tokio::signal;
use tokio::sync::mpsc;
use webrtc_sctp::association::*;
use webrtc_sctp::chunk::chunk_payload_data::PayloadProtocolIdentifier;
use webrtc_sctp::stream::*;
use webrtc_sctp::Error;

// RUST_LOG=trace cargo run --color=always --package webrtc-sctp --example ping -- --server 0.0.0.0:5678

#[tokio::main]
async fn main() -> Result<(), Error> {
    /*env_logger::Builder::new()
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
    .init();*/

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

    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await.unwrap());
    conn.connect(server).await.unwrap();
    println!("connecting {server}..");

    let config = Config {
        net_conn: conn,
        max_receive_buffer_size: 0,
        max_message_size: 0,
        name: "client".to_owned(),
    };
    let a = Association::client(config).await?;
    println!("created a client");

    let stream = a.open_stream(0, PayloadProtocolIdentifier::String).await?;
    println!("opened a stream");

    // set unordered = true and 10ms threshold for dropping packets
    stream.set_reliability_params(true, ReliabilityType::Timed, 10);

    let stream_tx = Arc::clone(&stream);
    tokio::spawn(async move {
        let mut ping_seq_num = 0;
        while ping_seq_num < 10 {
            let ping_msg = format!("ping {ping_seq_num}");
            println!("sent: {ping_msg}");
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
            println!("received: {pong_msg}");
        }

        println!("finished recv pong");
        drop(done_tx);
    });

    println!("Waiting for Ctrl-C...");
    signal::ctrl_c().await.expect("failed to listen for event");
    println!("Closing stream and association...");

    stream.shutdown(Shutdown::Both).await?;
    a.close().await?;

    let _ = done_rx.recv().await;

    Ok(())
}
