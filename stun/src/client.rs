#[cfg(test)]
mod client_test;

use std::collections::HashMap;
use std::io::BufReader;
use std::marker::{Send, Sync};
use std::ops::Add;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::time::{self, Duration, Instant};
use util::Conn;

use crate::agent::*;
use crate::error::*;
use crate::message::*;

const DEFAULT_TIMEOUT_RATE: Duration = Duration::from_millis(5);
const DEFAULT_RTO: Duration = Duration::from_millis(300);
const DEFAULT_MAX_ATTEMPTS: u32 = 7;
const DEFAULT_MAX_BUFFER_SIZE: usize = 8;

/// Collector calls function f with constant rate.
///
/// The simple Collector is ticker which calls function on each tick.
pub trait Collector {
    fn start(
        &mut self,
        rate: Duration,
        client_agent_tx: Arc<mpsc::Sender<ClientAgent>>,
    ) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}

#[derive(Default)]
struct TickerCollector {
    close_tx: Option<mpsc::Sender<()>>,
}

impl Collector for TickerCollector {
    fn start(
        &mut self,
        rate: Duration,
        client_agent_tx: Arc<mpsc::Sender<ClientAgent>>,
    ) -> Result<()> {
        let (close_tx, mut close_rx) = mpsc::channel(1);
        self.close_tx = Some(close_tx);

        tokio::spawn(async move {
            let mut interval = time::interval(rate);

            loop {
                tokio::select! {
                    _ = close_rx.recv() => break,
                    _ = interval.tick() => {
                        if client_agent_tx.send(ClientAgent::Collect(Instant::now())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        if self.close_tx.is_none() {
            return Err(Error::ErrCollectorClosed);
        }
        self.close_tx.take();
        Ok(())
    }
}

/// ClientTransaction represents transaction in progress.
/// If transaction is succeed or failed, f will be called
/// provided by event.
/// Concurrent access is invalid.
#[derive(Debug, Clone)]
pub struct ClientTransaction {
    id: TransactionId,
    attempt: u32,
    calls: u32,
    handler: Handler,
    start: Instant,
    rto: Duration,
    raw: Vec<u8>,
}

impl ClientTransaction {
    pub(crate) fn handle(&mut self, e: Event) -> Result<()> {
        self.calls += 1;
        if self.calls == 1 {
            if let Some(handler) = &self.handler {
                handler.send(e)?;
            }
        }
        Ok(())
    }

    pub(crate) fn next_timeout(&self, now: Instant) -> Instant {
        now.add((self.attempt + 1) * self.rto)
    }
}

struct ClientSettings {
    buffer_size: usize,
    rto: Duration,
    rto_rate: Duration,
    max_attempts: u32,
    closed: bool,
    //handler: Handler,
    collector: Option<Box<dyn Collector + Send>>,
    c: Option<Arc<dyn Conn + Send + Sync>>,
}

impl Default for ClientSettings {
    fn default() -> Self {
        ClientSettings {
            buffer_size: DEFAULT_MAX_BUFFER_SIZE,
            rto: DEFAULT_RTO,
            rto_rate: DEFAULT_TIMEOUT_RATE,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            closed: false,
            //handler: None,
            collector: None,
            c: None,
        }
    }
}

#[derive(Default)]
pub struct ClientBuilder {
    settings: ClientSettings,
}

impl ClientBuilder {
    // WithHandler sets client handler which is called if Agent emits the Event
    // with TransactionID that is not currently registered by Client.
    // Useful for handling Data indications from TURN server.
    //pub fn with_handler(mut self, handler: Handler) -> Self {
    //    self.settings.handler = handler;
    //    self
    //}

    /// with_rto sets client RTO as defined in STUN RFC.
    pub fn with_rto(mut self, rto: Duration) -> Self {
        self.settings.rto = rto;
        self
    }

    /// with_timeout_rate sets RTO timer minimum resolution.
    pub fn with_timeout_rate(mut self, d: Duration) -> Self {
        self.settings.rto_rate = d;
        self
    }

    /// with_buffer_size sets buffer size.
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        self.settings.buffer_size = buffer_size;
        self
    }

    /// with_collector rests client timeout collector, the implementation
    /// of ticker which calls function on each tick.
    pub fn with_collector(mut self, coll: Box<dyn Collector + Send>) -> Self {
        self.settings.collector = Some(coll);
        self
    }

    /// with_conn sets transport connection
    pub fn with_conn(mut self, conn: Arc<dyn Conn + Send + Sync>) -> Self {
        self.settings.c = Some(conn);
        self
    }

    /// with_no_retransmit disables retransmissions and sets RTO to
    /// DEFAULT_MAX_ATTEMPTS * DEFAULT_RTO which will be effectively time out
    /// if not set.
    /// Useful for TCP connections where transport handles RTO.
    pub fn with_no_retransmit(mut self) -> Self {
        self.settings.max_attempts = 0;
        if self.settings.rto == Duration::from_secs(0) {
            self.settings.rto = DEFAULT_MAX_ATTEMPTS * DEFAULT_RTO;
        }
        self
    }

    pub fn new() -> Self {
        ClientBuilder {
            settings: ClientSettings::default(),
        }
    }

    pub fn build(self) -> Result<Client> {
        if self.settings.c.is_none() {
            return Err(Error::ErrNoConnection);
        }

        let client = Client {
            settings: self.settings,
            ..Default::default()
        }
        .run()?;

        Ok(client)
    }
}

/// Client simulates "connection" to STUN server.
#[derive(Default)]
pub struct Client {
    settings: ClientSettings,
    close_tx: Option<mpsc::Sender<()>>,
    client_agent_tx: Option<Arc<mpsc::Sender<ClientAgent>>>,
    handler_tx: Option<Arc<mpsc::UnboundedSender<Event>>>,
}

impl Client {
    async fn read_until_closed(
        mut close_rx: mpsc::Receiver<()>,
        c: Arc<dyn Conn + Send + Sync>,
        client_agent_tx: Arc<mpsc::Sender<ClientAgent>>,
    ) {
        let mut msg = Message::new();
        let mut buf = vec![0; 1024];

        loop {
            tokio::select! {
                _ = close_rx.recv() => return,
                res = c.recv(&mut buf) => {
                    if let Ok(n) = res {
                        let mut reader = BufReader::new(&buf[..n]);
                        let result = msg.read_from(&mut reader);
                        if result.is_err() {
                            continue;
                        }

                        if client_agent_tx.send(ClientAgent::Process(msg.clone())).await.is_err(){
                            return;
                        }
                    }
                }
            }
        }
    }

    fn insert(&mut self, ct: ClientTransaction) -> Result<()> {
        if self.settings.closed {
            return Err(Error::ErrClientClosed);
        }

        if let Some(handler_tx) = &mut self.handler_tx {
            handler_tx.send(Event {
                event_type: EventType::Insert(ct),
                ..Default::default()
            })?;
        }

        Ok(())
    }

    fn remove(&mut self, id: TransactionId) -> Result<()> {
        if self.settings.closed {
            return Err(Error::ErrClientClosed);
        }

        if let Some(handler_tx) = &mut self.handler_tx {
            handler_tx.send(Event {
                event_type: EventType::Remove(id),
                ..Default::default()
            })?;
        }

        Ok(())
    }

    fn start(
        conn: Option<Arc<dyn Conn + Send + Sync>>,
        mut handler_rx: mpsc::UnboundedReceiver<Event>,
        client_agent_tx: Arc<mpsc::Sender<ClientAgent>>,
        mut t: HashMap<TransactionId, ClientTransaction>,
        max_attempts: u32,
    ) {
        tokio::spawn(async move {
            while let Some(event) = handler_rx.recv().await {
                match event.event_type {
                    EventType::Close => {
                        break;
                    }
                    EventType::Insert(ct) => {
                        if t.contains_key(&ct.id) {
                            continue;
                        }
                        t.insert(ct.id, ct);
                    }
                    EventType::Remove(id) => {
                        t.remove(&id);
                    }
                    EventType::Callback(id) => {
                        let mut ct = if t.contains_key(&id) {
                            t.remove(&id).unwrap()
                        } else {
                            /*if c.handler != nil && !errors.Is(e.Error, ErrTransactionStopped) {
                                c.handler(e)
                            }*/
                            continue;
                        };

                        if ct.attempt >= max_attempts || event.event_body.is_ok() {
                            if let Some(handler) = ct.handler {
                                let _ = handler.send(event);
                            }
                            continue;
                        }

                        // Doing re-transmission.
                        ct.attempt += 1;

                        let raw = ct.raw.clone();
                        let timeout = ct.next_timeout(Instant::now());
                        let id = ct.id;

                        // Starting client transaction.
                        t.insert(ct.id, ct);

                        // Starting agent transaction.
                        if client_agent_tx
                            .send(ClientAgent::Start(id, timeout))
                            .await
                            .is_err()
                        {
                            let ct = t.remove(&id).unwrap();
                            if let Some(handler) = ct.handler {
                                let _ = handler.send(event);
                            }
                            continue;
                        }

                        // Writing message to connection again.
                        if let Some(c) = &conn {
                            if c.send(&raw).await.is_err() {
                                let _ = client_agent_tx.send(ClientAgent::Stop(id)).await;

                                let ct = t.remove(&id).unwrap();
                                if let Some(handler) = ct.handler {
                                    let _ = handler.send(event);
                                }
                                continue;
                            }
                        }
                    }
                };
            }
        });
    }

    /// close stops internal connection and agent, returning CloseErr on error.
    pub async fn close(&mut self) -> Result<()> {
        if self.settings.closed {
            return Err(Error::ErrClientClosed);
        }

        self.settings.closed = true;

        if let Some(collector) = &mut self.settings.collector {
            let _ = collector.close();
        }
        self.settings.collector.take();

        self.close_tx.take(); //drop close channel
        if let Some(client_agent_tx) = &mut self.client_agent_tx {
            let _ = client_agent_tx.send(ClientAgent::Close).await;
        }
        self.client_agent_tx.take();

        if let Some(c) = self.settings.c.take() {
            c.close().await?;
        }

        Ok(())
    }

    fn run(mut self) -> Result<Self> {
        let (close_tx, close_rx) = mpsc::channel(1);
        let (client_agent_tx, client_agent_rx) = mpsc::channel(self.settings.buffer_size);
        let (handler_tx, handler_rx) = mpsc::unbounded_channel();
        let t: HashMap<TransactionId, ClientTransaction> = HashMap::new();

        let client_agent_tx = Arc::new(client_agent_tx);
        let handler_tx = Arc::new(handler_tx);
        self.client_agent_tx = Some(Arc::clone(&client_agent_tx));
        self.handler_tx = Some(Arc::clone(&handler_tx));
        self.close_tx = Some(close_tx);

        let conn = if let Some(conn) = &self.settings.c {
            Arc::clone(conn)
        } else {
            return Err(Error::ErrNoConnection);
        };

        Client::start(
            self.settings.c.clone(),
            handler_rx,
            Arc::clone(&client_agent_tx),
            t,
            self.settings.max_attempts,
        );

        let agent = Agent::new(Some(handler_tx));
        tokio::spawn(async move { Agent::run(agent, client_agent_rx).await });

        if self.settings.collector.is_none() {
            self.settings.collector = Some(Box::<TickerCollector>::default());
        }
        if let Some(collector) = &mut self.settings.collector {
            collector.start(self.settings.rto_rate, Arc::clone(&client_agent_tx))?;
        }

        let conn_rx = Arc::clone(&conn);
        tokio::spawn(
            async move { Client::read_until_closed(close_rx, conn_rx, client_agent_tx).await },
        );

        Ok(self)
    }

    pub async fn send(&mut self, m: &Message, handler: Handler) -> Result<()> {
        if self.settings.closed {
            return Err(Error::ErrClientClosed);
        }

        let has_handler = handler.is_some();

        if handler.is_some() {
            let t = ClientTransaction {
                id: m.transaction_id,
                attempt: 0,
                calls: 0,
                handler,
                start: Instant::now(),
                rto: self.settings.rto,
                raw: m.raw.clone(),
            };
            let d = t.next_timeout(t.start);
            self.insert(t)?;

            if let Some(client_agent_tx) = &mut self.client_agent_tx {
                client_agent_tx
                    .send(ClientAgent::Start(m.transaction_id, d))
                    .await?;
            }
        }

        if let Some(c) = &self.settings.c {
            let result = c.send(&m.raw).await;
            if result.is_err() && has_handler {
                self.remove(m.transaction_id)?;

                if let Some(client_agent_tx) = &mut self.client_agent_tx {
                    client_agent_tx
                        .send(ClientAgent::Stop(m.transaction_id))
                        .await?;
                }
            } else if let Err(err) = result {
                return Err(Error::Other(err.to_string()));
            }
        }

        Ok(())
    }
}
