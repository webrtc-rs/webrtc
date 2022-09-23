use crate::Transmit;

/// Events sent from an Endpoint to an Association
#[derive(Debug)]
pub struct AssociationEvent(pub(crate) AssociationEventInner);

#[derive(Debug)]
pub(crate) enum AssociationEventInner {
    /// A datagram has been received for the Association
    Datagram(Transmit),
    // New Association identifiers have been issued for the Association
    //NewIdentifiers(Vec<IssuedAid>, Instant),
}

/// Events sent from an Association to an Endpoint
#[derive(Debug)]
pub struct EndpointEvent(pub(crate) EndpointEventInner);

impl EndpointEvent {
    /// Construct an event that indicating that a `Association` will no longer emit events
    ///
    /// Useful for notifying an `Endpoint` that a `Association` has been destroyed outside of the
    /// usual state machine flow, e.g. when being dropped by the user.
    pub fn drained() -> Self {
        Self(EndpointEventInner::Drained)
    }

    /// Determine whether this is the last event a `Association` will emit
    ///
    /// Useful for determining when association-related event loop state can be freed.
    pub fn is_drained(&self) -> bool {
        self.0 == EndpointEventInner::Drained
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EndpointEventInner {
    /// The association has been drained
    Drained,
    /*// The association needs association identifiers
    NeedIdentifiers(Instant, u64),
    /// Stop routing Association ID for this sequence number to the Association
    /// When `bool == true`, a new Association ID will be issued to peer
    RetireAssociationId(Instant, u64, bool),*/
}

/// Protocol-level identifier for an Association.
///
/// Mainly useful for identifying this Association's packets on the wire with tools like Wireshark.
pub type AssociationId = u32;

/// Explicit congestion notification codepoint
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum EcnCodepoint {
    #[doc(hidden)]
    Ect0 = 0b10,
    #[doc(hidden)]
    Ect1 = 0b01,
    #[doc(hidden)]
    Ce = 0b11,
}

impl EcnCodepoint {
    /// Create new object from the given bits
    pub fn from_bits(x: u8) -> Option<Self> {
        use self::EcnCodepoint::*;
        Some(match x & 0b11 {
            0b10 => Ect0,
            0b01 => Ect1,
            0b11 => Ce,
            _ => {
                return None;
            }
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct IssuedAid {
    pub sequence: u64,
    pub id: AssociationId,
}
