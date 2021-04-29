use std::fmt;

// chunkType is an enum for SCTP Chunk Type field
// This field identifies the type of information contained in the
// Chunk Value field.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub(crate) enum ChunkType {
    PayloadData = 0,
    Init = 1,
    InitAck = 2,
    Sack = 3,
    Heartbeat = 4,
    HeartbeatAck = 5,
    Abort = 6,
    Shutdown = 7,
    ShutdownAck = 8,
    Error = 9,
    CookieEcho = 10,
    CookieAck = 11,
    Cwr = 13,
    ShutdownComplete = 14,
    Reconfig = 130,
    ForwardTsn = 192,
    Unknown,
}

impl fmt::Display for ChunkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            ChunkType::PayloadData => "DATA",
            ChunkType::Init => "INIT",
            ChunkType::InitAck => "INIT-ACK",
            ChunkType::Sack => "SACK",
            ChunkType::Heartbeat => "HEARTBEAT",
            ChunkType::HeartbeatAck => "HEARTBEAT-ACK",
            ChunkType::Abort => "ABORT",
            ChunkType::Shutdown => "SHUTDOWN",
            ChunkType::ShutdownAck => "SHUTDOWN-ACK",
            ChunkType::Error => "ERROR",
            ChunkType::CookieEcho => "COOKIE-ECHO",
            ChunkType::CookieAck => "COOKIE-ACK",
            ChunkType::Cwr => "ECNE", // Explicit Congestion Notification Echo
            ChunkType::ShutdownComplete => "SHUTDOWN-COMPLETE",
            ChunkType::Reconfig => "RECONFIG", // Re-configuration
            ChunkType::ForwardTsn => "FORWARD-TSN",
            _ => "Unknown ChunkType",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for ChunkType {
    fn from(v: u8) -> ChunkType {
        match v {
            0 => ChunkType::PayloadData,
            1 => ChunkType::Init,
            2 => ChunkType::InitAck,
            3 => ChunkType::Sack,
            4 => ChunkType::Heartbeat,
            5 => ChunkType::HeartbeatAck,
            6 => ChunkType::Abort,
            7 => ChunkType::Shutdown,
            8 => ChunkType::ShutdownAck,
            9 => ChunkType::Error,
            10 => ChunkType::CookieEcho,
            11 => ChunkType::CookieAck,
            13 => ChunkType::Cwr,
            14 => ChunkType::ShutdownComplete,
            130 => ChunkType::Reconfig,
            192 => ChunkType::ForwardTsn,
            _ => ChunkType::Unknown,
        }
    }
}
