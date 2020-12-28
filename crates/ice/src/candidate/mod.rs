pub mod candidate_base;
pub mod candidate_pair;
pub mod candidate_related_address;
pub mod candidate_type;

use crate::network_type::*;
use crate::tcp_type::*;
use candidate_related_address::*;
use candidate_type::*;

use util::Error;

use std::fmt;
use std::net::IpAddr;
use tokio::time::Instant;

pub(crate) const RECEIVE_MTU: usize = 8192;
pub(crate) const DEFAULT_LOCAL_PREFERENCE: u16 = 65535;

// COMPONENT_RTP indicates that the candidate is used for RTP
pub(crate) const COMPONENT_RTP: u16 = 1;
// COMPONENT_RTCP indicates that the candidate is used for RTCP
pub(crate) const COMPONENT_RTCP: u16 = 0;

// Candidate represents an ICE candidate
pub trait Candidate: fmt::Display {
    // An arbitrary string used in the freezing algorithm to
    // group similar candidates.  It is the same for two candidates that
    // have the same type, base IP address, protocol (UDP, TCP, etc.),
    // and STUN or TURN server.
    fn foundation(&self) -> String;

    // ID is a unique identifier for just this candidate
    // Unlike the foundation this is different for each candidate
    fn id(&self) -> String;

    // A component is a piece of a data stream.
    // An example is one for RTP, and one for RTCP
    fn component(&self) -> u16;
    fn set_component(&mut self, c: u16);

    // The last time this candidate received traffic
    fn last_received(&self) -> Instant;

    // The last time this candidate sent traffic
    fn last_sent(&self) -> Instant;

    fn network_type(&self) -> NetworkType;
    fn address(&self) -> String;
    fn port(&self) -> u16;

    fn priority(&self) -> u32;

    // A transport address related to a
    //  candidate, which is useful for diagnostics and other purposes
    fn related_address(&self) -> Option<CandidateRelatedAddress>;

    fn candidate_type(&self) -> CandidateType;
    fn tcp_type(&self) -> TCPType;

    fn marshal(&self) -> String;

    fn addr(&self) -> IpAddr;
    //TODO:fn agent(&self) -> Agent;
    //TODO:fn context(&self) ->Context;

    fn close(&self) -> Result<(), Error>;
    fn seen(&mut self, outbound: bool);
    //TODO:fn start(&self,a: &Agent, conn: PacketConn, initializedCh <-chan struct{})
    fn write_to(&mut self, raw: &[u8], dst: &dyn Candidate) -> Result<usize, Error>;
    fn equal(&self, other: &dyn Candidate) -> bool;
}
