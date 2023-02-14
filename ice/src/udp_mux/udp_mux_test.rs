use std::convert::TryInto;
use std::io;
use std::time::Duration;

use super::*;
use crate::error::Result;
use stun::message::{Message, BINDING_REQUEST};

use tokio::net::UdpSocket;
use tokio::time::{sleep, timeout};

use rand::{thread_rng, Rng};
use sha1::{Digest, Sha1};

#[derive(Debug, Copy, Clone)]
enum Network {
    Ipv4,
    Ipv6,
}

impl Network {
    /// Bind the UDP socket for the "remote".
    async fn bind(self) -> io::Result<UdpSocket> {
        match self {
            Network::Ipv4 => UdpSocket::bind("0.0.0.0:0").await,
            Network::Ipv6 => UdpSocket::bind("[::]:0").await,
        }
    }

    /// Connnect ip from the "remote".
    fn connect_ip(self, port: u16) -> String {
        match self {
            Network::Ipv4 => format!("127.0.0.1:{port}"),
            Network::Ipv6 => format!("[::1]:{port}"),
        }
    }
}

const TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::test]
async fn test_udp_mux() -> Result<()> {
    use std::io::Write;
    env_logger::Builder::from_default_env()
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
        .init();

    // TODO: Support IPv6 dual stack. This works Linux and macOS, but not Windows.
    #[cfg(all(unix, target_pointer_width = "64"))]
    let udp_socket = UdpSocket::bind((std::net::Ipv6Addr::UNSPECIFIED, 0)).await?;

    #[cfg(any(not(unix), not(target_pointer_width = "64")))]
    let udp_socket = UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).await?;

    let addr = udp_socket.local_addr()?;
    log::info!("Listening on {}", addr);

    let udp_mux = UDPMuxDefault::new(UDPMuxParams::new(udp_socket));
    let udp_mux_dyn = Arc::clone(&udp_mux) as Arc<dyn UDPMux + Send + Sync>;

    let udp_mux_dyn_1 = Arc::clone(&udp_mux_dyn);
    let h1 = tokio::spawn(async move {
        timeout(
            TIMEOUT,
            test_mux_connection(Arc::clone(&udp_mux_dyn_1), "ufrag1", addr, Network::Ipv4),
        )
        .await
    });

    let udp_mux_dyn_2 = Arc::clone(&udp_mux_dyn);
    let h2 = tokio::spawn(async move {
        timeout(
            TIMEOUT,
            test_mux_connection(Arc::clone(&udp_mux_dyn_2), "ufrag2", addr, Network::Ipv4),
        )
        .await
    });

    let all_results;

    #[cfg(all(unix, target_pointer_width = "64"))]
    {
        // TODO: Support IPv6 dual stack. This works Linux and macOS, but not Windows.
        let udp_mux_dyn_3 = Arc::clone(&udp_mux_dyn);
        let h3 = tokio::spawn(async move {
            timeout(
                TIMEOUT,
                test_mux_connection(Arc::clone(&udp_mux_dyn_3), "ufrag3", addr, Network::Ipv6),
            )
            .await
        });

        let (r1, r2, r3) = tokio::join!(h1, h2, h3);
        all_results = [r1, r2, r3];
    }

    #[cfg(any(not(unix), not(target_pointer_width = "64")))]
    {
        let (r1, r2) = tokio::join!(h1, h2);
        all_results = [r1, r2];
    }

    for timeout_result in &all_results {
        // Timeout error
        match timeout_result {
            Err(timeout_err) => {
                panic!("Mux test timedout: {timeout_err:?}");
            }

            // Join error
            Ok(join_result) => match join_result {
                Err(err) => {
                    panic!("Mux test failed with join error: {err:?}");
                }
                // Actual error
                Ok(mux_result) => {
                    if let Err(err) = mux_result {
                        panic!("Mux test failed with error: {err:?}");
                    }
                }
            },
        }
    }

    let timeout = all_results.iter().find_map(|r| r.as_ref().err());
    assert!(
        timeout.is_none(),
        "At least one of the muxed tasks timedout {all_results:?}"
    );

    let res = udp_mux.close().await;
    assert!(res.is_ok());
    let res = udp_mux.get_conn("failurefrag").await;

    assert!(
        res.is_err(),
        "Getting connections after UDPMuxDefault is closed should fail"
    );

    Ok(())
}

async fn test_mux_connection(
    mux: Arc<dyn UDPMux + Send + Sync>,
    ufrag: &str,
    listener_addr: SocketAddr,
    network: Network,
) -> Result<()> {
    let conn = mux.get_conn(ufrag).await?;
    // FIXME: Cleanup

    let connect_addr = network
        .connect_ip(listener_addr.port())
        .parse::<SocketAddr>()
        .unwrap();

    let remote_connection = Arc::new(network.bind().await?);
    log::info!("Bound for ufrag: {}", ufrag);
    remote_connection.connect(connect_addr).await?;
    log::info!("Connected to {} for ufrag: {}", connect_addr, ufrag);
    log::info!(
        "Testing muxing from {} over {}",
        remote_connection.local_addr().unwrap(),
        listener_addr
    );

    // These bytes should be dropped
    remote_connection.send("Droppped bytes".as_bytes()).await?;

    sleep(Duration::from_millis(1)).await;

    let stun_msg = {
        let mut m = Message {
            typ: BINDING_REQUEST,
            ..Message::default()
        };

        m.add(ATTR_USERNAME, format!("{ufrag}:otherufrag").as_bytes());

        m.marshal_binary().unwrap()
    };

    let remote_connection_addr = remote_connection.local_addr()?;

    conn.send_to(&stun_msg, remote_connection_addr).await?;

    let mut buffer = vec![0u8; RECEIVE_MTU];
    let len = remote_connection.recv(&mut buffer).await?;
    assert_eq!(buffer[..len], stun_msg);

    const TARGET_SIZE: usize = 1024 * 1024;

    // Read on the muxed side
    let conn_2 = Arc::clone(&conn);
    let mux_handle = tokio::spawn(async move {
        let conn = conn_2;

        let mut buffer = vec![0u8; RECEIVE_MTU];
        let mut next_sequence = 0;
        let mut read = 0;

        while read < TARGET_SIZE {
            let (n, _) = conn
                .recv_from(&mut buffer)
                .await
                .expect("recv_from should not error");
            assert_eq!(n, RECEIVE_MTU);

            verify_packet(&buffer[..n], next_sequence);

            conn.send_to(&buffer[..n], remote_connection_addr)
                .await
                .expect("Failed to write to muxxed connection");

            read += n;
            log::debug!("Muxxed read {}, sequence: {}", read, next_sequence);
            next_sequence += 1;
        }
    });

    let remote_connection_2 = Arc::clone(&remote_connection);
    let remote_handle = tokio::spawn(async move {
        let remote_connection = remote_connection_2;
        let mut buffer = vec![0u8; RECEIVE_MTU];
        let mut next_sequence = 0;
        let mut read = 0;

        while read < TARGET_SIZE {
            let n = remote_connection
                .recv(&mut buffer)
                .await
                .expect("recv_from should not error");
            assert_eq!(n, RECEIVE_MTU);

            verify_packet(&buffer[..n], next_sequence);
            read += n;
            log::debug!("Remote read {}, sequence: {}", read, next_sequence);
            next_sequence += 1;
        }
    });

    let mut sequence: u32 = 0;
    let mut written = 0;
    let mut buffer = vec![0u8; RECEIVE_MTU];
    while written < TARGET_SIZE {
        thread_rng().fill(&mut buffer[24..]);

        let hash = sha1_hash(&buffer[24..]);
        buffer[4..24].copy_from_slice(&hash);
        buffer[0..4].copy_from_slice(&sequence.to_le_bytes());

        let len = remote_connection.send(&buffer).await?;

        written += len;
        log::debug!("Data written {}, sequence: {}", written, sequence);
        sequence += 1;

        sleep(Duration::from_millis(1)).await;
    }

    let (r1, r2) = tokio::join!(mux_handle, remote_handle);
    assert!(r1.is_ok() && r2.is_ok());

    let res = conn.close().await;
    assert!(res.is_ok(), "Failed to close Conn: {res:?}");

    Ok(())
}

fn verify_packet(buffer: &[u8], next_sequence: u32) {
    let read_sequence = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
    assert_eq!(read_sequence, next_sequence);

    let hash = sha1_hash(&buffer[24..]);
    assert_eq!(hash, buffer[4..24]);
}

fn sha1_hash(buffer: &[u8]) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(&buffer[24..]);

    hasher.finalize().to_vec()
}
