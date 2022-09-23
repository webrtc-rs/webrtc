use proto::{AssociationError, ReliabilityType, ServerConfig};
use webrtc_sctp::{Connecting, Endpoint, NewAssociation, RecvStream, SendStream};

use anyhow::Result;
use bytes::Bytes;
use clap::{App, AppSettings, Arg};
use futures_util::{StreamExt, TryFutureExt};
use log::{error, info};
use std::time::Duration;

// RUST_LOG=trace cargo run --color=always --package webrtc-sctp --example pong -- --host 0.0.0.0:5678

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

    let mut app = App::new("SCTP Pong")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of SCTP Server")
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
                .help("SCTP host name."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let host = matches.value_of("host").unwrap();

    let (endpoint, mut incoming) = Endpoint::server(ServerConfig::new(), host.parse().unwrap())?;
    eprintln!("listening on {}", endpoint.local_addr()?);

    while let Some(conn) = incoming.next().await {
        info!("association incoming");
        tokio::spawn(handle_association(conn).unwrap_or_else(move |e| {
            error!("association failed: {reason}", reason = e.to_string())
        }));
    }

    Ok(())
}

async fn handle_association(conn: Connecting) -> Result<()> {
    let NewAssociation {
        mut incoming_streams,
        ..
    } = conn.await?;

    info!("established");

    // Each stream initiated by the client constitutes a new request.
    while let Some(stream) = incoming_streams.next().await {
        let stream = match stream {
            Err(AssociationError::ApplicationClosed { .. }) => {
                info!("association closed");
                return Ok(());
            }
            Err(e) => {
                info!("association error {}", e);
                return Err(e.into());
            }
            Ok(s) => s,
        };

        tokio::spawn(
            handle_stream(stream)
                .unwrap_or_else(move |e| error!("failed: {reason}", reason = e.to_string())),
        );
    }

    Ok(())
}

async fn handle_stream((mut send_stream, mut recv_stream): (SendStream, RecvStream)) -> Result<()> {
    info!("incoming stream {}", recv_stream.stream_identifier());

    // set unordered = true and 10ms threshold for dropping packets
    send_stream.set_reliability_params(true, ReliabilityType::Timed, 10)?;

    let mut buff = vec![0u8; 1024];
    while let Ok(Some(n)) = recv_stream.read(&mut buff).await {
        let ping_msg = String::from_utf8(buff[..n].to_vec()).unwrap();
        println!("received: {}", ping_msg);

        let pong_msg = format!("pong [{}]", ping_msg);
        println!("sent: {}", pong_msg);
        send_stream.write(&Bytes::from(pong_msg)).await?;

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    println!("finished ping-pong");

    // Gracefully terminate the stream
    send_stream.finish()?;

    info!("complete");
    Ok(())
}
