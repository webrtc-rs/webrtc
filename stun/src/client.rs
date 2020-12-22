use crate::agent::*;
use crate::errors::*;
use crate::message::*;

use util::Error;

use tokio::sync::mpsc;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::BufReader;
use std::ops::Add;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const DEFAULT_TIMEOUT_RATE: Duration = Duration::from_millis(5);
const DEFAULT_RTO: Duration = Duration::from_millis(300);
const DEFAULT_MAX_ATTEMPTS: u32 = 7;

// ClientAgent is Agent implementation that is used by Client to
// process transactions.
pub trait ClientAgent {
    fn process(&mut self, m: &Rc<RefCell<Message>>) -> Result<(), Error>;
    fn close(&mut self) -> Result<(), Error>;
    fn start(&mut self, id: TransactionId, deadline: Instant) -> Result<(), Error>;
    fn stop(&mut self, id: TransactionId) -> Result<(), Error>;
    fn collect(&mut self, gc_time: Instant) -> Result<(), Error>;
    fn set_handler(&mut self, h: Handler) -> Result<(), Error>;
}

// Collector calls function f with constant rate.
//
// The simple Collector is ticker which calls function on each tick.
pub trait Collector {
    fn start(&mut self, rate: Duration, f: fn(now: Instant)) -> Result<(), Error>;
    fn close(&mut self) -> Result<(), Error>;
}

// clientTransaction represents transaction in progress.
// If transaction is succeed or failed, f will be called
// provided by event.
// Concurrent access is invalid.
pub(crate) struct ClientTransaction {
    id: TransactionId,
    attempt: u32,
    calls: AtomicU32,
    h: Handler,
    start: Instant,
    rto: Duration,
    raw: Vec<u8>,
}

impl ClientTransaction {
    pub(crate) fn handle(&self, e: &Event) {
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            (self.h)(e);
        }
    }

    pub(crate) fn next_timeout(&self, now: Instant) -> Instant {
        now.add((self.attempt + 1) * self.rto)
    }
}

#[derive(Default)]
struct ClientSettings {
    rto: Duration,
    a: Option<Box<dyn ClientAgent>>,
    rto_rate: Duration,
    max_attempts: u32,
    closed: bool,
    handler: Option<Handler>,
    collector: Option<Box<dyn Collector>>,
    c: Option<Arc<UdpSocket>>,
}

#[derive(Default)]
pub struct ClientBuilder {
    settings: ClientSettings,
}

impl ClientBuilder {
    // WithHandler sets client handler which is called if Agent emits the Event
    // with TransactionID that is not currently registered by Client.
    // Useful for handling Data indications from TURN server.
    pub fn with_handler(mut self, h: Handler) -> Self {
        self.settings.handler = Some(h);
        self
    }

    // WithRTO sets client RTO as defined in STUN RFC.
    pub fn with_rto(mut self, rto: Duration) -> Self {
        self.settings.rto = rto;
        self
    }

    // WithClock sets Clock of client, the source of current time.
    // Also clock is passed to default collector if set.
    /*pub fn with_clock(mut self, clock: Clock) ->Self {
        self.settings.clock = clock
        self
    }*/

    // WithTimeoutRate sets RTO timer minimum resolution.
    pub fn with_timeout_rate(mut self, d: Duration) -> Self {
        self.settings.rto_rate = d;
        self
    }

    // WithAgent sets client STUN agent.
    //
    // Defaults to agent implementation in current package,
    // see agent.go.
    pub fn with_agent(mut self, a: Box<dyn ClientAgent>) -> Self {
        self.settings.a = Some(a);
        self
    }

    // WithCollector rests client timeout collector, the implementation
    // of ticker which calls function on each tick.
    pub fn with_collector(mut self, coll: Box<dyn Collector>) -> Self {
        self.settings.collector = Some(coll);
        self
    }

    pub fn with_conn(mut self, udp_socket: UdpSocket) -> Self {
        self.settings.c = Some(Arc::new(udp_socket));
        self
    }

    // with_no_retransmit disables retransmissions and sets RTO to
    // DEFAULT_MAX_ATTEMPTS * DEFAULT_RTO which will be effectively time out
    // if not set.
    // Useful for TCP connections where transport handles RTO.
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

    pub fn build(self) -> Result<Client, Error> {
        if self.settings.c.is_none() {
            return Err(ERR_NO_CONNECTION.clone());
        }

        Ok(Client {
            settings: self.settings,
            close_tx: None,
            t: HashMap::new(),
        })
    }
}

// Client simulates "connection" to STUN server.
#[derive(Default)]
pub struct Client {
    settings: ClientSettings,
    close_tx: Option<mpsc::Sender<()>>,
    t: HashMap<TransactionId, ClientTransaction>,
    // mux guards closed and t
    //	mux sync.RWMutex
}

impl Client {
    // NewClient initializes new Client from provided options,
    // starting internal goroutines and using default options fields
    // if necessary. Call Close method after using Client to close conn and
    // release resources.
    //
    // The conn will be closed on Close call. Use with_no_conn_close option to
    // prevent that.
    //
    // Note that user should handle the protocol multiplexing, client does not
    // provide any API for it, so if you need to read application data, wrap the
    // connection with your (de-)multiplexer and pass the wrapper as conn.
    /*pub fn new(conn Connection, options: &[ClientOption]) ->Result<Client, Error> {
        c := &Client{
            close:       make(chan struct{}),
            c:           conn,
            clock:       systemClock(),
            rto:         int64(DEFAULT_RTO),
            rto_rate:     DEFAULT_TIMEOUT_RATE,
            t:           make(map[transactionID]*clientTransaction, 100),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            close_conn:   true,
        }
        for _, o := range options {
            o(c)
        }
        if c.c == nil {
            return nil, ErrNoConnection
        }
        if c.a == nil {
            c.a = NewAgent(nil)
        }
        if err := c.a.SetHandler(c.handleAgentCallback); err != nil {
            return nil, err
        }
        if c.collector == nil {
            c.collector = &tickerCollector{
                close: make(chan struct{}),
                clock: c.clock,
            }
        }
        if err := c.collector.Start(c.rto_rate, func(t time.Time) {
            closedOrPanic(c.a.Collect(t))
        }); err != nil {
            return nil, err
        }
        c.wg.Add(1)
        go c.read_until_closed()
        runtime.SetFinalizer(c, clientFinalizer)
        return c, nil
    }*/

    async fn read_until_closed(
        mut close_rx: mpsc::Receiver<()>,
        c: Arc<UdpSocket>,
        mut a: Box<dyn ClientAgent>,
    ) {
        let msg = Rc::new(RefCell::new(Message::new()));
        let mut buf = vec![0; 1024];

        loop {
            tokio::select! {
             _ = close_rx.recv() => return,
             res = c.recv(&mut buf) => {
                if let Ok(n) = res {
                    {
                        let mut reader = BufReader::new(&buf[..n]);
                        let mut m = msg.borrow_mut();
                        let result = m.read_from(&mut reader);
                        if result.is_err() {
                            continue;
                        }
                    }
                    if let Err(err) = a.process(&Rc::clone(&msg)) {
                        if err == *ERR_AGENT_CLOSED {
                            return;
                        }
                    }
                }
             }
            }
        }
    }

    // start registers transaction.
    // Could return ErrClientClosed, ErrTransactionExists.
    fn register(&mut self, t: ClientTransaction) -> Result<(), Error> {
        //c.mux.Lock()
        //defer c.mux.Unlock()
        if self.settings.closed {
            return Err(ERR_CLIENT_CLOSED.clone());
        }

        if self.t.contains_key(&t.id) {
            return Err(ERR_TRANSACTION_EXISTS.clone());
        }
        self.t.insert(t.id, t);
        Ok(())
    }

    fn delete(&mut self, id: &TransactionId) {
        //c.mux.Lock()
        self.t.remove(id);
        //c.mux.Unlock()
    }

    // set_rto sets current RTO value.
    pub fn set_rto(&mut self, rto: Duration) {
        self.settings.rto = rto;
    }

    // Close stops internal connection and agent, returning CloseErr on error.
    pub async fn close(&mut self) -> Result<(), Error> {
        if self.settings.closed {
            return Err(ERR_CLIENT_CLOSED.clone());
        }

        self.settings.closed = true;
        if let Some(collector) = &mut self.settings.collector {
            collector.close()?;
        }
        self.close_tx.take(); //drop close channel
        if let Some(a) = &mut self.settings.a {
            a.close()?;
        }

        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
