use clap::{App, AppSettings, Arg};
use std::fs::File;
use std::io::{BufReader, Write};
use std::sync::Arc;
use util::conn::*;
use webrtc_dtls::config::{ClientAuthType, ExtendedMasterSecretType};
use webrtc_dtls::Error;
use webrtc_dtls::{config::Config, listener::listen};

// cargo run --example listen_verify -- --host 127.0.0.1:4444

#[tokio::main]
async fn main() -> Result<(), Error> {
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
                .default_value("127.0.0.1:4444")
                .long("host")
                .help("DTLS host name."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let host = matches.value_of("host").unwrap().to_owned();

    let certificate = hub::utilities::load_key_and_certificate(
        "examples/certificates/server.pem.private_key.pem".into(),
        "examples/certificates/server.pub.pem".into(),
    )?;

    let mut cert_pool = rustls::RootCertStore::empty();
    let f = File::open("examples/certificates/server.pub.pem")?;
    let mut reader = BufReader::new(f);
    if cert_pool.add_pem_file(&mut reader).is_err() {
        return Err(Error::Other("cert_pool add_pem_file failed".to_owned()));
    }

    let cfg = Config {
        certificates: vec![certificate],
        extended_master_secret: ExtendedMasterSecretType::Require,
        client_auth: ClientAuthType::RequireAndVerifyClientCert, //RequireAnyClientCert, //
        client_cas: cert_pool,
        ..Default::default()
    };

    println!("listening {host}...\ntype 'exit' to shutdown gracefully");

    let listener = Arc::new(listen(host, cfg).await?);

    // Simulate a chat session
    let h = Arc::new(hub::Hub::new());

    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
    let mut done_tx = Some(done_tx);

    let listener2 = Arc::clone(&listener);
    let h2 = Arc::clone(&h);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = done_rx.recv() => {
                    break;
                }
                result = listener2.accept() => {
                    match result{
                        Ok((dtls_conn, _)) => {
                            // Register the connection with the chat hub
                            h2.register(dtls_conn).await;
                        }
                        Err(err) => {
                            println!("connecting failed with error: {err}");
                        }
                    }
                }
            }
        }
    });

    h.chat().await;

    done_tx.take();

    Ok(listener.close().await?)
}
