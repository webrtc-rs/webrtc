mod flight0;
mod flight1;
mod flight2;
mod flight3;
mod flight4;
mod flight5;
mod flight6;

//use std::fmt;

use util::Error;

use crate::alert::*;
use crate::conn::*;
use crate::handshake::handshake_cache::*;
use crate::handshaker::*;
use crate::record_layer::*;
use crate::state::*;

use async_trait::async_trait;

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

pub(crate) struct Packet {
    record: RecordLayer,
    should_encrypt: bool,
    reset_local_sequence_number: bool,
}

#[async_trait]
pub(crate) trait Flight {
    fn is_last_send_flight(&self) -> bool {
        false
    }

    fn is_last_recv_flight(&self) -> bool {
        false
    }

    async fn parse(
        &self,
        c: &Conn,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight>, (Option<Alert>, Option<Error>)>;

    async fn generate(
        &self,
        c: &Conn,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)>;
}
