/// DataChannelConfig can be used to configure properties of the underlying
/// channel such as data reliability.
///
/// ## Specifications
///
/// * [W3C]
///
/// [W3C]: https://w3c.github.io/webrtc-pc/#dom-rtcdatachannelinit
#[derive(Default, Debug, Clone)]
pub struct RTCDataChannelInit {
    /// ordered indicates if data is allowed to be delivered out of order. The
    /// default value of true, guarantees that data will be delivered in order.
    pub ordered: Option<bool>,

    /// max_packet_life_time limits the time (in milliseconds) during which the
    /// channel will transmit or retransmit data if not acknowledged. This value
    /// may be clamped if it exceeds the maximum value supported.
    pub max_packet_life_time: Option<u16>,

    /// max_retransmits limits the number of times a channel will retransmit data
    /// if not successfully delivered. This value may be clamped if it exceeds
    /// the maximum value supported.
    pub max_retransmits: Option<u16>,

    /// protocol describes the subprotocol name used for this channel.
    pub protocol: Option<String>,

    /// negotiated describes if the data channel is created by the local peer or
    /// the remote peer. The default value of None tells the user agent to
    /// announce the channel in-band and instruct the other peer to dispatch a
    /// corresponding DataChannel. If set to Some(id), it is up to the application
    /// to negotiate the channel and create an DataChannel with the same id
    /// at the other peer.
    pub negotiated: Option<u16>,
}
