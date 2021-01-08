use crate::config::*;
use crate::errors::*;
use crate::message::name::*;
use crate::message::{header::*, parser::*, question::*, resource::a::*, resource::*, *};

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use util::Error;

use log::*;

const INBOUND_BUFFER_SIZE: usize = 512;
const DEFAULT_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_DEST_ADDR: &str = "224.0.0.251:5353";
const DEFAULT_DEST_PORT: u16 = 5353;
const MAX_MESSAGE_RECORDS: usize = 3;
const RESPONSE_TTL: u32 = 120;

// Conn represents a mDNS Server
pub struct DNSConn {
    //mu  sync.RWMutex
    //log logging.LeveledLogger
    socket: Arc<UdpSocket>, //*ipv4.PacketConn
    dst_addr: SocketAddr,   //*net.UDPAddr

    query_interval: Duration,
    //local_names: Vec<String>,
    queries: Arc<Mutex<Vec<Query>>>,

    start_closed_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    query_closed_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
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
    pub fn server(conn: UdpSocket, config: Config) -> Result<Self, Error> {
        conn.set_multicast_loop_v4(true)?;
        conn.set_multicast_ttl_v4(255)?;

        let dst_addr: SocketAddr = format!("{}", DEFAULT_DEST_ADDR).parse()?;

        {
            let mut _join_error_count = 0;

            let interfaces = ifaces::Interface::get_all().unwrap();
            _join_error_count = 0;

            for interface in interfaces {
                for ip in interface.addr {
                    if let IpAddr::V4(e) = ip.ip() {
                        if let Err(e) = conn.join_multicast_v4(Ipv4Addr::new(224, 0, 0, 251), e) {
                            _join_error_count += 1;
                            println!("Error connecting multicast, error: {:?}", e);
                        }
                        continue;
                    }

                    _join_error_count += 1;
                }
            }
        }

        let local_names = config
            .local_names
            .iter()
            .map(|l| l.to_string() + ".")
            .collect();

        let (start_closed_tx, start_closed_rx) = mpsc::channel(1);

        let c = DNSConn {
            query_interval: if config.query_interval != Duration::from_secs(0) {
                config.query_interval
            } else {
                DEFAULT_QUERY_INTERVAL
            },
            queries: Arc::new(Mutex::new(vec![])),
            socket: Arc::new(conn),
            dst_addr,
            start_closed_tx: Arc::new(Mutex::new(Some(start_closed_tx))),
            query_closed_tx: Arc::new(Mutex::new(None)),
        };

        let queries = c.queries.clone();
        let socket = Arc::clone(&c.socket);

        tokio::spawn(async move {
            if let Err(e) =
                DNSConn::start(start_closed_rx, socket, local_names, dst_addr, queries).await
            {
                panic!("Error starting dns connection, error: {:?}", e);
            };
        });

        Ok(c)
    }

    // Close closes the mDNS Conn
    pub async fn close(&self) -> Result<(), Error> {
        {
            let mut start_closed_tx = self.start_closed_tx.lock().await;
            if start_closed_tx.is_none() {
                return Err(ERR_CONNECTION_CLOSED.to_owned());
            }
            start_closed_tx.take();
        }

        {
            let mut query_closed_tx = self.query_closed_tx.lock().await;
            query_closed_tx.take();
        }

        Ok(())
    }

    // Query sends mDNS Queries for the following name until
    // either the Context is canceled/expires or we get a result
    pub async fn query(&self, name: &str) -> Result<(ResourceHeader, SocketAddr), Error> {
        {
            let start_closed_tx = self.start_closed_tx.lock().await;
            if start_closed_tx.is_none() {
                return Err(ERR_CONNECTION_CLOSED.to_owned());
            }
        }

        let (query_tx, mut query_close_rx) = mpsc::channel(1);
        {
            let mut query_closed_tx = self.query_closed_tx.lock().await;
            *query_closed_tx = Some(query_tx);
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

                _ = query_close_rx.recv() => return Err(ERR_CONNECTION_CLOSED.to_owned()),

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
                warn!("Failed to construct mDNS packet {}", err);
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

        println!("{:?} sending {:?}...", self.socket.local_addr(), raw_query);
        if let Err(err) = self.socket.send_to(&raw_query, self.dst_addr).await {
            println!("Failed to send mDNS packet {}", err);
        }
    }

    async fn start(
        mut closed_rx: mpsc::Receiver<()>,
        socket: Arc<UdpSocket>,
        local_names: Vec<String>,
        dst_addr: SocketAddr,
        queries: Arc<Mutex<Vec<Query>>>,
    ) -> Result<(), Error> {
        let mut b = vec![0u8; 1024];

        let (mut n, mut src);

        loop {
            println!("enter loop and listening {:?}", socket.local_addr());

            tokio::select! {
                result = socket.recv_from(&mut b) => {
                    println!("Received new connection");

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
                    return Ok(());
                }
            }

            println!("recv bytes {:?} from {}", &b[..n], src);

            let mut p = Parser::default();
            if let Err(err) = p.start(&b[..n]) {
                println!("Failed to parse mDNS packet {}", err);
                continue;
            }

            run(&mut p, &socket, &local_names, src, dst_addr, &queries).await;
        }
    }
}

async fn interface_for_remote(remote: String) -> Result<std::net::IpAddr, Error> {
    let conn = UdpSocket::bind(remote).await?;
    let local_addr = conn.local_addr()?;

    Ok(local_addr.ip())
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
                    println!("Failed to parse mDNS packet {}", err);
                    return;
                }
            }
        };

        for local_name in local_names {
            if local_name == &q.name.data {
                let local_address = match interface_for_remote(src.ip().to_string()).await {
                    Ok(e) => e,
                    Err(e) => {
                        println!("failed to retrieve remote interface for ip, error: {:?}", e);
                        continue;
                    }
                };

                let _ = send_answer(socket, &q.name.data, local_address, dst_addr).await;
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
                    warn!("Failed to parse mDNS packet {}", err);
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

    trace!("send_answer {} to {}", dst, dst_addr);
    socket.send_to(&raw_answer, dst_addr).await?;

    Ok(())
}
