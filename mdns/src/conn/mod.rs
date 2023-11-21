use core::sync::atomic;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use socket2::SockAddr;
use tokio::net::{ToSocketAddrs, UdpSocket};
use tokio::sync::{mpsc, Mutex};
use util::ifaces;

use crate::config::*;
use crate::error::*;
use crate::message::header::*;
use crate::message::name::*;
use crate::message::parser::*;
use crate::message::question::*;
use crate::message::resource::a::*;
use crate::message::resource::*;
use crate::message::*;

mod conn_test;

pub const DEFAULT_DEST_ADDR: &str = "224.0.0.251:5353";

const INBOUND_BUFFER_SIZE: usize = 65535;
const DEFAULT_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const MAX_MESSAGE_RECORDS: usize = 3;
const RESPONSE_TTL: u32 = 120;

// Conn represents a mDNS Server
pub struct DnsConn {
    socket: Arc<UdpSocket>,
    dst_addr: SocketAddr,

    query_interval: Duration,
    queries: Arc<Mutex<Vec<Query>>>,

    is_server_closed: Arc<atomic::AtomicBool>,
    close_server: mpsc::Sender<()>,
}

struct Query {
    name_with_suffix: String,
    query_result_chan: mpsc::Sender<QueryResult>,
}

struct QueryResult {
    answer: ResourceHeader,
    addr: SocketAddr,
}

impl DnsConn {
    /// server establishes a mDNS connection over an existing connection
    pub fn server(addr: SocketAddr, config: Config) -> Result<Self> {
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )?;

        #[cfg(feature = "reuse_port")]
        #[cfg(target_family = "unix")]
        socket.set_reuse_port(true)?;

        socket.set_reuse_address(true)?;
        socket.set_broadcast(true)?;
        socket.set_nonblocking(true)?;

        socket.bind(&SockAddr::from(addr))?;
        {
            let mut join_error_count = 0;
            let interfaces = match ifaces::ifaces() {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Error getting interfaces: {:?}", e);
                    return Err(Error::Other(e.to_string()));
                }
            };

            for interface in &interfaces {
                if let Some(SocketAddr::V4(e)) = interface.addr {
                    if let Err(e) = socket.join_multicast_v4(&Ipv4Addr::new(224, 0, 0, 251), e.ip())
                    {
                        log::trace!("Error connecting multicast, error: {:?}", e);
                        join_error_count += 1;
                        continue;
                    }

                    log::trace!("Connected to interface address {:?}", e);
                }
            }

            if join_error_count >= interfaces.len() {
                return Err(Error::ErrJoiningMulticastGroup);
            }
        }

        let socket = UdpSocket::from_std(socket.into())?;

        let local_names = config
            .local_names
            .iter()
            .map(|l| l.to_string() + ".")
            .collect();

        let dst_addr: SocketAddr = DEFAULT_DEST_ADDR.parse()?;

        let is_server_closed = Arc::new(atomic::AtomicBool::new(false));

        let (close_server_send, close_server_rcv) = mpsc::channel(1);

        let c = DnsConn {
            query_interval: if config.query_interval != Duration::from_secs(0) {
                config.query_interval
            } else {
                DEFAULT_QUERY_INTERVAL
            },

            queries: Arc::new(Mutex::new(vec![])),
            socket: Arc::new(socket),
            dst_addr,
            is_server_closed: Arc::clone(&is_server_closed),
            close_server: close_server_send,
        };

        let queries = c.queries.clone();
        let socket = Arc::clone(&c.socket);

        tokio::spawn(async move {
            DnsConn::start(
                close_server_rcv,
                is_server_closed,
                socket,
                local_names,
                dst_addr,
                queries,
            )
            .await
        });

        Ok(c)
    }

    /// Close closes the mDNS Conn
    pub async fn close(&self) -> Result<()> {
        log::info!("Closing connection");
        if self.is_server_closed.load(atomic::Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        log::trace!("Sending close command to server");
        match self.close_server.send(()).await {
            Ok(_) => {
                log::trace!("Close command sent");
                Ok(())
            }
            Err(e) => {
                log::warn!("Error sending close command to server: {:?}", e);
                Err(Error::ErrConnectionClosed)
            }
        }
    }

    /// Query sends mDNS Queries for the following name until
    /// either there's a close signal or we get a result
    pub async fn query(
        &self,
        name: &str,
        mut close_query_signal: mpsc::Receiver<()>,
    ) -> Result<(ResourceHeader, SocketAddr)> {
        if self.is_server_closed.load(atomic::Ordering::SeqCst) {
            return Err(Error::ErrConnectionClosed);
        }

        let name_with_suffix = name.to_owned() + ".";

        let (query_tx, mut query_rx) = mpsc::channel(1);
        {
            let mut queries = self.queries.lock().await;
            queries.push(Query {
                name_with_suffix: name_with_suffix.clone(),
                query_result_chan: query_tx,
            });
        }

        log::trace!("Sending query");
        self.send_question(&name_with_suffix).await;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.query_interval) => {
                    log::trace!("Sending query");
                    self.send_question(&name_with_suffix).await
                },

                _ = close_query_signal.recv() => {
                    log::info!("Query close signal received.");
                    return Err(Error::ErrConnectionClosed)
                },

                res_opt = query_rx.recv() =>{
                    log::info!("Received query result");
                    if let Some(res) = res_opt{
                        return Ok((res.answer, res.addr));
                    }
                }
            }
        }
    }

    async fn send_question(&self, name: &str) {
        let packed_name = match Name::new(name) {
            Ok(pn) => pn,
            Err(err) => {
                log::warn!("Failed to construct mDNS packet: {}", err);
                return;
            }
        };

        let raw_query = {
            let mut msg = Message {
                header: Header::default(),
                questions: vec![Question {
                    typ: DnsType::A,
                    class: DNSCLASS_INET,
                    name: packed_name,
                }],
                ..Default::default()
            };

            match msg.pack() {
                Ok(v) => v,
                Err(err) => {
                    log::error!("Failed to construct mDNS packet {}", err);
                    return;
                }
            }
        };

        log::trace!("{:?} sending {:?}...", self.socket.local_addr(), raw_query);
        if let Err(err) = self.socket.send_to(&raw_query, self.dst_addr).await {
            log::error!("Failed to send mDNS packet {}", err);
        }
    }

    async fn start(
        mut closed_rx: mpsc::Receiver<()>,
        close_server: Arc<atomic::AtomicBool>,
        socket: Arc<UdpSocket>,
        local_names: Vec<String>,
        dst_addr: SocketAddr,
        queries: Arc<Mutex<Vec<Query>>>,
    ) -> Result<()> {
        log::info!("Looping and listening {:?}", socket.local_addr());

        let mut b = vec![0u8; INBOUND_BUFFER_SIZE];
        let (mut n, mut src);

        loop {
            tokio::select! {
                _ = closed_rx.recv() => {
                    log::info!("Closing server connection");
                    close_server.store(true, atomic::Ordering::SeqCst);

                    return Ok(());
                }

                result = socket.recv_from(&mut b) => {
                    match result{
                        Ok((len, addr)) => {
                            n = len;
                            src = addr;
                            log::trace!("Received new connection from {:?}", addr);
                        },

                        Err(err) => {
                            log::error!("Error receiving from socket connection: {:?}", err);
                            continue;
                        },
                    }
                }
            }

            let mut p = Parser::default();
            if let Err(err) = p.start(&b[..n]) {
                log::error!("Failed to parse mDNS packet {}", err);
                continue;
            }

            run(&mut p, &socket, &local_names, src, dst_addr, &queries).await
        }
    }
}

async fn run(
    p: &mut Parser<'_>,
    socket: &Arc<UdpSocket>,
    local_names: &[String],
    src: SocketAddr,
    dst_addr: SocketAddr,
    queries: &Arc<Mutex<Vec<Query>>>,
) {
    let mut interface_addr = None;
    for _ in 0..=MAX_MESSAGE_RECORDS {
        let q = match p.question() {
            Ok(q) => q,
            Err(err) => {
                if Error::ErrSectionDone == err {
                    log::trace!("Parsing has completed");
                    break;
                } else {
                    log::error!("Failed to parse mDNS packet {}", err);
                    return;
                }
            }
        };

        for local_name in local_names {
            if *local_name == q.name.data {
                let interface_addr = match interface_addr {
                    Some(addr) => addr,
                    None => match get_interface_addr_for_ip(src).await {
                        Ok(addr) => {
                            interface_addr.replace(addr);
                            addr
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to get local interface to communicate with {}: {:?}",
                                &src,
                                e
                            );
                            continue;
                        }
                    },
                };

                log::trace!(
                    "Found local name: {} to send answer, IP {}, interface addr {}",
                    local_name,
                    src.ip(),
                    interface_addr
                );
                if let Err(e) =
                    send_answer(socket, &interface_addr, &q.name.data, src.ip(), dst_addr).await
                {
                    log::error!("Error sending answer to client: {:?}", e);
                    continue;
                };
            }
        }
    }

    // There might be more than MAX_MESSAGE_RECORDS questions, so skip the rest
    let _ = p.skip_all_questions();

    for _ in 0..=MAX_MESSAGE_RECORDS {
        let a = match p.answer_header() {
            Ok(a) => a,
            Err(err) => {
                if Error::ErrSectionDone != err {
                    log::warn!("Failed to parse mDNS packet {}", err);
                }
                return;
            }
        };

        if a.typ != DnsType::A && a.typ != DnsType::Aaaa {
            continue;
        }

        let mut qs = queries.lock().await;
        for j in (0..qs.len()).rev() {
            if qs[j].name_with_suffix == a.name.data {
                let _ = qs[j]
                    .query_result_chan
                    .send(QueryResult {
                        answer: a.clone(),
                        addr: src,
                    })
                    .await;
                qs.remove(j);
            }
        }
    }
}

async fn send_answer(
    socket: &Arc<UdpSocket>,
    interface_addr: &SocketAddr,
    name: &str,
    dst: IpAddr,
    dst_addr: SocketAddr,
) -> Result<()> {
    let raw_answer = {
        let mut msg = Message {
            header: Header {
                response: true,
                authoritative: true,
                ..Default::default()
            },

            answers: vec![Resource {
                header: ResourceHeader {
                    typ: DnsType::A,
                    class: DNSCLASS_INET,
                    name: Name::new(name)?,
                    ttl: RESPONSE_TTL,
                    ..Default::default()
                },
                body: Some(Box::new(AResource {
                    a: match interface_addr.ip() {
                        IpAddr::V4(ip) => ip.octets(),
                        IpAddr::V6(_) => {
                            return Err(Error::Other("Unexpected IpV6 addr".to_owned()))
                        }
                    },
                })),
            }],
            ..Default::default()
        };

        msg.pack()?
    };

    socket.send_to(&raw_answer, dst_addr).await?;
    log::trace!("Sent answer to IP {}", dst);

    Ok(())
}

async fn get_interface_addr_for_ip(addr: impl ToSocketAddrs) -> std::io::Result<SocketAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(addr).await?;
    socket.local_addr()
}
