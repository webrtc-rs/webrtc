use crate::config::*;
use crate::errors::*;
use crate::message::name::*;
use crate::message::{header::*, parser::*, question::*, resource::a::*, resource::*, *};

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use util::Error;

use log::*;

const INBOUND_BUFFER_SIZE: usize = 512;
const DEFAULT_QUERY_INTERVAL: Duration = Duration::from_secs(1);
const DESTINATION_ADDRESS: &str = "224.0.0.251:5353";
const MAX_MESSAGE_RECORDS: usize = 3;
const RESPONSE_TTL: u32 = 120;

// Conn represents a mDNS Server
pub struct DNSConn {
    //mu  sync.RWMutex
    //log logging.LeveledLogger
    socket: UdpSocket,    //*ipv4.PacketConn
    dst_addr: SocketAddr, //*net.UDPAddr

    query_interval: Duration,
    local_names: Vec<String>,
    queries: Arc<Mutex<Vec<Query>>>,

    closed: Option<mpsc::Sender<()>>,
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
        conn.join_multicast_v4(Ipv4Addr::new(224, 0, 0, 251), Ipv4Addr::new(0, 0, 0, 0))?;

        let dst_addr: SocketAddr = DESTINATION_ADDRESS.parse()?;
        let local_names = config
            .local_names
            .iter()
            .map(|l| l.to_string() + ".")
            .collect();

        let (closed_tx, _closed_rx) = mpsc::channel(1);

        let c = DNSConn {
            query_interval: if config.query_interval != Duration::from_secs(0) {
                config.query_interval
            } else {
                DEFAULT_QUERY_INTERVAL
            },
            queries: Arc::new(Mutex::new(vec![])),
            socket: conn,
            dst_addr,
            local_names,
            closed: Some(closed_tx),
        };

        //go c.start()
        Ok(c)
    }

    // Close closes the mDNS Conn
    pub fn close(&mut self) -> Result<(), Error> {
        if self.closed.is_none() {
            return Err(ERR_CONNECTION_CLOSED.to_owned());
        }

        self.closed.take();

        Ok(())
    }

    // Query sends mDNS Queries for the following name until
    // either the Context is canceled/expires or we get a result
    pub async fn query(&self, name: &str) -> Result<(ResourceHeader, SocketAddr), Error> {
        if self.closed.is_none() {
            return Err(ERR_CONNECTION_CLOSED.to_owned());
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

        let ticker = tokio::time::sleep(self.query_interval);
        tokio::pin!(ticker);

        loop {
            tokio::select! {
                _ = ticker.as_mut() => self.send_question(&name_with_suffix).await,
                //TODO: _ = self.closed.recv() => return Err(ERR_CONNECTION_CLOSED.to_owned()),
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
                warn!("Failed to construct mDNS packet {}", err);
                return;
            }
        };

        if let Err(err) = self.socket.send_to(&raw_query, self.dst_addr).await {
            warn!("Failed to send mDNS packet {}", err);
        }
    }

    async fn start(
        mut closed_rx: mpsc::Receiver<()>,
        socket: Arc<UdpSocket>,
        local_names: Vec<String>,
        dst_addr: SocketAddr,
        queries: Arc<Mutex<Vec<Query>>>,
    ) -> Result<(), Error> {
        let mut b = vec![0u8; INBOUND_BUFFER_SIZE];

        let (mut n, mut _src);

        loop {
            tokio::select! {
                result = socket.recv_from(&mut b) => {
                    match result{
                        Ok((len, addr)) => {
                            n = len;
                            _src = addr;
                        },
                        Err(err) => return Err(Error::new(err.to_string())),
                    }
                }
                _ = closed_rx.recv() => return Ok(()),
            }

            let mut p = Parser {
                msg: &b[..n],
                ..Default::default()
            };

            for _ in 0..=MAX_MESSAGE_RECORDS {
                let q = match p.question() {
                    Ok(q) => q,
                    Err(err) => {
                        if err == *ERR_SECTION_DONE {
                            break;
                        } else {
                            return Err(Error::new(err.to_string()));
                        }
                    }
                };

                for local_name in &local_names {
                    if local_name == &q.name.data {
                        /*let localAddress = interfaceForRemote(src.String())
                        if err != nil {
                            c.log.Warnf("Failed to get local interface to communicate with %s: %v", src.String(), err)
                            continue
                        }*/
                        let local_addr = socket.local_addr()?;

                        send_answer(&socket, &q.name.data, local_addr.ip(), dst_addr).await?;
                    }
                }
            }

            for _ in 0..=MAX_MESSAGE_RECORDS {
                let a = match p.answer_header() {
                    Ok(a) => a,
                    Err(_) => break,
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
                                addr: socket.local_addr()?,
                            })
                            .await;
                        qs.remove(j);
                    }
                }
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
    let packed_name = Name::new(name)?;

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
                name: packed_name,
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

    let raw_answer = msg.pack()?;
    socket.send_to(&raw_answer, dst_addr).await?;

    Ok(())
}
