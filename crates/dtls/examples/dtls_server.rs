use anyhow::Result;
use clap::{App, AppSettings, Arg};
use std::io::Write;
use std::str;
use std::sync::Arc;
use tokio::net::UdpSocket;
use util::{conn::conn_disconnected_packet::DisconnectedPacketConn, Conn};
use webrtc_dtls::{
    config::Config, conn::DTLSConn, crypto::Certificate,
    extension::extension_use_srtp::SrtpProtectionProfile,
};

async fn create_server(
    cb: Arc<dyn Conn + Send + Sync>,
    mut cfg: Config,
    generate_certificate: bool,
) -> Result<impl Conn> {
    if generate_certificate {
        let server_cert = Certificate::generate_self_signed(vec!["localhost".to_owned()])?;
        cfg.certificates = vec![server_cert];
    }

    DTLSConn::new(cb, cfg, false, None).await
}

// cargo run --color=always --package webrtc-dtls --example dtls_server -- --host 0.0.0.0:5678

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

    let mut app = App::new("DTLS Server")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of DTLS Server")
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
                .help("DTLS host name."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let host = matches.value_of("host").unwrap();
    let conn = DisconnectedPacketConn::new(Arc::new(UdpSocket::bind(host).await?));
    println!("listening {}...", conn.local_addr().await?);

    let cfg = Config {
        srtp_protection_profiles: vec![SrtpProtectionProfile::Srtp_Aes128_Cm_Hmac_Sha1_80],
        ..Default::default()
    };
    let dtls_conn = create_server(Arc::new(conn), cfg, true).await?;

    let mut buf = [0; 1024];
    let n = dtls_conn.recv(&mut buf).await?;
    println!("{}", str::from_utf8(&buf[..n])?);

    let message = "hello world from dtls server";
    dtls_conn.send(message.as_bytes()).await?;

    dtls_conn.close().await
}
