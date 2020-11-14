mod flight0;

use std::fmt;

use util::Error;

use crate::alert::*;
use crate::handshake::handshake_cache::*;
use crate::handshaker::*;
use crate::record_layer::*;
use crate::state::*;

use tokio::sync::mpsc;

/*
  DTLS messages are grouped into a series of message flights, according
  to the diagrams below.  Although each Flight of messages may consist
  of a number of messages, they should be viewed as monolithic for the
  purpose of timeout and retransmission.
  https://tools.ietf.org/html/rfc4347#section-4.2.4
  Client                                          Server
  ------                                          ------
                                      Waiting                 Flight 0

  ClientHello             -------->                           Flight 1

                          <-------    HelloVerifyRequest      Flight 2

  ClientHello              -------->                           Flight 3

                                             ServerHello    \
                                            Certificate*     \
                                      ServerKeyExchange*      Flight 4
                                     CertificateRequest*     /
                          <--------      ServerHelloDone    /

  Certificate*                                              \
  ClientKeyExchange                                          \
  CertificateVerify*                                          Flight 5
  [ChangeCipherSpec]                                         /
  Finished                -------->                         /

                                      [ChangeCipherSpec]    \ Flight 6
                          <--------             Finished    /

*/
#[derive(PartialEq)]
pub(crate) enum Flight {
    Flight0,
    Flight1,
    Flight2,
    Flight3,
    Flight4,
    Flight5,
    Flight6,
}

impl fmt::Display for Flight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Flight::Flight0 => write!(f, "Flight 0"),
            Flight::Flight1 => write!(f, "Flight 1"),
            Flight::Flight2 => write!(f, "Flight 2"),
            Flight::Flight3 => write!(f, "Flight 3"),
            Flight::Flight4 => write!(f, "Flight 4"),
            Flight::Flight5 => write!(f, "Flight 5"),
            Flight::Flight6 => write!(f, "Flight 6"),
        }
    }
}

impl Flight {
    pub(crate) fn is_last_send_flight(&self) -> bool {
        *self == Flight::Flight6
    }

    pub(crate) fn is_last_recv_flight(&self) -> bool {
        *self == Flight::Flight5
    }
}

pub(crate) struct Packet {
    record: RecordLayer,
    should_encrypt: bool,
    reset_local_sequence_number: bool,
}

pub(crate) trait FlightConn {
    fn notify(
        &self,
        /*ctx context.Context,*/ level: AlertLevel,
        desc: AlertDescription,
    ) -> Result<(), Error>;
    fn write_packets(&self, /*context.Context,*/ packets: Vec<Packet>) -> Result<(), Error>;
    fn recv_handshake(&self) -> mpsc::Receiver<()>;
    fn set_local_epoch(&self, epoch: u16);
    fn handle_queued_packets(&self /*context.Context*/) -> Result<(), Error>;
}

// Parse received handshakes and return next flightVal
/*type FlightParser = fn<FC: FlightConn>(
    /*context.Context,*/ fc: FC,
    state: &mut State,
    handshake_cache: &HandshakeCache,
    handshake_config: &HandshakeConfig,
) -> Result<(Flight, Option<Alert>), Error>;

// Generate flights
type FlightGenerator = fn<FC: FlightConn>(
    fc: FC,
    state: &State,
    handshake_cache: &HandshakeCache,
    handshake_config: &HandshakeConfig,
) -> Result<(Vec<Packet>, Option<Alert>), Error>;
*/
