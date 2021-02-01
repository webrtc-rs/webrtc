use webrtc_rs_mdns as mdns;

use mdns::{config::*, conn::*};

use clap::{App, AppSettings, Arg};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use tokio::sync::mpsc;
use util::Error;

// For interop with webrtc-rs/mdns_server
// cargo run --color=always --package webrtc-rs-mdns --example mdns_query

// For interop with pion/mdns_server:
// cargo run --color=always --package webrtc-rs-mdns --example mdns_query -- --local-name pion-test.local

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let mut app = App::new("mDNS Query")
        .version("0.1.0")
        .author("Rain Liu <yuliu@webrtc.rs>")
        .about("An example of mDNS Query")
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
                .default_value("0.0.0.0")
                .long("host")
                .help("mDNS Server name."),
        )
        .arg(
            Arg::with_name("port")
                .takes_value(true)
                .default_value("5353")
                .long("port")
                .help("Listening port."),
        )
        .arg(
            Arg::with_name("local-name")
                .long("local-name")
                .takes_value(true)
                .default_value("webrtc-rs-test.local")
                .help("Local name"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let host = matches.value_of("host").unwrap();
    let port = matches.value_of("port").unwrap();
    let local_name = matches.value_of("local-name").unwrap();

    let server = DNSConn::server(
        SocketAddr::new(IpAddr::from_str(host)?, port.parse()?),
        Config {
            ..Default::default()
        },
    )
    .unwrap();

    log::info!("querying dns");

    let (_a, b) = mpsc::channel(1);

    let (answer, src) = server.query(local_name, b).await.unwrap();
    log::info!("dns queried");
    println!("answer = {}, src = {}", answer, src);

    server.close().await.unwrap();

    Ok(())
}
