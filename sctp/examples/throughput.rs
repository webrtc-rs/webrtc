use clap::{App, AppSettings, Arg};
use std::io::Write;
use std::sync::Arc;
use tokio::net::UdpSocket;
use util::{conn::conn_disconnected_packet::DisconnectedPacketConn, Conn};
use webrtc_sctp::association::*;
use webrtc_sctp::chunk::chunk_payload_data::PayloadProtocolIdentifier;
use webrtc_sctp::stream::*;
use webrtc_sctp::Error;

fn main() -> Result<(), Error> {
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
        .filter(None, log::LevelFilter::Warn)
        .init();

    let mut app = App::new("SCTP Throughput")
        .version("0.1.0")
        .about("An example of SCTP Server")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("port")
                .required_unless("FULLHELP")
                .takes_value(true)
                .long("port")
                .help("use port ."),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let port1 = matches.value_of("port").unwrap().to_owned();
    let port2 = port1.clone();

    std::thread::spawn(|| {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let conn = DisconnectedPacketConn::new(Arc::new(
                    UdpSocket::bind(format!("127.0.0.1:{port1}")).await.unwrap(),
                ));
                println!("listening {}...", conn.local_addr().unwrap());

                let config = Config {
                    net_conn: Arc::new(conn),
                    max_receive_buffer_size: 0,
                    max_message_size: 0,
                    name: "recver".to_owned(),
                };
                let a = Association::server(config).await?;
                println!("created a server");

                let stream = a.accept_stream().await.unwrap();
                println!("accepted a stream");

                // set unordered = true and 10ms treshold for dropping packets
                stream.set_reliability_params(true, ReliabilityType::Rexmit, 0);

                let mut buff = [0u8; 65535];
                let mut recv = 0;
                let mut pkt_num = 0;
                let mut loop_num = 0;
                let mut now = tokio::time::Instant::now();
                while let Ok(n) = stream.read(&mut buff).await {
                    recv += n;
                    if n != 0 {
                        pkt_num += 1;
                    }
                    loop_num += 1;
                    if now.elapsed().as_secs() == 1 {
                        println!("Throughput: {recv} Bytes/s, {pkt_num} pkts, {loop_num} loops");
                        now = tokio::time::Instant::now();
                        recv = 0;
                        loop_num = 0;
                        pkt_num = 0;
                    }
                }
                Result::<(), Error>::Ok(())
            })
    });

    std::thread::spawn(|| {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await.unwrap());
                conn.connect(format!("127.0.0.1:{port2}")).await.unwrap();
                println!("connecting 127.0.0.1:{port2}..");

                let config = Config {
                    net_conn: conn,
                    max_receive_buffer_size: 0,
                    max_message_size: 0,
                    name: "sender".to_owned(),
                };
                let a = Association::client(config).await.unwrap();
                println!("created a client");

                let stream = a
                    .open_stream(0, PayloadProtocolIdentifier::Binary)
                    .await
                    .unwrap();
                println!("opened a stream");

                //const LEN: usize = 1200;
                const LEN: usize = 65535;
                let buf = vec![0; LEN];
                let bytes = bytes::Bytes::from(buf);

                let mut now = tokio::time::Instant::now();
                let mut pkt_num = 0;
                while stream.write(&bytes).await.is_ok() {
                    pkt_num += 1;
                    if now.elapsed().as_secs() == 1 {
                        println!("Send {pkt_num} pkts");
                        now = tokio::time::Instant::now();
                        pkt_num = 0;
                    }
                }
                Result::<(), Error>::Ok(())
            })
    });
    #[allow(clippy::empty_loop)]
    loop {}
}
