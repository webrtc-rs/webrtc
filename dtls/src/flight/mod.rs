pub(crate) mod flight0;
pub(crate) mod flight1;
pub(crate) mod flight2;
pub(crate) mod flight3;
pub(crate) mod flight4;
pub(crate) mod flight5;
pub(crate) mod flight6;

use std::fmt;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::alert::*;
use crate::error::Error;
use crate::handshake::handshake_cache::*;
use crate::handshaker::*;
use crate::record_layer::*;
use crate::state::*;

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

#[derive(Clone, Debug)]
pub(crate) struct Packet {
    pub(crate) record: RecordLayer,
    pub(crate) should_encrypt: bool,
    pub(crate) reset_local_sequence_number: bool,
}

#[async_trait]
pub(crate) trait Flight: fmt::Display + fmt::Debug {
    fn is_last_send_flight(&self) -> bool {
        false
    }
    fn is_last_recv_flight(&self) -> bool {
        false
    }
    fn has_retransmit(&self) -> bool {
        true
    }

    async fn parse(
        &self,
        tx: &mut mpsc::Sender<mpsc::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)>;

    async fn generate(
        &self,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)>;
}
