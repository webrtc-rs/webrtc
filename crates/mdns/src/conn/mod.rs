use crate::config::*;
use crate::errors::*;
use crate::message::name::*;
use crate::message::{header::*, parser::*, question::*, resource::a::*, resource::*, *};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use core::sync::atomic;
use socket2::SockAddr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use util::Error;
mod conn_test;

const INBOUND_BUFFER_SIZE: usize = 512;
const DEFAULT_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_DEST_ADDR: &str = "224.0.0.251:5353";
const MAX_MESSAGE_RECORDS: usize = 3;
const RESPONSE_TTL: u32 = 120;

// Conn represents a mDNS Server
pub struct DNSConn {
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
    addr: SocketAddr, //net.Addr
}

impl DNSConn {
    // server establishes a mDNS connection over an existing conn
    pub fn server(addr: SocketAddr, config: Config) -> Result<Self, Error> {
        let socket = socket2::Socket::new(
            socket2::Domain::ipv4(),
            socket2::Type::dgram(),
            Some(socket2::Protocol::udp()),
        )?;

        socket.set_reuse_address(true)?;
        socket.set_reuse_port(true)?;
        socket.bind(&SockAddr::from(addr))?;

        let socket = UdpSocket::from_std(socket.into_udp_socket())?;

        socket.set_multicast_loop_v4(true)?;
        socket.set_multicast_ttl_v4(255)?;
        socket.join_multicast_v4(Ipv4Addr::new(224, 0, 0, 251), Ipv4Addr::new(0, 0, 0, 0))?;

        let local_names = config
            .local_names
            .iter()
            .map(|l| l.to_string() + ".")
            .collect();

        let dst_addr: SocketAddr = format!("{}", DEFAULT_DEST_ADDR).parse()?;

        let is_server_closed = Arc::new(atomic::AtomicBool::new(false));

        let (close_server_send, close_server_rcv) = mpsc::channel(1);

        let c = DNSConn {
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
            if let Err(e) = DNSConn::start(
                close_server_rcv,
                is_server_closed,
                socket,
                local_names,
                dst_addr,
                queries,
            )
            .await
            {
                panic!("Error starting dns connection, error: {:?}", e);
            };
        });

        Ok(c)
    }

    // Close closes the mDNS Conn
    pub async fn close(&self) -> Result<(), Error> {
        {
            if self.is_server_closed.load(atomic::Ordering::SeqCst) {
                return Err(ERR_CONNECTION_CLOSED.to_owned());
            }

            match self.close_server.send(()).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    log::warn!("error sending close command to server: {:?}", e);
                    Err(ERR_CONNECTION_CLOSED.to_owned())
                }
            }
        }
    }

    // Query sends mDNS Queries for the following name until
    // either the Context is canceled/expires or we get a result
    pub async fn query(
        &mut self,
        name: &str,
        mut close_query_signal: mpsc::Receiver<()>,
    ) -> Result<(ResourceHeader, SocketAddr), Error> {
        {
            if self.is_server_closed.load(atomic::Ordering::SeqCst) {
                return Err(ERR_CONNECTION_CLOSED.to_owned());
            }
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

        self.send_question(&name_with_suffix).await;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.query_interval) => self.send_question(&name_with_suffix).await,

                _ = close_query_signal.recv() => return Err(ERR_CONNECTION_CLOSED.to_owned()),

                res_opt = query_rx.recv() =>{
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

        let mut msg = Message {
            header: Header::default(),
            questions: vec![Question {
                typ: DNSType::A,
                class: DNSCLASS_INET,
                name: packed_name,
            }],
            ..Default::default()
        };

        let raw_query = match msg.pack() {
            Ok(v) => v,
            Err(err) => {
                println!("Failed to construct mDNS packet {}", err);
                return;
            }
        };

        log::trace!("{:?} sending {:?}...", self.socket.local_addr(), raw_query);
        if let Err(err) = self.socket.send_to(&raw_query, self.dst_addr).await {
            println!("not sent");
            log::error!("Failed to send mDNS packet {}", err);
        }
        println!("sent");
    }

    async fn start(
        mut closed_rx: mpsc::Receiver<()>,
        close_server: Arc<atomic::AtomicBool>,
        socket: Arc<UdpSocket>,
        local_names: Vec<String>,
        dst_addr: SocketAddr,
        queries: Arc<Mutex<Vec<Query>>>,
    ) -> Result<(), Error> {
        let mut b = vec![0u8; 1024];

        let (mut n, mut src);

        loop {
            log::info!("enter loop and listening {:?}", socket.local_addr());

            tokio::select! {
                result = socket.recv_from(&mut b) => {
                    log::info!("Received new connection");

                    match result{
                        Ok((len, addr)) => {
                            n = len;
                            src = addr;
                        },

                        Err(err) => return Err(Error::new(err.to_string())),
                    }
                }

                _ = closed_rx.recv() => {
                    println!("Closing connection");
                    close_server.store(true, atomic::Ordering::SeqCst);

                    return Ok(());
                }
            }

            log::trace!("recv bytes {:?} from {}", &b[..n], src);

            let mut p = Parser::default();
            if let Err(err) = p.start(&b[..n]) {
                log::error!("Failed to parse mDNS packet {}", err);
                continue;
            }

            run(&mut p, &socket, &local_names, src, dst_addr, &queries).await;
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
    for _ in 0..=MAX_MESSAGE_RECORDS {
        let q = match p.question() {
            Ok(q) => q,
            Err(err) => {
                if err == *ERR_SECTION_DONE {
                    break;
                } else {
                    log::error!("Failed to parse mDNS packet {}", err);
                    return;
                }
            }
        };

        for local_name in local_names {
            if local_name == &q.name.data {
                let _ = send_answer(socket, &q.name.data, src.ip(), dst_addr).await;
            }
        }
    }

    for _ in 0..=MAX_MESSAGE_RECORDS {
        let a = match p.answer_header() {
            Ok(a) => a,
            Err(err) => {
                if err == *ERR_SECTION_DONE {
                    return;
                } else {
                    log::warn!("Failed to parse mDNS packet {}", err);
                    return;
                }
            }
        };

        if a.typ != DNSType::A && a.typ != DNSType::AAAA {
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

async fn interface_for_remote(remote: String) -> Result<std::net::IpAddr, Error> {
    let conn = UdpSocket::bind(remote).await?;
    let local_addr = conn.local_addr()?;

    Ok(local_addr.ip())
}

async fn send_answer(
    socket: &Arc<UdpSocket>,
    name: &str,
    dst: IpAddr,
    dst_addr: SocketAddr,
) -> Result<(), Error> {
    let raw_answer = {
        let mut msg = Message {
            header: Header {
                response: true,
                authoritative: true,
                ..Default::default()
            },

            answers: vec![Resource {
                header: ResourceHeader {
                    typ: DNSType::A,
                    class: DNSCLASS_INET,
                    name: Name::new(name)?,
                    ttl: RESPONSE_TTL,
                    ..Default::default()
                },
                body: Some(Box::new(AResource {
                    a: match dst {
                        IpAddr::V4(ip) => ip.octets(),
                        IpAddr::V6(_) => return Err(Error::new("unexpected IpV6 addr".to_owned())),
                    },
                })),
            }],
            ..Default::default()
        };

        msg.pack()?
    };

    log::trace!("send_answer {} to {}", dst, dst_addr);
    socket.send_to(&raw_answer, dst_addr).await?;

    Ok(())
}
